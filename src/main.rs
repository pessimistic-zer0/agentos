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
    if args.len() == 2 && args[1] == agentos::sandbox::ns::PROBE_ARG {
        agentos::sandbox::ns::probe_main();
    }
    let cli = Cli::parse();
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
