//! Starting and driving an execution: set up the folders, copy the workspace,
//! then loop "ask the agent, pass the call to the gateway, hand back the
//! result" until the agent finishes or the call budget runs out.

use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::agent::{Action, Agent};
use crate::blob::content_hash;
use crate::checkpoint;
use crate::gateway::Gateway;
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

/// Hold an exclusive lock on the execution, so two runtimes never work on the
/// same one at once. The kernel drops the lock when the process dies, even on
/// `kill -9`, so a crashed run never leaves a stale lock behind.
pub fn lock_execution(dir: &ExecDir) -> Result<File> {
    let f = File::options().create(true).truncate(false).write(true).open(dir.lock())?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(std::fs::TryLockError::WouldBlock) => bail!("execution {} is in use by another agentos process", dir.id),
        Err(std::fs::TryLockError::Error(e)) => Err(e.into()),
    }
}

/// Is some process holding the execution's lock right now?
pub fn is_locked(dir: &ExecDir) -> bool {
    match File::options().write(true).open(dir.lock()) {
        Ok(f) => matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock)),
        Err(_) => false,
    }
}

pub fn start(opts: &RunOptions, agent: &mut dyn Agent) -> Result<RunSummary> {
    let (policy, policy_text) = Policy::load(&opts.policy)?;
    let host_path = std::env::var("PATH").unwrap_or_default();
    let programs = Programs::resolve(&policy, &host_path)?;
    let repo = std::fs::canonicalize(&opts.repo).with_context(|| format!("repo {}", opts.repo.display()))?;

    let id = workspace::new_execution_id();
    let dir = ExecDir::new(&opts.state_dir, &id);
    std::fs::create_dir_all(&dir.root).with_context(|| format!("creating {}", dir.root.display()))?;
    let _lock = lock_execution(&dir)?;
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
        checkpoint: base,
    })?;
    if opts.verbose && !report.denied.is_empty() {
        println!("  workspace: {} files copied, left out by deny rules: {}", report.files_copied, report.denied.join(", "));
    }

    let mut gw = Gateway::new(dir.clone(), log, policy, programs, 1);
    drive(&mut gw, agent, None, opts.verbose)
}

/// The agent loop. `last` is the result to hand the agent first (used when
/// resuming).
pub fn drive(gw: &mut Gateway, agent: &mut dyn Agent, mut last: Option<ToolResult>, verbose: bool) -> Result<RunSummary> {
    let max_calls = gw.policy.limits.max_tool_calls;
    let mut calls = 0u64;
    let (status, summary) = loop {
        match agent.next_action(last.as_ref())? {
            Action::Finish { summary } => break ("completed".to_string(), summary),
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
        println!("{}  {}  {summary}\n  {what}: {}", gw.dir.id, status.to_uppercase(), gw.dir.patch().display());
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
