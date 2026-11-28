//! How long things took (T3), from the events' wall-clock times. Times are
//! for people only: the order of events always comes from `seq`, and a cause
//! is never guessed from a time (spec §9.13).

use std::collections::HashMap;

use crate::log::{parse_ts_ms, Event, EventBody};

/// The two times shown for one call.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct Timing {
    /// From the moment the agent had control (its last answer, the new
    /// workspace, or a resume) to this call's request: the agent's time,
    /// which for an LLM is mostly the model's.
    pub agent_ms: Option<i64>,
    /// From the request to the call's ending (completed, failed, denied,
    /// waiting, or settled by recovery).
    pub took_ms: Option<i64>,
}

/// Is this the event after which the agent has control again?
fn hands_back(body: &EventBody) -> bool {
    matches!(
        body,
        EventBody::WorkspaceCreated { .. }
            | EventBody::ToolCompleted { .. }
            | EventBody::ToolFailed { .. }
            | EventBody::ToolDenied { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalRefused { .. }
            | EventBody::ToolReconciled { .. }
            | EventBody::ExecutionResumed { .. }
    )
}

/// The timing of every call, by call id.
pub fn of_calls(events: &[Event]) -> HashMap<u64, Timing> {
    let mut out: HashMap<u64, Timing> = HashMap::new();
    let mut requested: HashMap<u64, i64> = HashMap::new();
    let mut mark: Option<i64> = None;
    for ev in events {
        let t = parse_ts_ms(&ev.ts);
        if let EventBody::ToolRequested { call_id, .. } = &ev.body {
            let entry = out.entry(*call_id).or_default();
            entry.agent_ms = t.zip(mark).map(|(t, m)| (t - m).max(0));
            if let Some(t) = t {
                requested.insert(*call_id, t);
            }
        }
        if hands_back(&ev.body) {
            if let Some(id) = ev.body.call_id() {
                // A call's first ending counts; recovery may settle it later.
                let entry = out.entry(id).or_default();
                if entry.took_ms.is_none() {
                    entry.took_ms = t.zip(requested.get(&id).copied()).map(|(t, r)| (t - r).max(0));
                }
            }
            if t.is_some() {
                mark = t;
            }
        }
    }
    out
}

/// Lines for the stops and starts between calls, with the `seq` they sit at:
/// a resume (since 1.3.0, D-170), and recovery after a stop.
pub fn markers(events: &[Event]) -> Vec<(u64, String)> {
    let mut out = Vec::new();
    let mut prev: Option<i64> = None;
    for ev in events {
        let t = parse_ts_ms(&ev.ts);
        match &ev.body {
            EventBody::ExecutionResumed { by, after_calls } => {
                let gap = t.zip(prev).map(|(t, p)| format!(", {} after the event before it", fmt_ms(t - p))).unwrap_or_default();
                out.push((ev.seq, format!("resumed by `{by}` after {after_calls} call(s){gap}")));
            }
            EventBody::RecoveryStarted { pending, .. } => {
                let gap = t.zip(prev).map(|(t, p)| format!(", {} after the event before it", fmt_ms(t - p))).unwrap_or_default();
                out.push((ev.seq, format!("recovery after a stop: {} call(s) without an ending{gap}", pending.len())));
            }
            _ => {}
        }
        if t.is_some() {
            prev = t;
        }
    }
    out
}

/// A duration for people: `0.004s`, `7.6s`, `4m05s`, `10h44m`. Under a
/// second in milliseconds, because most tool calls take a few.
pub fn fmt_ms(ms: i64) -> String {
    let ms = ms.max(0);
    if ms < 1_000 {
        format!("{:.3}s", ms as f64 / 1000.0)
    } else if ms < 60_000 {
        format!("{:.1}s", ms as f64 / 1000.0)
    } else if ms < 3_600_000 {
        format!("{}m{:02}s", ms / 60_000, (ms % 60_000) / 1000)
    } else {
        format!("{}h{:02}m", ms / 3_600_000, (ms % 3_600_000) / 60_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn ev(seq: u64, ts: &str, body: EventBody) -> Event {
        Event { seq, ts: format!("2026-11-28T{ts}Z"), body }
    }

    #[test]
    fn durations() {
        assert_eq!(fmt_ms(4), "0.004s");
        assert_eq!(fmt_ms(0), "0.000s");
        assert_eq!(fmt_ms(7_600), "7.6s");
        assert_eq!(fmt_ms(12_345), "12.3s");
        assert_eq!(fmt_ms(245_000), "4m05s");
        assert_eq!(fmt_ms(38_640_000), "10h44m");
    }

    #[test]
    fn the_agent_time_starts_at_the_last_answer_or_a_resume() {
        let call = json!({"tool": "fs.list", "path": "."});
        let events = vec![
            ev(1, "10:00:00.000", EventBody::WorkspaceCreated { files_copied: 1, denied: vec![], skipped: vec![], checkpoint: "cp-0000".into() }),
            ev(2, "10:00:02.000", EventBody::ToolRequested { call_id: 1, call: call.clone() }),
            ev(3, "10:00:02.000", EventBody::ToolIntent { call_id: 1, rule: "r".into(), plan: json!({}) }),
            ev(4, "10:00:02.500", EventBody::ToolCompleted { call_id: 1, summary: "s".into(), output: "o".into(), checkpoint: None }),
            ev(5, "20:00:00.000", EventBody::ExecutionResumed { by: "norn run --resume".into(), after_calls: 1 }),
            ev(6, "20:00:03.000", EventBody::ToolRequested { call_id: 2, call }),
            ev(7, "20:00:03.010", EventBody::ToolDenied { call_id: 2, rule: "r".into(), reason: "policy".into() }),
        ];
        let t = of_calls(&events);
        assert_eq!(t[&1], Timing { agent_ms: Some(2_000), took_ms: Some(500) });
        assert_eq!(t[&2], Timing { agent_ms: Some(3_000), took_ms: Some(10) }, "the ten hours stopped are not the agent's");
        let m = markers(&events);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].0, 5);
        assert!(m[0].1.contains("after 1 call(s), 9h59m after the event before it"), "{}", m[0].1);
    }
}
