//! `norn trace <id> --otel`: the run as OpenTelemetry spans, in the OTLP
//! JSON format (X1, v4 M26), written by hand with `serde_json`: no
//! OpenTelemetry library, and nothing is sent anywhere (D-169). A person
//! sends the file to a collector, for example with
//! `curl -H 'Content-Type: application/json' --data-binary @run.json http://127.0.0.1:4318/v1/traces`.
//!
//! The shape (E4, D-171): one span for the execution; one for each part of
//! the run (the start, and each stop or resume after it); one for each call,
//! inside its part. A call that used an approval links to the call that
//! waited for it. Ids are lower-case hex taken from a SHA-256 of the
//! execution id, so the same run always gives the same file.

use anyhow::Result;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use super::timing;
use crate::log::{self, parse_ts_ms, CallState, Cause, Event, EventBody};

fn hex_id(parts: &[&str], bytes: usize) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p.as_bytes());
        h.update([0u8]);
    }
    h.finalize()[..bytes].iter().map(|b| format!("{b:02x}")).collect()
}

fn ns(ms: i64) -> String {
    (ms.max(0) as u128 * 1_000_000).to_string()
}

fn attr(key: &str, value: Value) -> Value {
    let v = match value {
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({"intValue": n.to_string()}),
        Value::Bool(b) => json!({"boolValue": b}),
        Value::String(s) => json!({"stringValue": s}),
        other => json!({"stringValue": other.to_string()}),
    };
    json!({"key": key, "value": v})
}

/// Span status: unset for a call that ended normally, an error with a
/// message for one that did not (E4).
fn status(state: &CallState) -> Value {
    let error = |m: String| json!({"code": 2, "message": m});
    match state {
        CallState::Denied { rule } => error(format!("denied: {rule}")),
        CallState::Failed { error: e } => error(format!("failed: {}", super::truncate(e, 200))),
        CallState::IntentLogged { .. } | CallState::Requested => error("uncertain: no ending recorded".into()),
        CallState::Reconciled { applied: false, reason, .. } => error(format!("not applied: {}", super::truncate(reason, 200))),
        _ => json!({"code": 0}),
    }
}

pub fn render_otel(events: &[Event]) -> Result<String> {
    let calls = log::calls(events)?;
    let times = timing::of_calls(events);
    let Some(EventBody::ExecutionCreated { execution_id, agent, policy_name, policy_hash, task, .. }) = events.first().map(|e| &e.body) else {
        anyhow::bail!("the log has no execution.created event");
    };
    let trace_id = hex_id(&["norn trace", execution_id], 16);
    let exec_span = hex_id(&["norn execution", execution_id], 8);
    let ts = |e: &Event| parse_ts_ms(&e.ts).unwrap_or(0);
    let first = events.first().map(ts).unwrap_or(0);
    let last = events.last().map(ts).unwrap_or(first);
    let status_of_run = events.iter().rev().find_map(|e| match &e.body {
        EventBody::ExecutionFinished { status, .. } => Some(status.clone()),
        _ => None,
    });
    let usage = crate::replay::usage_of(events);
    let mut spans = Vec::new();
    let mut exec_attrs = vec![
        attr("norn.execution_id", json!(execution_id)),
        attr("norn.agent", json!(agent)),
        attr("norn.policy", json!(policy_name)),
        attr("norn.policy_hash", json!(policy_hash)),
        attr("norn.task", json!(super::truncate(task, 500))),
        attr("norn.calls", json!(calls.len())),
        attr("norn.status", json!(status_of_run.clone().unwrap_or_else(|| "not finished".into()))),
    ];
    if usage.replies > 0 {
        exec_attrs.push(attr("norn.prompt_tokens", json!(usage.prompt_tokens)));
        exec_attrs.push(attr("norn.completion_tokens", json!(usage.completion_tokens)));
        exec_attrs.push(attr("norn.models", json!(usage.models.join(", "))));
    }
    spans.push(json!({
        "traceId": trace_id, "spanId": exec_span, "name": "execution", "kind": 1,
        "startTimeUnixNano": ns(first), "endTimeUnixNano": ns(last),
        "attributes": exec_attrs,
        "status": if status_of_run.as_deref() == Some("completed") || status_of_run.is_none() { json!({"code": 0}) } else { json!({"code": 2, "message": status_of_run}) },
    }));

    // Parts of the run: the start, and each stop (recovery) or resume.
    // Each covers its events up to the next part.
    let mut parts: Vec<(String, String, i64)> = vec![(hex_id(&["norn part", execution_id, "0"], 8), "start".into(), first)];
    let mut part_of_call = std::collections::HashMap::new();
    for e in events {
        match &e.body {
            EventBody::RecoveryStarted { .. } => parts.push((hex_id(&["norn part", execution_id, &e.seq.to_string()], 8), "recovery after a stop".into(), ts(e))),
            EventBody::ExecutionResumed { by, after_calls } => {
                parts.push((hex_id(&["norn part", execution_id, &e.seq.to_string()], 8), format!("resumed by {by} after {after_calls} call(s)"), ts(e)))
            }
            EventBody::ToolRequested { call_id, .. } => {
                part_of_call.insert(*call_id, parts.len() - 1);
            }
            _ => {}
        }
    }
    for (i, (id, name, start)) in parts.iter().enumerate() {
        let end = parts.get(i + 1).map(|p| p.2).unwrap_or(last);
        spans.push(json!({
            "traceId": trace_id, "spanId": id, "parentSpanId": exec_span, "name": name, "kind": 1,
            "startTimeUnixNano": ns(*start), "endTimeUnixNano": ns(end.max(*start)), "attributes": [], "status": {"code": 0},
        }));
    }

    // The span of each approval's waiting call, for the link from the call that used it.
    let mut waited: std::collections::HashMap<&str, u64> = std::collections::HashMap::new();
    for e in events {
        if let EventBody::ApprovalRequested { call_id, approval, .. } = &e.body {
            waited.insert(approval.as_str(), *call_id);
        }
    }
    let call_span = |n: u64| hex_id(&["norn call", execution_id, &n.to_string()], 8);
    for c in &calls {
        let mine: Vec<&Event> = events.iter().filter(|e| e.body.call_id() == Some(c.call_id)).collect();
        let start = mine.first().map(|e| ts(e)).unwrap_or(first);
        let took = times.get(&c.call_id).and_then(|t| t.took_ms).unwrap_or(0);
        let mut attrs = vec![
            attr("norn.call_id", json!(c.call_id)),
            attr("norn.tool", json!(c.call.name())),
            attr("norn.arguments", json!(super::truncate(&c.call.describe(), 300))),
        ];
        let mut links = Vec::new();
        for e in &mine {
            match &e.body {
                EventBody::ToolRequested { cause: Some(cause), .. } => attrs.push(attr(
                    "norn.cause",
                    json!(match cause {
                        Cause::Message { seq } => format!("agent.message {seq}"),
                        Cause::Step { step } => format!("script step {step}"),
                        Cause::McpRequest { id } => format!("MCP request {id}"),
                    }),
                )),
                EventBody::ToolIntent { rule, .. } | EventBody::ToolDenied { rule, .. } => attrs.push(attr("norn.rule", json!(rule))),
                EventBody::ToolCompleted { summary, checkpoint, .. } => {
                    attrs.push(attr("norn.summary", json!(summary)));
                    if let Some(cp) = checkpoint {
                        attrs.push(attr("norn.checkpoint", json!(cp)));
                    }
                }
                EventBody::ApprovalRequested { approval, .. } => attrs.push(attr("norn.approval.waiting", json!(approval))),
                EventBody::ApprovalUsed { approval, by, .. } => {
                    attrs.push(attr("norn.approval.used", json!(approval)));
                    attrs.push(attr("norn.approval.by", json!(by)));
                    if let Some(w) = waited.get(approval.as_str()) {
                        links.push(json!({"traceId": trace_id, "spanId": call_span(*w), "attributes": [attr("norn.link", json!("waited for this approval"))]}));
                    }
                }
                EventBody::ExecNetwork { connections, .. } => {
                    attrs.push(attr("norn.network.connections", json!(connections.len())));
                    attrs.push(attr("norn.network.refused", json!(connections.iter().filter(|n| !n.allowed).count())));
                }
                EventBody::ToolReconciled { applied, .. } => attrs.push(attr("norn.reconciled", json!(if *applied { "applied" } else { "not applied" }))),
                _ => {}
            }
        }
        let part = part_of_call.get(&c.call_id).copied().unwrap_or(0);
        spans.push(json!({
            "traceId": trace_id, "spanId": call_span(c.call_id), "parentSpanId": parts[part].0,
            "name": c.call.name(), "kind": 1,
            "startTimeUnixNano": ns(start), "endTimeUnixNano": ns(start + took),
            "attributes": attrs, "links": links, "status": status(&c.state),
        }));
    }
    let doc = json!({"resourceSpans": [{
        "resource": {"attributes": [attr("service.name", json!("norn")), attr("service.version", json!(env!("CARGO_PKG_VERSION")))]},
        "scopeSpans": [{"scope": {"name": "norn.trace", "version": env!("CARGO_PKG_VERSION")}, "spans": spans}],
    }]});
    Ok(format!("{}\n", serde_json::to_string(&doc)?))
}
