//! The `agentos` command line.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

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
    /// Start a new execution: copy the repo, run the agent, write a patch
    Run {
        /// The repository to work on (it is copied, never changed)
        #[arg(long)]
        repo: PathBuf,
        /// Policy file (see policies/)
        #[arg(long)]
        policy: PathBuf,
        /// Which agent drives the run
        #[arg(long, value_enum, default_value = "scripted")]
        agent: AgentKind,
        /// For --agent scripted: the TOML script of tool calls
        #[arg(long)]
        script: Option<PathBuf>,
        /// Print only the execution id
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
    let state_dir = cli.state_dir.clone().unwrap_or_else(runtime::default_state_dir);
    match cli.cmd {
        Cmd::Run { repo, policy, agent, script, quiet } => {
            let opts = RunOptions { state_dir, repo, policy, verbose: !quiet };
            let summary = match agent {
                AgentKind::Scripted => {
                    let Some(script) = script else { bail!("--agent scripted needs --script <file>") };
                    let mut a = ScriptedAgent::new(Script::load(&script)?);
                    runtime::start(&opts, &mut a)?
                }
            };
            if quiet {
                println!("{}", summary.id);
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
