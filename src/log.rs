//! The event log: an append-only list of everything that happened in one
//! execution, stored in SQLite.
//!
//! Each row is one event. Rows are only ever inserted, never updated or
//! deleted (D-005). The current state of anything (a tool call, the whole
//! execution) is worked out by reading the events in order.

use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::tools::{LoggedCall, ToolOutcome};
#[cfg(test)]
use crate::tools::ToolCall;

/// The bump in this number means old logs cannot be read by new code.
pub const SCHEMA_VERSION: i64 = 1;

/// One record in the log. `seq` starts at 1 and has no gaps.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub seq: u64,
    /// Wall-clock time in UTC (RFC 3339). For people reading the trace only;
    /// ordering always comes from `seq`.
    pub ts: String,
    #[serde(flatten)]
    pub body: EventBody,
}

/// What happened. Serialized with a `"type"` field such as `"tool.requested"`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum EventBody {
    #[serde(rename = "execution.created")]
    ExecutionCreated {
        execution_id: String,
        repo: String,
        policy_name: String,
        /// sha256 of the policy text; the text itself is saved as `policy.toml`.
        policy_hash: String,
        agent: String,
        task: String,
    },
    #[serde(rename = "workspace.created")]
    WorkspaceCreated {
        files_copied: u64,
        /// Workspace-relative paths left out because a deny rule matched (D-008).
        denied: Vec<String>,
        /// Left out by `filesystem.skip` (absent in logs written before D-099).
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        skipped: Vec<String>,
        /// The first checkpoint, used as the base of the output patch.
        checkpoint: String,
    },
    #[serde(rename = "agent.message")]
    AgentMessage { message: Value },
    #[serde(rename = "tool.requested")]
    ToolRequested { call_id: u64, call: Value },
    #[serde(rename = "tool.denied")]
    ToolDenied { call_id: u64, rule: String, reason: String },
    /// Written (and fsynced) before the effect starts: design rule 3.
    #[serde(rename = "tool.intent")]
    ToolIntent { call_id: u64, rule: String, plan: Value },
    #[serde(rename = "tool.completed")]
    ToolCompleted {
        call_id: u64,
        summary: String,
        /// Content hash of the full result in the blob store.
        output: String,
        /// The checkpoint taken after the effect (mutating calls only): rule 4.
        checkpoint: Option<String>,
    },
    #[serde(rename = "tool.failed")]
    ToolFailed { call_id: u64, error: String },
    /// Code run by `exec` changed paths outside the write globs; they were put back.
    #[serde(rename = "policy.violation")]
    PolicyViolation { call_id: u64, reverted: Vec<String> },
    /// Recovery began after a crash. `pending` lists the calls that had no
    /// ending. May appear more than once if recovery itself was interrupted.
    #[serde(rename = "recovery.started")]
    RecoveryStarted { pending: Vec<u64>, orphan_checkpoints: Vec<String> },
    /// Recovery's verdict on one call that had no ending (D-016).
    #[serde(rename = "tool.reconciled")]
    ToolReconciled {
        call_id: u64,
        /// Did the call's effect happen and stay?
        applied: bool,
        reason: String,
        /// APPLIED: the checkpoint taken now for it.
        checkpoint: Option<String>,
        /// NOT APPLIED by rollback: the checkpoint the workspace was restored to.
        restored: Option<String>,
    },
    #[serde(rename = "recovery.completed")]
    RecoveryCompleted { reconciled: u64, duration_ms: u64 },
    #[serde(rename = "execution.finished")]
    ExecutionFinished { status: String, summary: String, patch: Option<String> },
}

impl EventBody {
    pub fn type_name(&self) -> &'static str {
        match self {
            EventBody::ExecutionCreated { .. } => "execution.created",
            EventBody::WorkspaceCreated { .. } => "workspace.created",
            EventBody::AgentMessage { .. } => "agent.message",
            EventBody::ToolRequested { .. } => "tool.requested",
            EventBody::ToolDenied { .. } => "tool.denied",
            EventBody::ToolIntent { .. } => "tool.intent",
            EventBody::ToolCompleted { .. } => "tool.completed",
            EventBody::ToolFailed { .. } => "tool.failed",
            EventBody::PolicyViolation { .. } => "policy.violation",
            EventBody::RecoveryStarted { .. } => "recovery.started",
            EventBody::ToolReconciled { .. } => "tool.reconciled",
            EventBody::RecoveryCompleted { .. } => "recovery.completed",
            EventBody::ExecutionFinished { .. } => "execution.finished",
        }
    }

    pub fn call_id(&self) -> Option<u64> {
        match self {
            EventBody::ToolRequested { call_id, .. }
            | EventBody::ToolDenied { call_id, .. }
            | EventBody::ToolIntent { call_id, .. }
            | EventBody::ToolCompleted { call_id, .. }
            | EventBody::ToolFailed { call_id, .. }
            | EventBody::ToolReconciled { call_id, .. }
            | EventBody::PolicyViolation { call_id, .. } => Some(*call_id),
            _ => None,
        }
    }
}

pub struct EventLog {
    conn: Connection,
    next_seq: u64,
}

impl EventLog {
    /// Create a new, empty log. Fails if the file already holds events.
    pub fn create(path: &Path) -> Result<EventLog> {
        let log = EventLog::open_inner(path)?;
        if log.next_seq != 1 {
            bail!("{} already holds events", path.display());
        }
        Ok(log)
    }

    /// Open an existing log.
    pub fn open(path: &Path) -> Result<EventLog> {
        if !path.exists() {
            bail!("no event log at {}", path.display());
        }
        EventLog::open_inner(path)
    }

    fn open_inner(path: &Path) -> Result<EventLog> {
        let conn = Connection::open(path).with_context(|| format!("opening event log {}", path.display()))?;
        // WAL: a commit appends to a side file, so a crash mid-write never
        // damages earlier rows. synchronous=FULL: every commit is fsynced before
        // it returns, so an event that was appended is on disk (see D-031).
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS meta (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS events (
                 seq     INTEGER PRIMARY KEY,
                 ts      TEXT NOT NULL,
                 type    TEXT NOT NULL,
                 call_id INTEGER,
                 body    TEXT NOT NULL
             );",
        )?;
        let version: Option<String> =
            conn.query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0)).optional()?;
        match version {
            None => {
                conn.execute(
                    "INSERT INTO meta (key, value) VALUES ('schema_version', ?1)",
                    params![SCHEMA_VERSION.to_string()],
                )?;
            }
            Some(v) if v == SCHEMA_VERSION.to_string() => {}
            Some(v) => bail!("event log schema version {v} is not supported (expected {SCHEMA_VERSION})"),
        }
        // SQLite integers are signed 64-bit; sequence numbers and call ids are
        // stored as i64 and converted at this boundary.
        let max: Option<i64> = conn.query_row("SELECT MAX(seq) FROM events", [], |r| r.get(0))?;
        Ok(EventLog { conn, next_seq: max.unwrap_or(0) as u64 + 1 })
    }

    /// Append one event and return its sequence number. When this returns, the
    /// row is committed and fsynced.
    pub fn append(&mut self, body: EventBody) -> Result<u64> {
        let seq = self.next_seq;
        let ts = now_rfc3339();
        let json = serde_json::to_string(&body)?;
        self.conn
            .execute(
                "INSERT INTO events (seq, ts, type, call_id, body) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![seq as i64, ts, body.type_name(), body.call_id().map(|c| c as i64), json],
            )
            .context("appending to the event log")?;
        self.next_seq += 1;
        Ok(seq)
    }

    /// All events, in order.
    pub fn events(&self) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare("SELECT seq, ts, body FROM events ORDER BY seq")?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?, r.get::<_, String>(2)?))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, ts, body) = row?;
            let body: EventBody = serde_json::from_str(&body)
                .with_context(|| format!("event {seq} cannot be decoded"))?;
            out.push(Event { seq, ts, body });
        }
        Ok(out)
    }
}

// ---------------------------------------------------------------------------
// Reading state back out of the events.
// ---------------------------------------------------------------------------

/// Where one tool call got to, worked out from its events.
#[derive(Debug, Clone, PartialEq)]
pub enum CallState {
    /// `tool.requested` only.
    Requested,
    /// The policy said no. Terminal.
    Denied { rule: String },
    /// Intent recorded, no ending yet. After a crash this call is UNCERTAIN.
    IntentLogged { plan: Value },
    /// Terminal: the effect happened (or, for reads, the result was produced).
    Completed { output: String, checkpoint: Option<String> },
    /// Terminal: the tool ran into an error; nothing was changed.
    Failed { error: String },
    /// Terminal: the call was cut off by a crash and recovery settled it.
    Reconciled { applied: bool, reason: String, checkpoint: Option<String> },
}

impl CallState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CallState::Denied { .. } | CallState::Completed { .. } | CallState::Failed { .. } | CallState::Reconciled { .. }
        )
    }
}

#[derive(Debug, Clone)]
pub struct CallRecord {
    pub call_id: u64,
    pub call: LoggedCall,
    pub state: CallState,
    /// The `seq` of each event that belongs to this call, in order.
    pub events: Vec<u64>,
}

impl CallRecord {
    /// The result the agent saw (or will see) for this call.
    pub fn outcome(&self) -> Option<ToolOutcome> {
        match &self.state {
            CallState::Denied { rule } => Some(ToolOutcome::Denied { rule: rule.clone() }),
            CallState::Failed { error } => Some(ToolOutcome::Failed { error: error.clone() }),
            CallState::Completed { .. } => Some(ToolOutcome::Ok),
            CallState::Reconciled { applied: true, .. } => Some(ToolOutcome::Ok),
            CallState::Reconciled { applied: false, reason, .. } => Some(ToolOutcome::NotApplied { reason: reason.clone() }),
            _ => None,
        }
    }
}

/// Group the events into tool calls, in the order the calls were made.
pub fn calls(events: &[Event]) -> Result<Vec<CallRecord>> {
    let mut out: Vec<CallRecord> = Vec::new();
    for ev in events {
        let Some(id) = ev.body.call_id() else { continue };
        if let EventBody::ToolRequested { call, .. } = &ev.body {
            if out.iter().any(|c| c.call_id == id) {
                bail!("event {}: call {id} requested twice", ev.seq);
            }
            let call: LoggedCall =
                serde_json::from_value(call.clone()).with_context(|| format!("event {}: bad call", ev.seq))?;
            out.push(CallRecord { call_id: id, call, state: CallState::Requested, events: vec![ev.seq] });
            continue;
        }
        let Some(rec) = out.iter_mut().find(|c| c.call_id == id) else {
            bail!("event {}: call {id} has no tool.requested before it", ev.seq);
        };
        rec.events.push(ev.seq);
        rec.state = match (&rec.state, &ev.body) {
            (CallState::Requested, EventBody::ToolDenied { rule, .. }) => CallState::Denied { rule: rule.clone() },
            (CallState::Requested, EventBody::ToolIntent { plan, .. }) => CallState::IntentLogged { plan: plan.clone() },
            (CallState::IntentLogged { .. }, EventBody::PolicyViolation { .. }) => rec.state.clone(),
            (CallState::IntentLogged { .. }, EventBody::ToolCompleted { output, checkpoint, .. }) => {
                CallState::Completed { output: output.clone(), checkpoint: checkpoint.clone() }
            }
            (CallState::IntentLogged { .. }, EventBody::ToolFailed { error, .. }) => {
                CallState::Failed { error: error.clone() }
            }
            // A crash can cut a call off right after REQUESTED (no intent) or
            // after its INTENT; recovery then writes the verdict.
            (CallState::Requested | CallState::IntentLogged { .. }, EventBody::ToolReconciled { applied, reason, checkpoint, .. }) => {
                CallState::Reconciled { applied: *applied, reason: reason.clone(), checkpoint: checkpoint.clone() }
            }
            (state, body) => bail!("event {}: `{}` is not allowed after state {state:?}", ev.seq, body.type_name()),
        };
    }
    Ok(out)
}

/// Every checkpoint the log names, in order. "The last checkpoint" always
/// means the last entry of this list (D-011); a checkpoint folder that is not
/// in it does not count and is deleted by recovery.
pub fn named_checkpoints(events: &[Event]) -> Vec<String> {
    let mut out = Vec::new();
    for ev in events {
        match &ev.body {
            EventBody::WorkspaceCreated { checkpoint, .. } => out.push(checkpoint.clone()),
            EventBody::ToolCompleted { checkpoint: Some(c), .. } => out.push(c.clone()),
            EventBody::ToolReconciled { checkpoint: Some(c), .. } => out.push(c.clone()),
            _ => {}
        }
    }
    out
}

/// Check invariant 5: sequence numbers are exactly 1, 2, 3, ... with no gap.
pub fn check_gap_free(events: &[Event]) -> Result<()> {
    for (i, ev) in events.iter().enumerate() {
        if ev.seq != i as u64 + 1 {
            bail!("event log has a gap: position {} holds seq {}", i + 1, ev.seq);
        }
    }
    Ok(())
}

/// Current UTC time as `YYYY-MM-DDTHH:MM:SS.mmmZ`, without a date library.
pub fn now_rfc3339() -> String {
    let d = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let secs = d.as_secs() as i64;
    let (y, m, day) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    format!(
        "{y:04}-{m:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60,
        d.subsec_millis()
    )
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm
/// (public domain, http://howardhinnant.github.io/date_algorithms.html).
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> std::path::PathBuf {
        let d = std::env::temp_dir().join(format!("agentos-log-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d.join("log.db")
    }

    #[test]
    fn dates() {
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(19_723), (2024, 1, 1));
        assert_eq!(civil_from_days(20_513), (2026, 3, 1));
        assert_eq!(now_rfc3339().len(), 24);
    }

    #[test]
    fn append_and_read_back_in_order() -> Result<()> {
        let path = tmp("order");
        let mut log = EventLog::create(&path)?;
        let call = serde_json::to_value(ToolCall::FsRead { path: "a".into(), start_line: None, max_lines: None }.logged())?;
        assert_eq!(log.append(EventBody::ToolRequested { call_id: 1, call })?, 1);
        assert_eq!(log.append(EventBody::ToolDenied { call_id: 1, rule: "r".into(), reason: "x".into() })?, 2);
        drop(log);
        let mut log = EventLog::open(&path)?;
        assert_eq!(log.append(EventBody::AgentMessage { message: Value::Null })?, 3, "seq continues after reopen");
        let evs = log.events()?;
        assert_eq!(evs.iter().map(|e| e.seq).collect::<Vec<_>>(), vec![1, 2, 3]);
        let calls = calls(&evs)?;
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].state, CallState::Denied { rule: "r".into() });
        assert!(EventLog::create(&path).is_err(), "create refuses a log that has events");
        Ok(())
    }

    #[test]
    fn illegal_transitions_are_reported() -> Result<()> {
        let path = tmp("illegal");
        let mut log = EventLog::create(&path)?;
        let call = serde_json::to_value(ToolCall::FsRead { path: "a".into(), start_line: None, max_lines: None }.logged())?;
        log.append(EventBody::ToolRequested { call_id: 1, call })?;
        // COMPLETED straight after REQUESTED skips the intent: must be refused.
        log.append(EventBody::ToolCompleted { call_id: 1, summary: "".into(), output: "".into(), checkpoint: None })?;
        assert!(calls(&log.events()?).is_err());
        Ok(())
    }
}
