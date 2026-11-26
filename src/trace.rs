//! `norn trace` and `norn status`: read an execution's event log and
//! print it for people (or as JSON lines for scripts).

use std::fmt::Write as _;

use anyhow::Result;

use crate::log::{self, CallState, Event, EventBody, EventLog};
use crate::runtime::{is_locked, truncate};
use crate::workspace::ExecDir;

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

/// One line per tool call: what was asked, what the policy said, how it ended.
pub fn render_calls(events: &[Event]) -> Result<String> {
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
    for c in log::calls(events)? {
        let (verdict, detail) = match &c.state {
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
        };
        writeln!(
            out,
            "#{:<3} {:<8} {:<40} {:<5}  {}",
            c.call_id,
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

pub fn load(dir: &ExecDir) -> Result<Vec<Event>> {
    EventLog::open(&dir.log())?.events()
}
