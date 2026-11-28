//! `norn trace <id> --call N`: everything the record holds about one call
//! (T4): what was asked, the policy's answer and the rule behind it, any
//! approval, the plan written before the effect, how it ended, its full
//! output from the blob store, its network traffic, and recovery's verdict.

use std::fmt::Write as _;

use anyhow::{bail, Result};
use serde_json::Value;

use super::timing;
use crate::blob::BlobStore;
use crate::log::{self, Event, EventBody};
use crate::tools::LoggedCall;

/// Lines of one stored text shown unless `full` is set.
const SHOWN_LINES: usize = 40;

/// `live`: the execution is running now (see `render_calls`).
pub fn render_call(events: &[Event], call_id: u64, blobs: &BlobStore, full: bool, live: bool) -> Result<String> {
    let calls = log::calls(events)?;
    let Some(c) = calls.iter().find(|c| c.call_id == call_id) else {
        bail!("there is no call #{call_id}; this execution has {} call(s)", calls.len());
    };
    let t = timing::of_calls(events).get(&call_id).copied().unwrap_or_default();
    let mine: Vec<&Event> = events.iter().filter(|e| e.body.call_id() == Some(call_id)).collect();
    let mut out = String::new();
    writeln!(out, "call #{call_id}  {}  {}", c.call.name(), c.call.describe())?;
    let seqs: Vec<String> = c.events.iter().map(u64::to_string).collect();
    writeln!(out, "  events     seq {}", seqs.join(", "))?;
    if let Some(first) = mine.first() {
        let before = t.agent_ms.map(|ms| format!(" (the agent took {} before it)", timing::fmt_ms(ms))).unwrap_or_default();
        writeln!(out, "  asked      {}{before}", first.ts)?;
    }
    if let Some(EventBody::ToolRequested { call, .. }) = mine.first().map(|e| &e.body) {
        writeln!(out, "  arguments  {}", serde_json::to_string(call)?)?;
    }
    write_argument_blobs(&mut out, &c.call, blobs, full)?;
    for ev in &mine {
        match &ev.body {
            EventBody::ToolDenied { rule, reason, .. } => writeln!(out, "  policy     DENIED by {rule} ({reason})")?,
            EventBody::ApprovalRequested { approval, rule, .. } => {
                writeln!(out, "  approval   asked for {approval} ({rule}); not run, the agent was told to wait")?
            }
            EventBody::ApprovalUsed { approval, by, granted_at, .. } => {
                writeln!(out, "  approval   {approval} given by {by} at {granted_at}, used now")?
            }
            EventBody::ApprovalRefused { approval, by, .. } => writeln!(out, "  approval   {approval} refused by {by}")?,
            EventBody::ToolIntent { rule, plan, .. } => {
                writeln!(out, "  policy     allowed by {rule}")?;
                writeln!(out, "  plan       {}  (written and fsynced before the effect)", serde_json::to_string(plan)?)?;
            }
            EventBody::PolicyViolation { reverted, .. } => {
                writeln!(out, "  reverted   changed outside the write rules and put back: {}", reverted.join(", "))?
            }
            EventBody::ExecNetwork { connections, dropped, .. } => {
                for n in connections {
                    let what = match (n.allowed, &n.error) {
                        (false, _) => format!("REFUSED ({})", n.rule),
                        (true, Some(e)) => format!("allowed by {}, failed: {e}", n.rule),
                        (true, None) => format!("allowed by {}: {} B up, {} B down", n.rule, n.bytes_up, n.bytes_down),
                    };
                    writeln!(out, "  network    {} {}:{} {what}", n.method, n.host, n.port)?;
                }
                if *dropped > 0 {
                    writeln!(out, "  network    {dropped} more connection(s) not recorded")?;
                }
            }
            EventBody::ToolCompleted { summary, output, checkpoint, .. } => {
                let took = t.took_ms.map(|ms| format!(" after {}", timing::fmt_ms(ms))).unwrap_or_default();
                let cp = checkpoint.as_ref().map(|c| format!("; checkpoint {c}")).unwrap_or_default();
                writeln!(out, "  ended      completed{took} at {}: {summary}{cp}", ev.ts)?;
                write_output(&mut out, blobs, output, full)?;
            }
            EventBody::ToolFailed { error, .. } => writeln!(out, "  ended      FAILED at {}: {error}", ev.ts)?,
            EventBody::ToolReconciled { applied, reason, checkpoint, restored, .. } => {
                let v = if *applied { "APPLIED" } else { "NOT APPLIED" };
                writeln!(out, "  recovery   cut off by a stop; settled at {}: {v}, {reason}", ev.ts)?;
                if let Some(c) = checkpoint {
                    writeln!(out, "  recovery   checkpoint {c} taken for it")?;
                }
                if let Some(r) = restored {
                    writeln!(out, "  recovery   the workspace was put back to {r}")?;
                }
            }
            _ => {}
        }
    }
    if matches!(c.state, log::CallState::IntentLogged { .. }) {
        if live {
            writeln!(out, "  ended      not yet: the call is running")?;
        } else {
            writeln!(out, "  ended      UNCERTAIN: the intent is recorded and no ending; `norn recover` settles it")?;
        }
    }
    Ok(out)
}

/// What a call carried that the log keeps only as a hash: the new file of a
/// write, the two texts of an edit, the payload of an `ext.call`.
fn write_argument_blobs(out: &mut String, call: &LoggedCall, blobs: &BlobStore, full: bool) -> Result<()> {
    let parts: Vec<(&str, &str)> = match call {
        LoggedCall::FsWrite { content_hash, .. } => vec![("content", content_hash)],
        LoggedCall::FsEdit { old_hash, new_hash, .. } => vec![("old text", old_hash), ("new text", new_hash)],
        LoggedCall::ExtCall { payload_hash, .. } => vec![("payload", payload_hash)],
        _ => vec![],
    };
    for (name, hash) in parts {
        match blobs.get(hash) {
            Ok(bytes) => write_text(out, name, &String::from_utf8_lossy(&bytes), full)?,
            Err(e) => writeln!(out, "  {name:<10} {hash}: not readable ({e})")?,
        }
    }
    Ok(())
}

/// A call's full result. Text fields with line breaks (a command's output, a
/// file read) are printed as text; the rest as JSON.
fn write_output(out: &mut String, blobs: &BlobStore, hash: &str, full: bool) -> Result<()> {
    let bytes = match blobs.get(hash) {
        Ok(b) => b,
        Err(e) => {
            writeln!(out, "  output     {hash}: not readable ({e})")?;
            return Ok(());
        }
    };
    let Ok(Value::Object(fields)) = serde_json::from_slice::<Value>(&bytes) else {
        return write_text(out, "output", &String::from_utf8_lossy(&bytes), full);
    };
    writeln!(out, "  output     ({hash})")?;
    for (k, v) in &fields {
        match v {
            Value::String(text) if text.contains('\n') => write_text(out, &format!(".{k}"), text, full)?,
            other => writeln!(out, "    .{k:<9} {}", super::truncate(&other.to_string(), 200))?,
        }
    }
    Ok(())
}

fn write_text(out: &mut String, name: &str, text: &str, full: bool) -> Result<()> {
    let lines: Vec<&str> = text.lines().collect();
    let shown = if full { lines.len() } else { lines.len().min(SHOWN_LINES) };
    let label = if name.starts_with('.') { format!("    {name}") } else { format!("  {name}") };
    writeln!(out, "{label}  ({} line(s))", lines.len())?;
    for l in &lines[..shown] {
        writeln!(out, "      | {l}")?;
    }
    if shown < lines.len() {
        writeln!(out, "      | … {} more line(s); --full shows them", lines.len() - shown)?;
    }
    Ok(())
}
