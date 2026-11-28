//! The `norn` command line.

use std::path::PathBuf;

use anyhow::{bail, Result};
use clap::{Parser, Subcommand};

use norn::agent::llm::{LlmAgent, LlmConfig};
use norn::agent::scripted::{Script, ScriptedAgent};
use norn::runtime::{self, RunOptions};
use norn::trace;
use norn::workspace::{self, ExecDir};

#[derive(Parser)]
#[command(name = "norn", version, about = "Run a coding agent's tool calls under a policy, with a durable event log")]
struct Cli {
    /// Where executions are stored [default: $NORN_STATE_DIR, else ~/.local/share/norn]
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
        /// NORN_LLM_BASE_URL, NORN_LLM_MODEL and NORN_LLM_API_KEY
        #[arg(long)]
        task: Option<String>,
        /// Continue this interrupted execution: recover it, then go on
        #[arg(long, value_name = "ID", conflicts_with_all = ["repo", "policy", "script", "task"])]
        resume: Option<String>,
        /// Print only the execution id
        #[arg(long, short)]
        quiet: bool,
    },
    /// Serve the tools to an MCP client (Claude Code, Cursor, …) over standard
    /// input and output. Continues the unfinished MCP execution of this
    /// repository if there is one; `norn finish` ends it
    Mcp {
        /// The repository to work on (it is copied, never changed)
        #[arg(long)]
        repo: PathBuf,
        /// Policy file (see policies/)
        #[arg(long)]
        policy: PathBuf,
    },
    /// Approve a call that is waiting for a person's yes (`[approvals]` in
    /// the policy). Without an approval id: list what is waiting, with the
    /// change each write would make
    Approve {
        /// The execution
        id: String,
        /// The approval id from the list (or from the agent's message)
        approval: Option<String>,
        /// How long the yes stays usable, in minutes; it is used once
        #[arg(long, default_value_t = 60)]
        minutes: u64,
    },
    /// Refuse a call that is waiting for approval; the same call is then refused every time
    Deny { id: String, approval: String },
    /// End an execution nobody is driving (one made by `norn mcp`): settle a
    /// cut-off call, then write its patch. Then read the patch and `norn apply`
    Finish { id: String },
    /// After a crash: settle the call that was cut off and repair the workspace
    Recover {
        id: String,
        #[arg(long, short)]
        quiet: bool,
    },
    /// List executions, or show one execution's state
    Status { id: Option<String> },
    /// Copy the result of a finished execution into its repository: every
    /// change, also the ones a patch cannot carry (binary and empty files,
    /// modes). Refuses if the repository changed since the run copied it
    Apply {
        id: String,
        /// Another copy of the repository to apply to [default: the one the run was started on]
        #[arg(long, value_name = "FOLDER")]
        to: Option<PathBuf>,
        /// Only list what would change
        #[arg(long)]
        dry_run: bool,
    },
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
    /// Run an execution's agent again, answering every call from the
    /// recording; nothing is run. Stops at the first call that differs
    Replay {
        id: String,
        /// For a scripted run: another script to replay instead of its own
        #[arg(long)]
        script: Option<PathBuf>,
        /// For an LLM run: another model (default: the recorded one)
        #[arg(long)]
        model: Option<String>,
        /// For an LLM run: another server (default: the recorded one)
        #[arg(long)]
        base_url: Option<String>,
    },
    /// Compare two executions: calls, results, time, tokens, patches
    Diff { a: String, b: String },
    /// Find calls across all executions
    Search {
        /// Only calls of this tool (fs.read, exec, ...)
        #[arg(long)]
        tool: Option<String>,
        /// Only calls whose path matches this glob ('src/**')
        #[arg(long)]
        path: Option<String>,
        /// done, denied, failed, waiting, uncertain or settled
        #[arg(long)]
        outcome: Option<String>,
        /// Only executions created on this day (YYYY-MM-DD, UTC) or later
        #[arg(long)]
        since: Option<String>,
    },
    /// Print an execution's tool calls (default: the latest execution)
    Trace {
        id: Option<String>,
        /// Print every event instead of one line per call
        #[arg(long, conflicts_with_all = ["json", "call", "security"])]
        events: bool,
        /// Print every event as one JSON object per line
        #[arg(long, conflicts_with_all = ["call", "security"])]
        json: bool,
        /// Everything recorded about one call: arguments, policy, plan, output
        #[arg(long, value_name = "N", conflicts_with = "security")]
        call: Option<u64>,
        /// With --call: print stored texts whole instead of their first lines
        #[arg(long, requires = "call")]
        full: bool,
        /// Denials, undone changes, network, approvals, calls cut off by a stop
        #[arg(long, conflicts_with_all = ["graph", "diff"])]
        security: bool,
        /// What asked for each call, approvals, stops and resumes; `--graph=dot` for Graphviz
        #[arg(long, value_name = "FORM", num_args = 0..=1, default_missing_value = "tree", conflicts_with_all = ["events", "json", "call", "diff"])]
        graph: Option<GraphForm>,
        /// The files call N changed (a patch between its checkpoints); without N, the whole run
        #[arg(long, value_name = "N", num_args = 0..=1, conflicts_with_all = ["events", "json", "call", "security"])]
        diff: Option<Option<u64>>,
        /// The run as OpenTelemetry spans (OTLP JSON), to send to a collector yourself
        #[arg(long, conflicts_with_all = ["events", "json", "call", "security", "graph", "diff"])]
        otel: bool,
        /// Only calls the policy or a person refused
        #[arg(long, conflicts_with_all = ["events", "json", "call", "security", "graph", "diff", "otel"])]
        denied: bool,
        /// Only calls that failed, and commands that exited with an error
        #[arg(long, conflicts_with_all = ["events", "json", "call", "security", "graph", "diff", "otel"])]
        failed: bool,
        /// Only calls a stop cut off (uncertain, or settled by recovery)
        #[arg(long, conflicts_with_all = ["events", "json", "call", "security", "graph", "diff", "otel"])]
        uncertain: bool,
        /// Only calls of this tool (fs.read, exec, ...)
        #[arg(long, value_name = "TOOL", conflicts_with_all = ["events", "json", "call", "security", "graph", "diff", "otel"])]
        tool: Option<String>,
        /// Only calls whose path matches this glob ('src/**')
        #[arg(long, value_name = "GLOB", conflicts_with_all = ["events", "json", "call", "security", "graph", "diff", "otel"])]
        path: Option<String>,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
enum GraphForm {
    Tree,
    Dot,
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
        norn::sandbox::ns::helper_main(&args[2]);
    }
    let cli = Cli::parse();
    if matches!(cli.cmd, Cmd::Run { .. } | Cmd::Check { .. } | Cmd::Mcp { .. }) {
        enter_delegated_scope();
    }
    if matches!(cli.cmd, Cmd::Run { .. } | Cmd::Recover { .. } | Cmd::Mcp { .. } | Cmd::Finish { .. }) {
        norn::interrupt::install();
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
        Cmd::Mcp { repo, policy } => {
            // Before anything can print: standard output belongs to the
            // protocol from here on.
            let mut out = norn::mcp::take_stdout()?;
            let cfg = norn::mcp::Config { state_dir, repo: norn::mcp::canonical_repo(&repo)?, policy };
            let mut server = norn::mcp::Server::new(cfg)?;
            server.serve(&mut std::io::stdin().lock(), &mut out)?;
        }
        Cmd::Finish { id } => {
            runtime::finish_execution(&state_dir, &id, true)?;
        }
        Cmd::Approve { id, approval: None, .. } => {
            print!("{}", runtime::list_approvals(&state_dir, &id)?);
        }
        Cmd::Approve { id, approval: Some(a), minutes } => {
            let v = norn::approval::answer(&state_dir, &id, &a, true, minutes)?;
            println!("approved {a} for one run, by {}, for {minutes} minutes; the agent's next identical call runs it", v.by);
        }
        Cmd::Deny { id, approval } => {
            let v = norn::approval::answer(&state_dir, &id, &approval, false, 0)?;
            println!("refused {approval}, by {}; the agent is told so when it asks again", v.by);
        }
        Cmd::Recover { id, quiet } => {
            runtime::recover(&state_dir, &id, !quiet)?;
        }
        Cmd::Apply { id, to, dry_run } => {
            runtime::apply(&state_dir, &id, to.as_deref(), dry_run)?;
        }
        Cmd::Check { policy, repo } => {
            let problems = runtime::check(policy.as_deref(), repo.as_deref());
            if problems > 0 {
                bail!("{problems} problem(s) found; `norn run` would not start");
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
                    runtime::Removal::InUse => println!("kept {id}: a norn process is working on it"),
                    runtime::Removal::Unfinished => println!(
                        "kept {id}: it never finished and can still be continued (`norn run --resume {id}`, or `norn mcp` for an MCP execution); --force deletes it anyway"
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
                    let calls = norn::log::calls(&events).map(|c| c.len()).unwrap_or(0);
                    println!("{id}  {:<12} {calls} calls", trace::execution_state(&dir, &events));
                }
            }
        },
        Cmd::Replay { id, script, model, base_url } => {
            let dir = ExecDir::new(&state_dir, &id);
            let report = match runtime::agent_kind(&state_dir, &id)?.as_str() {
                "scripted" => {
                    if model.is_some() || base_url.is_some() {
                        bail!("execution {id} was run by a script: --model and --base-url are for LLM runs; use --script");
                    }
                    let script = Script::load(&script.unwrap_or_else(|| dir.script()))?;
                    norn::replay::replay(&dir, &mut ScriptedAgent::new(script))?
                }
                "llm" => {
                    if script.is_some() {
                        bail!("execution {id} was run by a model: --script is for scripted runs; use --model or --base-url");
                    }
                    let mut cfg: LlmConfig = serde_json::from_str(&std::fs::read_to_string(dir.llm_config())?)?;
                    cfg.api_key = LlmConfig::from_env(String::new()).api_key;
                    if let Some(m) = model {
                        cfg.model = m;
                    }
                    if let Some(u) = base_url {
                        cfg.base_url = u;
                    }
                    cfg.check_reachable()?;
                    norn::replay::replay(&dir, &mut LlmAgent::new(cfg))?
                }
                "mcp" => bail!("execution {id} was driven by an MCP client; its reasoning is not in the log, so it cannot be replayed"),
                other => bail!("execution {id} was started with agent `{other}`, which this build cannot replay"),
            };
            print!("{}", trace::for_terminal(&norn::replay::render(&report)?));
        }
        Cmd::Diff { a, b } => {
            let (da, db) = (ExecDir::new(&state_dir, &a), ExecDir::new(&state_dir, &b));
            let (ea, eb) = (trace::load(&da)?, trace::load(&db)?);
            let text = trace::render_run_diff(&trace::Side { id: &a, dir: &da, events: &ea }, &trace::Side { id: &b, dir: &db, events: &eb })?;
            print!("{}", trace::for_terminal(&text));
        }
        Cmd::Search { tool, path, outcome, since } => {
            let outcome = outcome.as_deref().map(trace::Outcome::parse).transpose()?;
            print!("{}", trace::for_terminal(&trace::search(&state_dir, &trace::Query { tool, path, outcome, since })?));
        }
        Cmd::Trace { id, events, json, call, full, security, graph, diff, otel, denied, failed, uncertain, tool, path } => {
            let id = match id {
                Some(id) => id,
                None => match workspace::list_executions(&state_dir)?.pop() {
                    Some(id) => id,
                    None => bail!("no executions in {}", state_dir.display()),
                },
            };
            let dir = ExecDir::new(&state_dir, &id);
            let evs = trace::load(&dir)?;
            let blobs = norn::blob::BlobStore::new(&dir.blobs());
            let text = if json {
                trace::render_json(&evs)?
            } else if events {
                trace::render_events(&evs)?
            } else if let Some(n) = call {
                trace::render_call(&evs, n, &blobs, full, trace::is_live(&dir))?
            } else if security {
                trace::render_security(&evs)?
            } else if let Some(form) = graph {
                match form {
                    GraphForm::Tree => trace::render_tree(&evs)?,
                    GraphForm::Dot => trace::render_dot(&evs)?,
                }
            } else if let Some(n) = diff {
                trace::render_diff(&dir, &evs, n)?
            } else if otel {
                trace::render_otel(&evs)?
            } else {
                let filter = trace::Filter { denied, failed, uncertain, tool, path };
                trace::render_calls(&evs, &filter, Some(&blobs), trace::is_live(&dir))?
            };
            // JSON and OTLP are escaped by serde_json; the rest is text from
            // the log for a terminal (D-175).
            let text = if json || otel { text } else { trace::for_terminal(&text) };
            print!("{text}");
        }
    }
    Ok(())
}

/// The sandbox needs a cgroup that systemd has handed over to this user
/// (D-013). If norn was started without one, start it again inside one:
/// `systemd-run --user --scope -p Delegate=yes norn …`, which is what the
/// user was told to type before (D-105).
///
/// `systemd-run --scope` does not stay around as a parent: it turns into
/// the command it was given, so signals, exit codes and `kill -9` reach the
/// same process as before. `NORN_IN_SCOPE` marks the second start, so
/// this cannot loop; setting it by hand switches the whole thing off.
fn enter_delegated_scope() {
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    if norn::env_var("IN_SCOPE").is_some() || norn::sandbox::cgroup::CgroupRoot::init().is_ok() {
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
    // if norn was itself started by `systemd-run --scope`, the default
    // name is already taken by the scope it is in.
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0);
    let unit = format!("--unit=norn-{}-{nanos}", std::process::id());
    // `exec` only returns if it failed; then the normal path goes on.
    let _ = Command::new("systemd-run")
        .arg(unit)
        .args(scope)
        .arg(me)
        .args(std::env::args_os().skip(1))
        .env("NORN_IN_SCOPE", "1")
        .exec();
}
