//! `norn trace` and `norn status`: read an execution's event log and
//! print it for people (or as JSON lines for scripts). TraceWeaver (v4,
//! spec §9.13): every view here only reads the record a run left (the log,
//! the blob store, the checkpoints) and never changes it.

use std::fmt::Write as _;

use anyhow::Result;

use crate::blob::BlobStore;
use crate::log::{self, CallState, Event, EventBody};
use crate::runtime::{is_locked, truncate};
use crate::tools::LoggedCall;
use crate::workspace::ExecDir;

/// Is a process working on this execution now (`norn run`, `recover` or
/// `mcp` holds its lock)?
pub fn is_live(dir: &ExecDir) -> bool {
    is_locked(dir)
}

/// Overall state of an execution, worked out from its log.
pub fn execution_state(dir: &ExecDir, events: &[Event]) -> String {
    if let Some(Event { body: EventBody::ExecutionFinished { status, .. }, .. }) =
        events.iter().rev().find(|e| matches!(e.body, EventBody::ExecutionFinished { .. }))
    {
        return status.clone();
    }
    // An MCP execution is unfinished between sessions by design (D-141).
    let mcp = events.iter().any(|e| matches!(&e.body, EventBody::ExecutionCreated { agent, .. } if agent == "mcp"));
    match (is_locked(dir), mcp) {
        (true, true) => "serving (norn mcp)".into(),
        (true, false) => "running".into(),
        (false, true) => "open (`norn mcp` continues it, `norn finish` ends it)".into(),
        (false, false) => "interrupted (run `norn recover`)".into(),
    }
}

mod diff;
mod graph;
mod inspect;
mod security;
mod timing;

pub use diff::render_diff;
pub use graph::{render_dot, render_tree};
pub use inspect::render_call;
pub use security::render_security;
pub use timing::{fmt_ms, Timing};

/// Which calls the call list shows (T2). The kinds (`denied`, `failed`,
/// `uncertain`) widen each other: a call is shown if it is any of the kinds
/// asked for. `tool` and `path` narrow: a shown call must also match them.
#[derive(Debug, Default, Clone)]
pub struct Filter {
    pub denied: bool,
    pub failed: bool,
    pub uncertain: bool,
    pub tool: Option<String>,
    /// A glob in the policy's syntax, matched against the path the call names.
    pub path: Option<String>,
}

impl Filter {
    pub fn is_empty(&self) -> bool {
        !self.denied && !self.failed && !self.uncertain && self.tool.is_none() && self.path.is_none()
    }

    fn describe(&self) -> String {
        let mut parts = Vec::new();
        for (on, name) in [(self.denied, "denied"), (self.failed, "failed"), (self.uncertain, "cut off by a stop")] {
            if on {
                parts.push(name.to_string());
            }
        }
        let mut out = parts.join(" or ");
        if let Some(t) = &self.tool {
            out.push_str(&format!("{}tool {t}", if out.is_empty() { "" } else { ", " }));
        }
        if let Some(p) = &self.path {
            out.push_str(&format!("{}path {p}", if out.is_empty() { "" } else { ", " }));
        }
        out
    }

    /// `exit`: the exit code of a command that ran, from its output.
    fn keeps(&self, c: &log::CallRecord, exit: Option<i64>) -> bool {
        let kinds = self.denied || self.failed || self.uncertain;
        let kind = !kinds
            || (self.denied && matches!(c.state, CallState::Denied { .. }))
            || (self.failed && (matches!(c.state, CallState::Failed { .. }) || exit.is_some_and(|e| e != 0)))
            || (self.uncertain && matches!(c.state, CallState::Requested | CallState::IntentLogged { .. } | CallState::Reconciled { .. }));
        let tool = self.tool.as_deref().is_none_or(|t| t == c.call.name());
        let path = self.path.as_deref().is_none_or(|g| path_of(&c.call).is_some_and(|p| crate::policy::glob_match(g, &normal(p))));
        kind && tool && path
    }
}

/// The path a call names, for any tool that names one.
pub fn path_of(call: &LoggedCall) -> Option<&str> {
    match call {
        LoggedCall::FsRead { path, .. } | LoggedCall::FsList { path } => Some(path),
        LoggedCall::FsSearch { path, .. } => path.as_deref(),
        other => other.path(),
    }
}

/// A path as the agent wrote it, in the form the policy's globs use:
/// no `./` in front, no `/` at the end.
fn normal(p: &str) -> String {
    let p = p.trim_start_matches("./").trim_end_matches('/');
    if p.is_empty() { ".".into() } else { p.into() }
}

/// The exit code of a command call that ran, read from its output in the
/// blob store. `None` for other tools, or if the output cannot be read.
fn exit_code(blobs: Option<&BlobStore>, c: &log::CallRecord) -> Option<i64> {
    let (LoggedCall::Exec { .. }, CallState::Completed { output, .. }) = (&c.call, &c.state) else { return None };
    let bytes = blobs?.get(output).ok()?;
    serde_json::from_slice::<serde_json::Value>(&bytes).ok()?.get("exit_code")?.as_i64()
}

/// One line per tool call: what was asked, what the policy said, how it
/// ended, how long the agent took before it and how long it took itself.
/// `blobs` lets `--failed` find commands that exited with an error. `live`:
/// the execution is running now, so a call without an ending is in progress,
/// not uncertain.
pub fn render_calls(events: &[Event], filter: &Filter, blobs: Option<&BlobStore>, live: bool) -> Result<String> {
    let mut out = String::new();
    for ev in events {
        if let EventBody::ExecutionCreated { execution_id, repo, policy_name, agent, task, .. } = &ev.body {
            writeln!(out, "execution {execution_id}")?;
            writeln!(out, "  repo    {repo}\n  policy  {policy_name}\n  agent   {agent}\n  task    {}", truncate(task, 70))?;
        }
        if let EventBody::WorkspaceCreated { files_copied, denied, skipped, checkpoint } = &ev.body {
            writeln!(out, "  workspace: {files_copied} files copied, base checkpoint {checkpoint}")?;
            if !denied.is_empty() {
                writeln!(out, "  left out by deny rules: {}", denied.join(", "))?;
            }
            if !skipped.is_empty() {
                writeln!(out, "  left out by skip rules: {}", skipped.join(", "))?;
            }
        }
    }
    writeln!(out)?;
    let calls = log::calls(events)?;
    let times = timing::of_calls(events);
    let shown: Vec<&log::CallRecord> = calls.iter().filter(|c| filter.keeps(c, exit_code(blobs, c))).collect();
    if !filter.is_empty() {
        writeln!(out, "showing {} of {} calls: {}", shown.len(), calls.len(), filter.describe())?;
    }
    // Times: `agent` is from the moment the agent had its last answer (or the
    // workspace, or a resume) to this request; `took` is this call's own time.
    let h = ["call", "agent", "took", "tool", "arguments", "", "result"];
    writeln!(out, "{:<4} {:>7} {:>7}  {:<8} {:<40} {:<5}  {}", h[0], h[1], h[2], h[3], h[4], h[5], h[6])?;
    // Stops and resumes between calls, placed by their position in the log.
    let mut markers = timing::markers(events).into_iter().peekable();
    for c in shown {
        let first = c.events.first().copied().unwrap_or(0);
        while let Some((_, line)) = markers.next_if(|(seq, _)| *seq < first) {
            writeln!(out, "     -- {line}")?;
        }
        let t = times.get(&c.call_id).copied().unwrap_or_default();
        let (verdict, detail) = verdict_of(events, c, live);
        writeln!(
            out,
            "#{:<3} {:>7} {:>7}  {:<8} {:<40} {:<5}  {}",
            c.call_id,
            t.agent_ms.map(fmt_ms).unwrap_or_else(|| "-".into()),
            t.took_ms.map(fmt_ms).unwrap_or_else(|| "-".into()),
            c.call.name(),
            truncate(&c.call.describe(), 40),
            verdict,
            detail
        )?;
        for ev in events.iter().filter(|e| e.body.call_id() == Some(c.call_id)) {
            if let EventBody::PolicyViolation { reverted, .. } = &ev.body {
                writeln!(out, "      policy violation: reverted {}", reverted.join(", "))?;
            }
            if let EventBody::ExecNetwork { connections, dropped, .. } = &ev.body {
                for n in connections {
                    let what = match (n.allowed, &n.error) {
                        (false, _) => format!("REFUSED ({})", n.rule),
                        (true, Some(e)) => format!("failed: {}", truncate(e, 50)),
                        (true, None) => format!("{} B up, {} B down", n.bytes_up, n.bytes_down),
                    };
                    writeln!(out, "      network: {} {}:{} {what}", n.method, n.host, n.port)?;
                }
                if *dropped > 0 {
                    writeln!(out, "      network: {dropped} more connection(s) not recorded")?;
                }
            }
        }
    }
    if filter.is_empty() {
        for (_, line) in markers {
            writeln!(out, "     -- {line}")?;
        }
    }
    if let Some(Event { body: EventBody::ExecutionFinished { status, summary, patch }, .. }) =
        events.iter().rev().find(|e| matches!(e.body, EventBody::ExecutionFinished { .. }))
    {
        writeln!(out, "\nfinished: {status} ({summary})")?;
        if let Some(p) = patch {
            writeln!(out, "patch: {p}")?;
        }
    }
    for ev in events {
        if let EventBody::ResultApplied { target, changes } = &ev.body {
            writeln!(out, "applied: {} change(s) to {target} at {}", changes.len(), ev.ts)?;
        }
    }
    Ok(out)
}

/// The short verdict (`allow`, `DENY`, `WAIT`, `?`) and the rest of the line.
fn verdict_of(events: &[Event], c: &log::CallRecord, live: bool) -> (&'static str, String) {
    match &c.state {
        CallState::Requested if live => ("?", "requested; the policy is deciding".to_string()),
        CallState::IntentLogged { .. } if live => ("allow", "RUNNING: intent recorded, not ended yet".to_string()),
        CallState::Requested => ("?", "requested; no decision recorded".to_string()),
        CallState::Denied { rule } => ("DENY", rule.clone()),
        CallState::AwaitingApproval { approval, rule } => ("WAIT", format!("waiting for approval {approval} ({rule})")),
        CallState::IntentLogged { .. } => ("allow", "UNCERTAIN: intent recorded, no ending".to_string()),
        CallState::Completed { checkpoint, .. } => {
            let summary = summary_of(events, c.call_id).unwrap_or_default();
            let cp = checkpoint.as_ref().map(|c| format!("  [{c}]")).unwrap_or_default();
            ("allow", format!("{summary}{cp}"))
        }
        CallState::Failed { error } => ("allow", format!("FAILED: {}", truncate(error, 60))),
        CallState::Reconciled { applied, reason, checkpoint } => {
            let cp = checkpoint.as_ref().map(|c| format!("  [{c}]")).unwrap_or_default();
            let v = if *applied { "RECOVERED, APPLIED" } else { "RECOVERED, NOT APPLIED" };
            ("allow", format!("{v}: {}{cp}", truncate(reason, 70)))
        }
    }
}

fn summary_of(events: &[Event], call_id: u64) -> Option<String> {
    events.iter().find_map(|e| match &e.body {
        EventBody::ToolCompleted { call_id: id, summary, .. } if *id == call_id => Some(summary.clone()),
        _ => None,
    })
}

/// Every event, one per line: `seq  time  type  details`.
pub fn render_events(events: &[Event]) -> Result<String> {
    let mut out = String::new();
    for ev in events {
        let mut body = serde_json::to_value(&ev.body)?;
        if let Some(obj) = body.as_object_mut() {
            obj.remove("type");
        }
        writeln!(out, "{:>4}  {}  {:<18} {}", ev.seq, &ev.ts[11..23], ev.body.type_name(), truncate(&body.to_string(), 120))?;
    }
    Ok(out)
}

pub fn render_json(events: &[Event]) -> Result<String> {
    let mut out = String::new();
    for ev in events {
        writeln!(out, "{}", serde_json::to_string(ev)?)?;
    }
    Ok(out)
}

/// The execution's events, read without changing the log (D-170).
pub fn load(dir: &ExecDir) -> Result<Vec<Event>> {
    log::read_events(&dir.log())
}
