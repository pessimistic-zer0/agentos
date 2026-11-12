//! Recovery: after the runtime was killed, work out what happened to the
//! call that was in flight, put the workspace into a known state, and write
//! the verdict to the log. Only then may the agent continue (design rule 5).
//!
//! The rules per tool (D-016):
//!
//! | Cut-off call | Verdict |
//! |---|---|
//! | no intent in the log | NOT APPLIED: the effect never started |
//! | `fs.read`, `fs.list` | NOT APPLIED: nothing to undo; the agent can ask again |
//! | `fs.write`, file has the new hash | APPLIED: take the missing checkpoint now |
//! | `fs.write`, file has the old hash | NOT APPLIED |
//! | `fs.write`, anything else | roll back to the last checkpoint; NOT APPLIED |
//! | `exec` | cannot be known: roll back to the last checkpoint; NOT APPLIED |
//!
//! Recovery can itself be killed (crash point P8). Every step is therefore
//! safe to repeat: the verdict record is written only after its work is done,
//! and doing the work twice gives the same result.

use std::io::Read;
use std::path::Path;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use nix::fcntl::{openat, OFlag, ResolveFlag};
use nix::sys::stat::Mode;
use nix::unistd::{unlinkat, UnlinkatFlags};
use serde_json::Value;

use crate::blob::content_hash;
use crate::checkpoint;
use crate::faults;
use crate::log::{self, CallRecord, CallState, Event, EventBody, EventLog};
use crate::sandbox::{beneath, cgroup::ExecCgroup};
use crate::tools::LoggedCall;
use crate::workspace::ExecDir;

#[derive(Debug, Default)]
pub struct RecoveryReport {
    /// Nothing was cut off and nothing was left over.
    pub clean: bool,
    /// The execution had already finished; recovery did nothing.
    pub finished: bool,
    pub verdicts: Vec<Verdict>,
    pub orphan_checkpoints: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Verdict {
    pub call_id: u64,
    pub tool: &'static str,
    pub applied: bool,
    pub reason: String,
}

/// Recover one execution. The caller must hold the execution's lock.
pub fn recover(dir: &ExecDir, log: &mut EventLog) -> Result<RecoveryReport> {
    let start = Instant::now();
    let events = log.events()?;
    log::check_gap_free(&events)?;
    if events.iter().any(|e| matches!(e.body, EventBody::ExecutionFinished { .. })) {
        return Ok(RecoveryReport { clean: true, finished: true, ..Default::default() });
    }
    let mut named = log::named_checkpoints(&events);
    if named.is_empty() {
        bail!("execution {} was interrupted before its workspace was ready; start a new run", dir.id);
    }
    let calls = log::calls(&events)?;
    let pending: Vec<&CallRecord> = calls.iter().filter(|c| !c.state.is_terminal()).collect();

    // 1. Make sure nothing from the dead runtime is still running. A sandboxed
    //    command normally dies with the runtime (PR_SET_PDEATHSIG), but that
    //    takes a moment, and it must not write to /work while we repair it.
    for c in &pending {
        if let CallState::IntentLogged { plan } = &c.state {
            if let Some(path) = plan.get("cgroup").and_then(Value::as_str) {
                kill_leftover_cgroup(Path::new(path))?;
            }
        }
    }

    // 2. Remove half-made copies, and checkpoints the log does not name
    //    (a crash after the copy but before the COMPLETED record: D-011).
    checkpoint::remove_leftovers(&dir.work(), &dir.checkpoints())?;
    let mut orphans = Vec::new();
    for cp in checkpoint::on_disk(&dir.checkpoints())? {
        if !named.contains(&cp) {
            std::fs::remove_dir_all(dir.checkpoints().join(&cp))?;
            orphans.push(cp);
        }
    }
    if pending.is_empty() && orphans.is_empty() {
        return Ok(RecoveryReport { clean: true, ..Default::default() });
    }

    log.append(EventBody::RecoveryStarted {
        pending: pending.iter().map(|c| c.call_id).collect(),
        orphan_checkpoints: orphans.clone(),
    })?;
    faults::point("P8", None);

    // 3. Settle each cut-off call, oldest first. (There is normally one: the
    //    runtime handles one call at a time.)
    let mut verdicts = Vec::new();
    for c in &pending {
        let last = named.last().expect("checked non-empty").clone();
        let v = reconcile(dir, c, &last, &named)?;
        faults::point("P8", None);
        log.append(EventBody::ToolReconciled {
            call_id: c.call_id,
            applied: v.applied,
            reason: v.reason.clone(),
            checkpoint: v.checkpoint.clone(),
            restored: v.restored.clone(),
        })?;
        faults::point("P8", None);
        if let Some(cp) = &v.checkpoint {
            named.push(cp.clone());
        }
        verdicts.push(Verdict { call_id: c.call_id, tool: c.call.name(), applied: v.applied, reason: v.reason });
    }
    log.append(EventBody::RecoveryCompleted { reconciled: verdicts.len() as u64, duration_ms: start.elapsed().as_millis() as u64 })?;
    Ok(RecoveryReport { clean: false, finished: false, verdicts, orphan_checkpoints: orphans })
}

struct Outcome {
    applied: bool,
    reason: String,
    checkpoint: Option<String>,
    restored: Option<String>,
}

fn not_applied(reason: &str) -> Outcome {
    Outcome { applied: false, reason: reason.into(), checkpoint: None, restored: None }
}

fn reconcile(dir: &ExecDir, c: &CallRecord, last: &str, named: &[String]) -> Result<Outcome> {
    let plan = match &c.state {
        CallState::Requested => return Ok(not_applied("no intent was recorded, so the effect never started")),
        CallState::IntentLogged { plan } => plan,
        other => bail!("call {} is in state {other:?}, which needs no recovery", c.call_id),
    };
    if plan.get("prepare_failed").is_some() {
        return Ok(not_applied("the call had failed before any effect"));
    }
    let rollback = |why: &str| -> Result<Outcome> {
        checkpoint::restore(&dir.work(), &dir.checkpoints().join(last))?;
        Ok(Outcome { applied: false, reason: format!("{why}; workspace rolled back to {last}"), checkpoint: None, restored: Some(last.to_string()) })
    };
    match &c.call {
        LoggedCall::FsRead { .. } | LoggedCall::FsList { .. } => Ok(not_applied("read-only call; nothing to undo")),
        LoggedCall::Exec { .. } => rollback("a command's effects cannot be known after a crash"),
        LoggedCall::FsWrite { .. } => {
            let field = |k: &str| plan.get(k).and_then(Value::as_str).map(str::to_string);
            let path = field("path").context("fs.write intent has no path")?;
            let new_hash = field("new_hash").context("fs.write intent has no new_hash")?;
            let pre_hash = field("pre_hash");
            let temp = field("temp").context("fs.write intent has no temp name")?;
            match inspect_write(&dir.work(), &path, &temp)? {
                // The file has the new content: the rename happened.
                Some(Some(h)) if h == new_hash => {
                    let name = checkpoint::name(named.iter().filter_map(|n| checkpoint::number(n)).max().unwrap_or(0) + 1);
                    checkpoint::create(&dir.work(), &dir.checkpoints(), &name)?;
                    Ok(Outcome {
                        applied: true,
                        reason: format!("file has the new content; checkpoint {name} taken"),
                        checkpoint: Some(name),
                        restored: None,
                    })
                }
                // The file is as it was before the call (or still absent).
                Some(now) if now == pre_hash => Ok(not_applied("file still has its old content; the write did not happen")),
                Some(_) => rollback("file matches neither the old nor the new content"),
                None => rollback("the file's folder could not be inspected safely"),
            }
        }
    }
}

/// Delete the write's leftover temp file, then hash the target.
/// `Some(Some(hash))`: a regular file. `Some(None)`: no file. `None`: the
/// path cannot be reached beneath the workspace or is not a regular file.
fn inspect_write(work: &Path, rel: &str, temp: &str) -> Result<Option<Option<String>>> {
    let (parent_rel, name) = match rel.rsplit_once('/') {
        Some((d, n)) => (d, n),
        None => ("", rel),
    };
    let root = beneath::open_root(work)?;
    let Ok(parent) = beneath::open_beneath(&root, parent_rel, OFlag::O_PATH | OFlag::O_DIRECTORY, ResolveFlag::empty()) else {
        return Ok(None);
    };
    // A crash during the temp-file write leaves it behind (m3-rename-kill).
    let _ = unlinkat(&parent, temp, UnlinkatFlags::NoRemoveDir);
    match openat(&parent, name, OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK, Mode::empty()) {
        Err(nix::errno::Errno::ENOENT) => Ok(Some(None)),
        Err(_) => Ok(None),
        Ok(fd) => {
            let mut f = std::fs::File::from(fd);
            if !f.metadata()?.is_file() {
                return Ok(None);
            }
            let mut bytes = Vec::new();
            f.read_to_end(&mut bytes)?;
            Ok(Some(Some(content_hash(&bytes))))
        }
    }
}

/// Kill and remove a cgroup left behind by the crashed runtime, if it is
/// still there. A cgroup cannot disappear while it holds processes, so "not
/// there" means "nothing is running".
fn kill_leftover_cgroup(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    let cg = ExecCgroup::open(path.to_path_buf());
    // destroy() kills, waits until the group is empty, then removes it. If
    // systemd removes the emptied scope first, the folder vanishing is fine.
    match cg.destroy() {
        Ok(()) => Ok(()),
        Err(_) if !path.exists() => Ok(()),
        Err(e) if !cg.is_populated() => {
            let _ = e; // empty but not removable by us: harmless
            Ok(())
        }
        Err(e) => Err(e),
    }
}

/// The results the agent has seen so far, one per call, for rebuilding the
/// agent's position after a restart (ADR 0005).
pub fn history(events: &[Event]) -> Result<Vec<crate::tools::ToolResult>> {
    let mut out = Vec::new();
    for c in log::calls(events)? {
        if let Some(outcome) = c.outcome() {
            out.push(crate::tools::ToolResult { call_id: c.call_id, outcome, output: None });
        }
    }
    Ok(out)
}
