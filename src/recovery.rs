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
//! | `fs.write`, file has the old hash | NOT APPLIED (rolled back first if the write was going to make folders) |
//! | `fs.write`, anything else | roll back to the last checkpoint; NOT APPLIED |
//! | `exec` | cannot be known: roll back to the last checkpoint; NOT APPLIED |
//! | `ext.call`, the service answers 2xx to the same request and key | APPLIED (it was done before, or it is done now; never twice) |
//! | `ext.call`, the service answers 4xx | NOT APPLIED: the service refused it |
//! | `ext.call`, no answer | **stays uncertain**: recovery stops with an error and is run again later |
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
use crate::tools::ext::{self, Answer};
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
            checkpoint::remove_tree(&dir.checkpoints().join(&cp))?;
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
        LoggedCall::ExtCall { service, payload_hash, .. } => reconcile_external(dir, c.call_id, service, payload_hash, plan),
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
                    // The policy saved with the execution says which paths
                    // are caches and stay out of checkpoints (D-118).
                    let (policy, _) = crate::policy::Policy::load(&dir.policy()).context("reading the execution's saved policy")?;
                    checkpoint::create(&dir.work(), &dir.checkpoints(), &name, |rel| policy.scratch(rel))?;
                    Ok(Outcome {
                        applied: true,
                        reason: format!("file has the new content; checkpoint {name} taken"),
                        checkpoint: Some(name),
                        restored: None,
                    })
                }
                // The file is as it was before the call (or still absent).
                // The write was going to make folders first (D-084). The file
                // is absent, but some of those folders may exist: only the
                // checkpoint says what the workspace looked like before.
                Some(now) if now == pre_hash && plan.get("created_dirs").and_then(Value::as_array).is_some_and(|d| !d.is_empty()) => {
                    rollback("the write did not happen, but folders may have been made for it")
                }
                Some(now) if now == pre_hash => Ok(not_applied("file still has its old content; the write did not happen")),
                Some(_) => rollback("file matches neither the old nor the new content"),
                None => rollback("the file's folder could not be inspected safely"),
            }
        }
    }
}

/// Settle an `ext.call` that has an intent and no ending (D-078).
///
/// A workspace can be rolled back; a request that may have reached another
/// program cannot. So this call is settled *forwards*: the same request is
/// sent again with the same idempotency key, both taken from the intent.
/// The service applies a key at most once, so whether the first request
/// arrived or not, afterwards the effect has happened exactly once.
///
/// If the service still gives no answer, nothing is decided and nothing is
/// written: the call stays uncertain, the execution stays interrupted, and
/// recovery has to be run again. Guessing here would break design rule 5.
fn reconcile_external(dir: &ExecDir, call_id: u64, service: &str, payload_hash: &str, plan: &Value) -> Result<Outcome> {
    let text = |k: &str| plan.get(k).and_then(Value::as_str).with_context(|| format!("ext.call intent has no {k}"));
    let number = |k: &str| plan.get(k).and_then(Value::as_u64).with_context(|| format!("ext.call intent has no {k}"));
    let (url, key) = (text("url")?, text("key")?);
    let timeout = std::time::Duration::from_millis(number("timeout_ms")?);
    let attempts = number("attempts")? as u32;
    let payload = crate::blob::BlobStore::new(&dir.blobs())
        .get(payload_hash)
        .with_context(|| format!("the payload of call {call_id} is not in the blob store"))?;
    let payload = String::from_utf8(payload).context("the stored payload is not text")?;
    let (answer, sent) = ext::send_until_answered(url, key, &payload, timeout, attempts, 64 * 1024);
    match answer {
        Answer::Applied { status, body } => Ok(Outcome {
            applied: true,
            reason: format!(
                "service `{service}` answered HTTP {status} to the same request sent again with idempotency key {key}; answer: {}",
                body.chars().take(200).collect::<String>()
            ),
            checkpoint: None,
            restored: None,
        }),
        Answer::Refused { status, .. } => Ok(not_applied(&format!("service `{service}` refused the request (HTTP {status}); it did nothing"))),
        Answer::Unknown { why } => bail!(
            "call #{call_id} to external service `{service}` is still UNCERTAIN: no answer after {sent} request(s) ({why}). \
             Nothing was decided. Run `agentos recover {}` again when the service is reachable.",
            dir.id
        ),
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
/// agent after a restart (ADR 0005). Outputs of completed calls are read
/// back from the blob store; a reconciled call has none.
pub fn history(events: &[Event], blobs: &crate::blob::BlobStore) -> Result<Vec<crate::tools::ToolResult>> {
    let mut out = Vec::new();
    for c in log::calls(events)? {
        let Some(outcome) = c.outcome() else { continue };
        let output = match &c.state {
            CallState::Completed { output, .. } => blobs.get(output).ok().and_then(|b| serde_json::from_slice(&b).ok()),
            _ => None,
        };
        out.push(crate::tools::ToolResult { call_id: c.call_id, outcome, output });
    }
    Ok(out)
}
