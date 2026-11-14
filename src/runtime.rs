//! Starting and driving an execution: set up the folders, copy the workspace,
//! then loop "ask the agent, pass the call to the gateway, hand back the
//! result" until the agent finishes or the call budget runs out.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::agent::{Action, Agent};
use crate::blob::content_hash;
use crate::checkpoint;
use crate::gateway::{Gateway, SandboxCtx};
use crate::log::{EventBody, EventLog};
use crate::policy::Policy;
use crate::tools::exec::Programs;
use crate::tools::{ToolOutcome, ToolResult};
use crate::workspace::{self, ExecDir};

pub struct RunOptions {
    pub state_dir: PathBuf,
    pub repo: PathBuf,
    pub policy: PathBuf,
    /// Print one line per call while running.
    pub verbose: bool,
}

pub struct RunSummary {
    pub id: String,
    pub status: String,
    pub calls: u64,
    pub patch: PathBuf,
}

/// How long to wait for the lock when someone else holds it (see below).
const LOCK_WAIT: std::time::Duration = std::time::Duration::from_secs(3);

/// Hold an exclusive lock on the execution, so two runtimes never work on the
/// same one at once. The kernel drops the lock when the process dies, even on
/// `kill -9`, so a crashed run never leaves a stale lock behind.
///
/// One catch (bug B-008): the lock belongs to the open file, and a child
/// process gets a copy of every open file at `fork`. The copy is closed when
/// the child becomes another program (`exec`), but for the moment in between
/// the child holds the lock too. If the runtime is killed in that moment, the
/// lock outlives it until the child is next given CPU time and exits or
/// execs. So "held by someone else" is retried for a short while before it is
/// reported. A runtime that is really alive still holds the lock afterwards.
pub fn lock_execution(dir: &ExecDir) -> Result<File> {
    let f = File::options().create(true).truncate(false).write(true).open(dir.lock())?;
    let deadline = std::time::Instant::now() + LOCK_WAIT;
    loop {
        match f.try_lock() {
            Ok(()) => return Ok(f),
            Err(std::fs::TryLockError::WouldBlock) if std::time::Instant::now() < deadline => {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            Err(std::fs::TryLockError::WouldBlock) => bail!("execution {} is in use by another agentos process", dir.id),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
    }
}

/// Is some process holding the execution's lock right now?
pub fn is_locked(dir: &ExecDir) -> bool {
    match File::options().write(true).open(dir.lock()) {
        Ok(f) => matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        Err(_) => false,
    }
}

/// Check the machine can run the sandbox, and set up the cgroup subtree.
/// Stops with one message listing everything that is missing.
pub fn prepare_sandbox() -> Result<SandboxCtx> {
    let problems = crate::sandbox::check_host();
    if !problems.is_empty() {
        bail!("the AgentOS sandbox cannot start:\n  - {}", problems.join("\n  - "));
    }
    let cgroups = crate::sandbox::cgroup::CgroupRoot::init()?;
    let helper = std::env::current_exe().context("finding the agentos binary")?;
    Ok(SandboxCtx { cgroups, helper })
}

pub fn start(opts: &RunOptions, agent: &mut dyn Agent) -> Result<RunSummary> {
    let (mut gw, _lock) = create(opts, agent)?;
    drive(&mut gw, agent, None, 0, opts.verbose)
}

/// Set up a new execution (folder, log, workspace copy, base checkpoint) and
/// return its gateway and the lock that must be held while it is used.
/// `start` drives an agent on it; the benchmarks call the gateway directly.
pub fn create(opts: &RunOptions, agent: &dyn Agent) -> Result<(Gateway, File)> {
    // The user's own arguments are checked first, so a mistyped path is
    // reported as that and not hidden behind a later, unrelated problem.
    let repo = std::fs::canonicalize(&opts.repo).with_context(|| format!("--repo {}", opts.repo.display()))?;
    if !repo.is_dir() {
        bail!("--repo {} is not a folder", opts.repo.display());
    }
    let (policy, policy_text) = Policy::load(&opts.policy)?;
    let host_path = std::env::var("PATH").unwrap_or_default();
    let programs = Programs::resolve(&policy, &host_path)?;
    let sandbox = prepare_sandbox()?;

    let id = workspace::new_execution_id();
    let dir = ExecDir::new(&opts.state_dir, &id);
    std::fs::create_dir_all(&dir.root).with_context(|| format!("creating {}", dir.root.display()))?;
    let lock = lock_execution(&dir)?;
    std::fs::write(dir.policy(), &policy_text)?;
    let mut log = EventLog::create(&dir.log())?;
    log.append(EventBody::ExecutionCreated {
        execution_id: id.clone(),
        repo: repo.display().to_string(),
        policy_name: policy.name.clone(),
        policy_hash: content_hash(policy_text.as_bytes()),
        agent: agent.kind().into(),
        task: agent.task(),
    })?;
    if opts.verbose {
        println!("{id}  RUNNING  policy {}  agent {}", policy.name, agent.kind());
    }

    let report = workspace::create(&repo, &dir.work(), &policy)?;
    let base = checkpoint::name(0);
    checkpoint::create(&dir.work(), &dir.checkpoints(), &base)?;
    log.append(EventBody::WorkspaceCreated {
        files_copied: report.files_copied,
        denied: report.denied.clone(),
        checkpoint: base.clone(),
    })?;
    if opts.verbose && !report.denied.is_empty() {
        println!("  workspace: {} files copied, left out by deny rules: {}", report.files_copied, report.denied.join(", "));
    }

    // Whatever the agent needs to be rebuilt after a crash (the script).
    agent.persist(&dir)?;
    Ok((Gateway::new(dir.clone(), log, policy, programs, sandbox, 1, base), lock))
}

/// Run recovery on an interrupted execution and print what it decided.
pub fn recover(state_dir: &Path, id: &str, verbose: bool) -> Result<crate::recovery::RecoveryReport> {
    let dir = ExecDir::new(state_dir, id);
    let mut log = EventLog::open(&dir.log())?;
    let _lock = lock_execution(&dir)?;
    let report = crate::recovery::recover(&dir, &mut log)?;
    if verbose {
        print_recovery(id, &report);
    }
    Ok(report)
}

fn print_recovery(id: &str, report: &crate::recovery::RecoveryReport) {
    if report.finished {
        println!("{id}  already finished; nothing to recover");
    } else if report.clean {
        println!("{id}  no call was cut off; nothing to recover");
    } else {
        println!("{id}  RECOVERED");
        for cp in &report.orphan_checkpoints {
            println!("  removed checkpoint {cp}: the log does not name it");
        }
        for v in &report.verdicts {
            let verdict = if v.applied { "APPLIED" } else { "NOT APPLIED" };
            println!("  #{:<3} {:<8} {verdict}: {}", v.call_id, v.tool, v.reason);
        }
    }
}

/// The agent kind recorded when the execution was created.
pub fn agent_kind(state_dir: &Path, id: &str) -> Result<String> {
    let dir = ExecDir::new(state_dir, id);
    for ev in EventLog::open(&dir.log())?.events()? {
        if let EventBody::ExecutionCreated { agent, .. } = ev.body {
            return Ok(agent);
        }
    }
    bail!("execution {id} has no execution.created event")
}

/// Continue an interrupted execution: recover first (design rule 5), rebuild
/// the agent's position from the log, then carry on with the agent loop.
pub fn resume(state_dir: &Path, id: &str, agent: &mut dyn Agent, verbose: bool) -> Result<RunSummary> {
    let dir = ExecDir::new(state_dir, id);
    let mut log = EventLog::open(&dir.log())?;
    let _lock = lock_execution(&dir)?;
    let report = crate::recovery::recover(&dir, &mut log)?;
    if verbose {
        print_recovery(id, &report);
    }
    let events = log.events()?;
    if report.finished {
        let calls = crate::log::calls(&events)?.len() as u64;
        return Ok(RunSummary { id: id.into(), status: "completed".into(), calls, patch: dir.patch() });
    }
    // The policy this execution started with, not whatever the file says now.
    let (policy, _) = Policy::load(&dir.policy())?;
    let host_path = std::env::var("PATH").unwrap_or_default();
    let programs = Programs::resolve(&policy, &host_path)?;
    let sandbox = prepare_sandbox()?;
    let calls = crate::log::calls(&events)?;
    let next_call_id = calls.iter().map(|c| c.call_id).max().unwrap_or(0) + 1;
    let last_cp = crate::log::named_checkpoints(&events).pop().context("no checkpoint in the log")?;
    let history = crate::recovery::history(&events, &crate::blob::BlobStore::new(&dir.blobs()))?;
    agent.restore(&history, &events)?;
    let mut gw = Gateway::new(dir.clone(), log, policy, programs, sandbox, next_call_id, last_cp);
    if verbose {
        println!("{id}  RESUMING after {} calls", history.len());
    }
    drive(&mut gw, agent, history.last().cloned(), history.len() as u64, verbose)
}

/// The agent loop. `last` is the result to hand the agent first, and
/// `calls_so_far` the calls already made (both used when resuming).
pub fn drive(gw: &mut Gateway, agent: &mut dyn Agent, mut last: Option<ToolResult>, calls_so_far: u64, verbose: bool) -> Result<RunSummary> {
    let max_calls = gw.policy.limits.max_tool_calls;
    let mut calls = calls_so_far;
    let (status, summary) = loop {
        let action = agent.next_action(last.as_ref());
        // Store the conversation before acting on it, so the log always
        // holds the message that led to a call (ADR 0005).
        for message in agent.take_messages() {
            gw.log.append(EventBody::AgentMessage { message })?;
        }
        match action? {
            Action::Finish { summary } => break ("completed".to_string(), summary),
            Action::GiveUp { reason } => break ("stopped".to_string(), reason),
            Action::Call(call) => {
                if calls >= max_calls {
                    break ("stopped".to_string(), format!("tool call budget of {max_calls} used up"));
                }
                let logged = call.logged();
                let result = gw.handle(call)?;
                calls += 1;
                if verbose {
                    print_call_line(result.call_id, logged.name(), &logged.describe(), &result);
                }
                last = Some(result);
            }
        }
    };
    finish(gw, &status, &summary, verbose)?;
    Ok(RunSummary { id: gw.dir.id.clone(), status, calls, patch: gw.dir.patch() })
}

fn finish(gw: &mut Gateway, status: &str, summary: &str, verbose: bool) -> Result<()> {
    let base = gw.dir.checkpoints().join(checkpoint::name(0));
    let changed = checkpoint::write_patch(&base, &gw.dir.work(), &gw.dir.patch())?;
    gw.log.append(EventBody::ExecutionFinished {
        status: status.into(),
        summary: summary.into(),
        patch: Some(gw.dir.patch().display().to_string()),
    })?;
    if verbose {
        let what = if changed { "patch" } else { "no changes; empty patch" };
        println!("{}  {}  {}\n  {what}: {}", gw.dir.id, status.to_uppercase(), truncate(summary, 200), gw.dir.patch().display());
    }
    Ok(())
}

pub fn print_call_line(call_id: u64, tool: &str, args: &str, r: &ToolResult) {
    let args = truncate(args, 40);
    let (verdict, detail) = match &r.outcome {
        ToolOutcome::Ok => ("allow", r.output.as_ref().map(|o| o.summary()).unwrap_or_default()),
        ToolOutcome::Denied { rule } => ("DENY", rule.clone()),
        ToolOutcome::Failed { error } => ("allow", format!("FAILED: {}", truncate(error, 60))),
        ToolOutcome::NotApplied { reason } => ("allow", format!("NOT APPLIED: {reason}")),
    };
    println!("  #{call_id:<3} {tool:<8} {args:<40} {verdict:<5}  {detail}");
}

pub fn truncate(s: &str, n: usize) -> String {
    let one_line = s.replace('\n', "\\n");
    if one_line.chars().count() <= n {
        one_line
    } else {
        let cut: String = one_line.chars().take(n.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

/// Default state folder: `$AGENTOS_STATE_DIR`, else `$XDG_DATA_HOME/agentos`,
/// else `~/.local/share/agentos`.
pub fn default_state_dir() -> PathBuf {
    if let Some(d) = std::env::var_os("AGENTOS_STATE_DIR") {
        return PathBuf::from(d);
    }
    if let Some(d) = std::env::var_os("XDG_DATA_HOME") {
        return Path::new(&d).join("agentos");
    }
    let home = std::env::var_os("HOME").unwrap_or_else(|| ".".into());
    Path::new(&home).join(".local/share/agentos")
}
