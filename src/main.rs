//! The `agentos` command line.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use agentos::agent::llm::{LlmAgent, LlmConfig};
use agentos::agent::scripted::{Script, ScriptedAgent};
use agentos::runtime::{self, RunOptions};
use agentos::trace;
use agentos::workspace::{self, ExecDir};

#[derive(Parser)]
#[command(name = "agentos", version, about = "Run a coding agent's tool calls under a policy, with a durable event log")]
struct Cli {
    /// Where executions are stored [default: $AGENTOS_STATE_DIR, else ~/.local/share/agentos]
    #[arg(long, global = true)]
    state_dir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Start a new execution (copy the repo, run the agent, write a patch),
    /// or continue an interrupted one with --resume
    Run {
        /// The repository to work on (it is copied, never changed)
        #[arg(long, required_unless_present = "resume")]
        repo: Option<PathBuf>,
        /// Policy file (see policies/)
        #[arg(long, required_unless_present = "resume")]
        policy: Option<PathBuf>,
        /// Which agent drives the run
        #[arg(long, value_enum, default_value = "scripted")]
        agent: AgentKind,
        /// For --agent scripted: the TOML script of tool calls
        #[arg(long)]
        script: Option<PathBuf>,
        /// For --agent llm: the task, in words. The endpoint comes from
        /// AGENTOS_LLM_BASE_URL, AGENTOS_LLM_MODEL and AGENTOS_LLM_API_KEY
        #[arg(long)]
        task: Option<String>,
        /// Continue this interrupted execution: recover it, then go on
        #[arg(long, value_name = "ID", conflicts_with_all = ["repo", "policy", "script", "task"])]
        resume: Option<String>,
        /// Print only the execution id
        #[arg(long, short)]
        quiet: bool,
    },
    /// After a crash: settle the call that was cut off and repair the workspace
    Recover {
        id: String,
        #[arg(long, short)]
        quiet: bool,
    },
    /// List executions, or show one execution's state
    Status { id: Option<String> },
    /// Check this machine, a policy and a repository without starting anything
    Check {
        /// Policy file to check (and to find its programs on PATH)
        #[arg(long)]
        policy: Option<PathBuf>,
        /// Repository: shows what would be copied and what the policy leaves out
        #[arg(long)]
        repo: Option<PathBuf>,
    },
    /// Delete executions (log, workspace, checkpoints, patch). Nothing is deleted automatically
    Rm {
        /// Execution ids to delete
        ids: Vec<String>,
        /// Also every execution created more than this many days ago
        #[arg(long, value_name = "DAYS")]
        older_than: Option<u64>,
        /// Every execution in the state folder
        #[arg(long)]
        all: bool,
        /// Also delete executions that never finished (they could still be resumed)
        #[arg(long)]
        force: bool,
        /// Only say what would be deleted
        #[arg(long)]
        dry_run: bool,
    },
    /// Print an execution's tool calls (default: the latest execution)
    Trace {
        id: Option<String>,
        /// Print every event instead of one line per call
        #[arg(long)]
        events: bool,
        /// Print every event as one JSON object per line
        #[arg(long, conflicts_with = "events")]
        json: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum AgentKind {
    Scripted,
    Llm,
}

fn main() -> Result<()> {
    // The sandbox helper: the runtime starts this same binary again with a
    // hidden first argument (see sandbox/ns.rs). It is handled before clap,
    // so it never shows up in --help.
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 3 && args[1] == "__sandbox-init" {
        agentos::sandbox::ns::helper_main(&args[2]);
    }
    let cli = Cli::parse();
    if matches!(cli.cmd, Cmd::Run { .. } | Cmd::Check { .. }) {
        enter_delegated_scope();
    }
    if matches!(cli.cmd, Cmd::Run { .. } | Cmd::Recover { .. }) {
        agentos::interrupt::install();
    }
    // Made absolute once, here: later steps run helper programs (`diff`,
    // `cp`) from other folders, where a relative path means something else
    // (bug B-011).
    let state_dir = std::path::absolute(cli.state_dir.clone().unwrap_or_else(runtime::default_state_dir))?;
    match cli.cmd {
        Cmd::Run { resume: Some(id), quiet, .. } => {
            let summary = match runtime::agent_kind(&state_dir, &id)?.as_str() {
                "scripted" => {
                    let script = Script::load(&ExecDir::new(&state_dir, &id).script())?;
                    runtime::resume(&state_dir, &id, &mut ScriptedAgent::new(script), !quiet)?
                }
                "llm" => {
                    let text = std::fs::read_to_string(ExecDir::new(&state_dir, &id).llm_config())?;
                    let mut cfg: LlmConfig = serde_json::from_str(&text)?;
                    // The key is never saved; take it from the environment again.
                    cfg.api_key = LlmConfig::from_env(String::new()).api_key;
                    cfg.check_reachable()?;
                    runtime::resume(&state_dir, &id, &mut LlmAgent::new(cfg), !quiet)?
                }
                other => bail!("execution {id} was started with agent `{other}`, which this build cannot resume"),
            };
            if quiet {
                println!("{}", summary.id);
            }
        }
        Cmd::Run { repo, policy, agent, script, task, quiet, resume: None } => {
            let (Some(repo), Some(policy)) = (repo, policy) else { bail!("--repo and --policy are required") };
            let opts = RunOptions { state_dir, repo, policy, verbose: !quiet };
            let summary = match agent {
                AgentKind::Scripted => {
                    let Some(script) = script else { bail!("--agent scripted needs --script <file>") };
                    let mut a = ScriptedAgent::new(Script::load(&script)?);
                    runtime::start(&opts, &mut a)?
                }
                AgentKind::Llm => {
                    let Some(task) = task else { bail!("--agent llm needs --task \"...\"") };
                    let cfg = LlmConfig::from_env(task);
                    cfg.check_reachable()?;
                    runtime::start(&opts, &mut LlmAgent::new(cfg))?
                }
            };
            if quiet {
                println!("{}", summary.id);
            }
        }
        Cmd::Recover { id, quiet } => {
            runtime::recover(&state_dir, &id, !quiet)?;
        }
        Cmd::Check { policy, repo } => {
            let problems = runtime::check(policy.as_deref(), repo.as_deref());
            if problems > 0 {
                bail!("{problems} problem(s) found; `agentos run` would not start");
            }
        }
        Cmd::Rm { ids, older_than, all, force, dry_run } => {
            if ids.is_empty() && older_than.is_none() && !all {
                bail!("say what to delete: execution ids, --older-than <days>, or --all");
            }
            let mut targets = ids.clone();
            for id in workspace::all_execution_dirs(&state_dir)? {
                let old = older_than.is_some_and(|days| workspace::age_in_days(&id).is_some_and(|age| age > days as i64));
                if (all || old) && !targets.contains(&id) {
                    targets.push(id);
                }
            }
            let verb = if dry_run { "would remove" } else { "removed" };
            let (mut removed, mut kept) = (0, 0);
            let mut named_and_kept = false;
            for id in &targets {
                let result = runtime::remove_execution(&state_dir, id, force, dry_run)?;
                match result {
                    runtime::Removal::Removed => println!("{verb} {id}"),
                    runtime::Removal::InUse => println!("kept {id}: an agentos process is working on it"),
                    runtime::Removal::Unfinished => println!(
                        "kept {id}: it never finished and can still be resumed (`agentos run --resume {id}`); --force deletes it anyway"
                    ),
                    runtime::Removal::NotFound => println!("no execution {id} in {}", state_dir.display()),
                }
                if result == runtime::Removal::Removed {
                    removed += 1;
                } else {
                    kept += 1;
                    named_and_kept |= ids.contains(id);
                }
            }
            println!("{removed} {verb}, {kept} kept or not found");
            // Naming an execution that then is not deleted is an error;
            // `--all` skipping a running one is not.
            if named_and_kept {
                bail!("not everything that was named could be deleted");
            }
        }
        Cmd::Status { id } => match id {
            Some(id) => {
                let dir = ExecDir::new(&state_dir, &id);
                let events = trace::load(&dir)?;
                println!("{id}  {}", trace::execution_state(&dir, &events));
            }
            None => {
                let ids = workspace::list_executions(&state_dir)?;
                if ids.is_empty() {
                    println!("no executions in {}", state_dir.display());
                }
                for id in ids {
                    let dir = ExecDir::new(&state_dir, &id);
                    let events = trace::load(&dir)?;
                    let calls = agentos::log::calls(&events).map(|c| c.len()).unwrap_or(0);
                    println!("{id}  {:<12} {calls} calls", trace::execution_state(&dir, &events));
                }
            }
        },
        Cmd::Trace { id, events, json } => {
            let id = match id {
                Some(id) => id,
                None => match workspace::list_executions(&state_dir)?.pop() {
                    Some(id) => id,
                    None => bail!("no executions in {}", state_dir.display()),
                },
            };
            let dir = ExecDir::new(&state_dir, &id);
            let evs = trace::load(&dir)?;
            let text = if json {
                trace::render_json(&evs)?
            } else if events {
                trace::render_events(&evs)?
            } else {
                trace::render_calls(&evs)?
            };
            print!("{text}");
        }
    }
    Ok(())
}

/// The sandbox needs a cgroup that systemd has handed over to this user
/// (D-013). If agentos was started without one, start it again inside one:
/// `systemd-run --user --scope -p Delegate=yes agentos …`, which is what the
/// user was told to type before (D-105).
///
/// `systemd-run --scope` does not stay around as a parent: it turns into
/// the command it was given, so signals, exit codes and `kill -9` reach the
/// same process as before. `AGENTOS_IN_SCOPE` marks the second start, so
/// this cannot loop; setting it by hand switches the whole thing off.
fn enter_delegated_scope() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    if std::env::var_os("AGENTOS_IN_SCOPE").is_some() || agentos::sandbox::cgroup::CgroupRoot::init().is_ok() {
        return;
    }
    let scope = ["--user", "--scope", "-q", "-p", "Delegate=yes", "--"];
    // Try with a command that does nothing first. If systemd cannot give a
    // scope (no user session, no systemd at all), carry on as before and
    // let the startup check explain; after `exec` there is no way back.
    let works = Command::new("systemd-run").args(scope).arg("true").stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status();
    if !matches!(works, Ok(s) if s.success()) {
        return;
    }
    let Ok(me) = std::env::current_exe() else { return };
    // A name of our own for the scope. Left to itself, systemd-run names it
    // after the process id, and this process keeps its id across `exec`:
    // if agentos was itself started by `systemd-run --scope`, the default
    // name is already taken by the scope it is in.
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let unit = format!("--unit=agentos-{}-{nanos}", std::process::id());
    // `exec` only returns if it failed; then the normal path goes on.
    let _ = Command::new("systemd-run")
        .arg(unit)
        .args(scope)
        .arg(me)
        .args(std::env::args_os().skip(1))
        .env("AGENTOS_IN_SCOPE", "1")
        .exec();
}
