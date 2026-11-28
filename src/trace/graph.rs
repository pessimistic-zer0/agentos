//! `norn trace <id> --graph`: the causal links of a run (C1 to C3), from
//! recorded ids only (spec §9.13): which message, script step or client
//! request asked for each call; which call waited for an approval and which
//! one used it; which stop's recovery settled which call; where a run was
//! resumed. As an indented tree, or as DOT for Graphviz (`--graph=dot`).
//!
//! Nothing is guessed from time or from order inside a part of the run. The
//! one use of order is to place each call in the part of the run (the start,
//! or a resume) whose events come before it: the log's `seq` is the order.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::Result;
use serde_json::Value;

use crate::log::{self, CallState, Cause, Event, EventBody};

/// One part of the run: from its start or a resume to the next one.
struct Part {
    title: String,
    /// Recovery that happened at the start of this part: (seq, settled calls).
    recovery: Option<(u64, Vec<u64>)>,
    calls: Vec<u64>,
}

/// What happened to one approval id: the calls that waited for it, the
/// calls that used a yes (with who said it), and the refusals.
#[derive(Default)]
struct ApprovalTrail {
    waited: Vec<u64>,
    used: Vec<(u64, String)>,
    refused: Vec<(u64, String)>,
}

struct Graph<'a> {
    execution: String,
    agent: String,
    parts: Vec<Part>,
    calls: Vec<log::CallRecord>,
    causes: BTreeMap<u64, Option<Cause>>,
    messages: BTreeMap<u64, &'a Value>,
    approvals: BTreeMap<String, ApprovalTrail>,
}

fn build(events: &[Event]) -> Result<Graph<'_>> {
    let calls = log::calls(events)?;
    let mut g = Graph {
        execution: String::new(),
        agent: String::new(),
        parts: vec![Part { title: "start".into(), recovery: None, calls: vec![] }],
        calls,
        causes: BTreeMap::new(),
        messages: BTreeMap::new(),
        approvals: BTreeMap::new(),
    };
    for ev in events {
        match &ev.body {
            EventBody::ExecutionCreated { execution_id, agent, .. } => {
                g.execution = execution_id.clone();
                g.agent = agent.clone();
            }
            EventBody::AgentMessage { message } => {
                g.messages.insert(ev.seq, message);
            }
            EventBody::ToolRequested { call_id, cause, .. } => {
                g.causes.insert(*call_id, cause.clone());
                g.parts.last_mut().expect("never empty").calls.push(*call_id);
            }
            EventBody::RecoveryStarted { .. } => {
                // Recovery belongs to the part that follows the stop. A
                // `norn recover` on its own (no resume after it) opens a part
                // of its own too, so its verdicts are shown.
                g.parts.push(Part { title: "after a stop".into(), recovery: Some((ev.seq, vec![])), calls: vec![] });
            }
            EventBody::ToolReconciled { call_id, .. } => {
                if let Some((_, settled)) = &mut g.parts.last_mut().expect("never empty").recovery {
                    settled.push(*call_id);
                }
            }
            EventBody::ExecutionResumed { by, after_calls } => {
                let title = format!("resumed by `{by}` after {after_calls} call(s)");
                // Joined to the recovery just before it, if that part has no call yet.
                match g.parts.last_mut() {
                    Some(p) if p.recovery.is_some() && p.calls.is_empty() && p.title == "after a stop" => p.title = format!("after a stop, {title}"),
                    _ => g.parts.push(Part { title, recovery: None, calls: vec![] }),
                }
            }
            EventBody::ApprovalRequested { call_id, approval, .. } => g.approvals.entry(approval.clone()).or_default().waited.push(*call_id),
            EventBody::ApprovalUsed { call_id, approval, by, .. } => g.approvals.entry(approval.clone()).or_default().used.push((*call_id, by.clone())),
            EventBody::ApprovalRefused { call_id, approval, by } => {
                g.approvals.entry(approval.clone()).or_default().refused.push((*call_id, by.clone()))
            }
            _ => {}
        }
    }
    g.parts.retain(|p| !(p.calls.is_empty() && p.recovery.is_none() && p.title == "start"));
    Ok(g)
}

impl Graph<'_> {
    fn call_label(&self, id: u64) -> String {
        match self.calls.iter().find(|c| c.call_id == id) {
            Some(c) => format!("#{id} {} {}  {}", c.call.name(), super::truncate(&c.call.describe(), 50), state_word(&c.state)),
            None => format!("#{id}"),
        }
    }

    fn cause_label(&self, cause: &Option<Cause>) -> String {
        cause_text(cause, |seq| self.messages.get(&seq).copied())
    }
}

/// A cause in words. `message` finds an `agent.message` by its `seq`.
pub(super) fn cause_text<'a>(cause: &Option<Cause>, message: impl Fn(u64) -> Option<&'a Value>) -> String {
    match cause {
        None => "cause not recorded".into(),
        Some(Cause::Step { step }) => format!("script step {step}"),
        Some(Cause::McpRequest { id }) => format!("client request {id} (its reasons are not sent to norn)"),
        Some(Cause::Message { seq }) => {
            let text = message(*seq).and_then(|m| m.get("content")).and_then(Value::as_str).unwrap_or("").trim();
            if text.is_empty() {
                format!("message {seq} (a tool call, no text)")
            } else {
                format!("message {seq}: \"{}\"", super::truncate(text, 70))
            }
        }
    }
}

fn state_word(s: &CallState) -> &'static str {
    match s {
        CallState::Requested => "no decision",
        CallState::Denied { .. } => "DENIED",
        CallState::AwaitingApproval { .. } => "WAITED",
        CallState::IntentLogged { .. } => "UNCERTAIN",
        CallState::Completed { .. } => "done",
        CallState::Failed { .. } => "FAILED",
        CallState::Reconciled { applied: true, .. } => "settled: applied",
        CallState::Reconciled { applied: false, .. } => "settled: not applied",
    }
}

/// The tree, for people.
pub fn render_tree(events: &[Event]) -> Result<String> {
    let g = build(events)?;
    let mut out = String::new();
    writeln!(out, "execution {} ({})", g.execution, g.agent)?;
    for p in &g.parts {
        writeln!(out, "  {}", p.title)?;
        if let Some((_, settled)) = &p.recovery {
            if settled.is_empty() {
                writeln!(out, "    recovery: nothing was cut off")?;
            }
            for id in settled {
                writeln!(out, "    recovery settled {}", g.call_label(*id))?;
            }
        }
        for id in &p.calls {
            let cause = g.causes.get(id).cloned().flatten();
            writeln!(out, "    {} -> {}", g.cause_label(&cause), g.call_label(*id))?;
        }
    }
    if !g.approvals.is_empty() {
        writeln!(out, "  approvals")?;
        for (id, ApprovalTrail { waited, used, refused }) in &g.approvals {
            let mut steps: Vec<String> = waited.iter().map(|c| format!("#{c} waited")).collect();
            steps.extend(used.iter().map(|(c, by)| format!("{by} said yes, #{c} ran")));
            steps.extend(refused.iter().map(|(c, by)| format!("{by} said no to #{c}")));
            writeln!(out, "    {id}: {}", steps.join(" -> "))?;
        }
    }
    Ok(out)
}

/// The same links as a Graphviz graph: `norn trace <id> --graph=dot | dot -Tsvg`.
pub fn render_dot(events: &[Event]) -> Result<String> {
    let g = build(events)?;
    let mut out = String::new();
    writeln!(out, "digraph norn {{")?;
    writeln!(out, "  rankdir=LR; node [shape=box, fontname=\"monospace\", fontsize=10];")?;
    writeln!(out, "  exec [label={}, shape=ellipse];", quote(&format!("execution {} ({})", g.execution, g.agent)))?;
    for (i, p) in g.parts.iter().enumerate() {
        writeln!(out, "  part{i} [label={}, shape=ellipse];", quote(&p.title))?;
        writeln!(out, "  exec -> part{i};")?;
        if let Some((seq, settled)) = &p.recovery {
            writeln!(out, "  rec{seq} [label={}, shape=octagon];", quote("recovery"))?;
            writeln!(out, "  part{i} -> rec{seq};")?;
            for id in settled {
                writeln!(out, "  rec{seq} -> call{id} [label=\"settled\"];")?;
            }
        }
        for id in &p.calls {
            writeln!(out, "  call{id} [label={}];", quote(&g.call_label(*id)))?;
            match g.causes.get(id).cloned().flatten() {
                None => writeln!(out, "  part{i} -> call{id} [style=dashed, label=\"cause not recorded\"];")?,
                cause => {
                    let node = match &cause {
                        Some(Cause::Message { seq }) => format!("msg{seq}"),
                        Some(Cause::Step { step }) => format!("step{step}"),
                        Some(Cause::McpRequest { id }) => format!("req{}", sanitize(&id.to_string())),
                        None => unreachable!("matched above"),
                    };
                    writeln!(out, "  {node} [label={}, shape=note];", quote(&g.cause_label(&cause)))?;
                    writeln!(out, "  part{i} -> {node};")?;
                    writeln!(out, "  {node} -> call{id};")?;
                }
            }
        }
    }
    for (id, ApprovalTrail { waited, used, refused }) in &g.approvals {
        let node = format!("appr_{}", sanitize(id));
        writeln!(out, "  {node} [label={}, shape=diamond];", quote(&format!("approval {id}")))?;
        for c in waited {
            writeln!(out, "  call{c} -> {node} [label=\"waited\"];")?;
        }
        for (c, by) in used {
            writeln!(out, "  {node} -> call{c} [label={}];", quote(&format!("yes ({by})")))?;
        }
        for (c, by) in refused {
            writeln!(out, "  {node} -> call{c} [label={}, color=red];", quote(&format!("no ({by})")))?;
        }
    }
    writeln!(out, "}}")?;
    Ok(out)
}

/// A DOT string: in double quotes, with `\` and `"` escaped and line
/// breaks as `\n`, so no text from the log can end the label early.
fn quote(s: &str) -> String {
    format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n").replace('\r', ""))
}

/// Letters, digits and `_` only: for node names built from ids.
fn sanitize(s: &str) -> String {
    s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_from_the_log_cannot_break_out_of_dot() {
        assert_eq!(quote(r#"a "b" \c"#), r#""a \"b\" \\c""#);
        assert_eq!(quote("x\ny"), r#""x\ny""#);
        assert_eq!(sanitize("\"a-1\""), "_a_1_");
    }
}
