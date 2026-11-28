//! `norn trace <id> --security`: what the run tried that the rules stopped
//! or had to look at (T5; spec §9.13 "security view"). Denials, changes the
//! runtime undid, refused and allowed network connections, approvals, and
//! calls cut off by a stop. All of it is already in the log; this view only
//! gathers it in one place.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::Result;

use crate::log::{self, CallState, Event, EventBody};

pub fn render_security(events: &[Event]) -> Result<String> {
    let calls = log::calls(events)?;
    let label = |id: u64| {
        calls.iter().find(|c| c.call_id == id).map(|c| format!("#{id} {} {}", c.call.name(), super::truncate(&c.call.describe(), 50))).unwrap_or(format!("#{id}"))
    };
    let mut out = String::new();
    if let Some(EventBody::ExecutionCreated { execution_id, policy_name, policy_hash, .. }) = events.first().map(|e| &e.body) {
        writeln!(out, "security view of {execution_id}, policy {policy_name} ({policy_hash})\n")?;
    }

    let denied: Vec<String> = events
        .iter()
        .filter_map(|e| match &e.body {
            EventBody::ToolDenied { call_id, rule, reason } => Some(format!("{}: {rule} ({reason})", label(*call_id))),
            _ => None,
        })
        .collect();
    section(&mut out, "denied by the policy", &denied)?;

    let reverted: Vec<String> = events
        .iter()
        .filter_map(|e| match &e.body {
            EventBody::PolicyViolation { call_id, reverted } => Some(format!("{}: put back {}", label(*call_id), reverted.join(", "))),
            _ => None,
        })
        .collect();
    section(&mut out, "changed outside the write rules, undone by the runtime", &reverted)?;

    // Network: every refused connection, and the allowed ones added up by host.
    let mut refused = Vec::new();
    let mut failed = Vec::new();
    let mut allowed: BTreeMap<(String, u16), (u64, u64, u64)> = BTreeMap::new();
    let mut unrecorded = 0;
    for e in events {
        if let EventBody::ExecNetwork { call_id, connections, dropped } = &e.body {
            unrecorded += dropped;
            for n in connections {
                match (n.allowed, &n.error) {
                    (false, _) => refused.push(format!("{} {}:{} in {}: {}", n.method, n.host, n.port, label(*call_id), n.rule)),
                    (true, Some(err)) => failed.push(format!("{} {}:{} in {}: {}", n.method, n.host, n.port, label(*call_id), super::truncate(err, 60))),
                    (true, None) => {
                        let a = allowed.entry((n.host.clone(), n.port)).or_default();
                        *a = (a.0 + 1, a.1 + n.bytes_up, a.2 + n.bytes_down);
                    }
                }
            }
        }
    }
    section(&mut out, "network connections refused", &refused)?;
    let reached: Vec<String> =
        allowed.iter().map(|((h, p), (n, up, down))| format!("{h}:{p}: {n} connection(s), {up} B up, {down} B down")).collect();
    section(&mut out, "network hosts reached", &reached)?;
    if !failed.is_empty() {
        section(&mut out, "network connections allowed but not made", &failed)?;
    }
    if unrecorded > 0 {
        writeln!(out, "  ({unrecorded} more connection(s) were not recorded: the per-call limit)\n")?;
    }

    // Approvals: one line per approval id, from its first request to its end.
    let mut approvals: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for e in events {
        match &e.body {
            EventBody::ApprovalRequested { call_id, approval, rule, .. } => {
                approvals.entry(approval.clone()).or_default().push(format!("asked by {} ({rule})", label(*call_id)))
            }
            EventBody::ApprovalUsed { call_id, approval, by, granted_at } => {
                approvals.entry(approval.clone()).or_default().push(format!("given by {by} at {granted_at}, used by #{call_id}"))
            }
            EventBody::ApprovalRefused { call_id, approval, by } => {
                approvals.entry(approval.clone()).or_default().push(format!("refused by {by}, for #{call_id}"))
            }
            _ => {}
        }
    }
    let approvals: Vec<String> = approvals.iter().map(|(id, steps)| format!("{id}: {}", steps.join("; "))).collect();
    section(&mut out, "approvals", &approvals)?;

    let cut: Vec<String> = calls
        .iter()
        .filter_map(|c| match &c.state {
            CallState::Reconciled { applied, reason, .. } => {
                Some(format!("{}: settled by recovery, {}: {reason}", label(c.call_id), if *applied { "applied" } else { "not applied" }))
            }
            CallState::IntentLogged { .. } | CallState::Requested => Some(format!("{}: UNCERTAIN, not settled yet", label(c.call_id))),
            _ => None,
        })
        .collect();
    section(&mut out, "calls cut off by a stop", &cut)?;
    Ok(out)
}

fn section(out: &mut String, title: &str, lines: &[String]) -> Result<()> {
    writeln!(out, "{title} ({})", lines.len())?;
    if lines.is_empty() {
        writeln!(out, "  none")?;
    }
    for l in lines {
        writeln!(out, "  {l}")?;
    }
    writeln!(out)?;
    Ok(())
}
