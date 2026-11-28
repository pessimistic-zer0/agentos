//! The event log: an append-only list of everything that happened in one
//! execution, stored in SQLite.
//!
//! Each row is one event. Rows are only ever inserted, never updated or
//! deleted (D-005). The current state of anything (a tool call, the whole
//! execution) is worked out by reading the events in order.

use std::path::Path;

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OpenFlags, OptionalExtension};
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
    /// Connections an `exec` command asked the network proxy for, allowed
    /// or refused, with bytes each way (M19, N3). Written when the command
    /// has ended, before the call's checkpoint; a call cut off by a crash
    /// has no such record.
    #[serde(rename = "exec.network")]
    ExecNetwork { call_id: u64, connections: Vec<crate::sandbox::proxy::Connection>, dropped: u64 },
    /// The call needs a person's yes and has none that can be used: it was
    /// not run. Terminal for the call (M20). `content`: for a write, the
    /// hash of the whole new file, so the person can see what they approve.
    #[serde(rename = "approval.requested")]
    ApprovalRequested { call_id: u64, approval: String, rule: String, content: Option<String> },
    /// A person's yes was found and is used now, for this call; the intent
    /// follows. `granted_at` tells one answer from a later one for the same id.
    #[serde(rename = "approval.used")]
    ApprovalUsed { call_id: u64, approval: String, by: String, granted_at: String },
    /// A person said no. Terminal for the call, like a denial.
    #[serde(rename = "approval.refused")]
    ApprovalRefused { call_id: u64, approval: String, by: String },
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
    /// The agent goes on with a stopped execution (D-170): `by` is
    /// `"norn run --resume"` or `"norn mcp"`, `after_calls` the number of
    /// calls made before. Before 1.3.0 a resume left no event.
    #[serde(rename = "execution.resumed")]
    ExecutionResumed { by: String, after_calls: u64 },
    #[serde(rename = "execution.finished")]
    ExecutionFinished { status: String, summary: String, patch: Option<String> },
    /// `norn apply` copied the result into a repository (D-108). Written
    /// after `execution.finished`; an execution may have several.
    #[serde(rename = "result.applied")]
    ResultApplied { target: String, changes: Vec<String> },
}

/// Every event type, in the order of `docs/event-schema.md`. A test checks
/// that the document describes each one; `type_name` (an exhaustive match)
/// and the test `types_lists_every_variant` keep this list complete.
pub const TYPES: &[&str] = &[
    "execution.created",
    "workspace.created",
    "agent.message",
    "tool.requested",
    "tool.denied",
    "tool.intent",
    "tool.completed",
    "tool.failed",
    "policy.violation",
    "exec.network",
    "approval.requested",
    "approval.used",
    "approval.refused",
    "recovery.started",
    "tool.reconciled",
    "recovery.completed",
    "execution.resumed",
    "execution.finished",
    "result.applied",
];

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
            EventBody::ExecNetwork { .. } => "exec.network",
            EventBody::ApprovalRequested { .. } => "approval.requested",
            EventBody::ApprovalUsed { .. } => "approval.used",
            EventBody::ApprovalRefused { .. } => "approval.refused",
            EventBody::RecoveryStarted { .. } => "recovery.started",
            EventBody::ToolReconciled { .. } => "tool.reconciled",
            EventBody::RecoveryCompleted { .. } => "recovery.completed",
            EventBody::ExecutionResumed { .. } => "execution.resumed",
            EventBody::ExecutionFinished { .. } => "execution.finished",
            EventBody::ResultApplied { .. } => "result.applied",
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
            | EventBody::PolicyViolation { call_id, .. }
            | EventBody::ExecNetwork { call_id, .. }
            | EventBody::ApprovalRequested { call_id, .. }
            | EventBody::ApprovalUsed { call_id, .. }
            | EventBody::ApprovalRefused { call_id, .. } => Some(*call_id),
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
        read_rows(&self.conn)
    }
}

/// Every event of the log at `path`, read without changing anything: for
/// `norn trace` and the other readers, which may run while the execution is
/// still writing (spec §9.13). The file is opened read-only, so nothing is
/// created, no journal mode is set and no row is written; WAL gives the read
/// one fixed snapshot, so it never sees half an event (E1, D-170).
pub fn read_events(path: &Path) -> Result<Vec<Event>> {
    if !path.exists() {
        bail!("no event log at {}", path.display());
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX | OpenFlags::SQLITE_OPEN_URI;
    let conn = Connection::open_with_flags(path, flags).with_context(|| format!("opening event log {} to read it", path.display()))?;
    match read_checked(&conn) {
        // A log closed cleanly has no `-wal` and `-shm` files beside it, and a
        // read-only connection must create `-shm` to read a WAL database. In a
        // folder it cannot write, SQLite refuses (E1, Findings). With no
        // `-wal` file every committed event is in the main file and no writer
        // is active (it would need the `-wal`), so the file can be read as
        // immutable: SQLite then skips the shared memory altogether.
        Err(e) if is_readonly_directory(&e) && !wal_of(path).exists() => {
            let uri = format!("file:{}?immutable=1", uri_path(path));
            let conn = Connection::open_with_flags(&uri, flags).with_context(|| format!("opening event log {} to read it", path.display()))?;
            read_checked(&conn)
        }
        other => other,
    }
}

fn wal_of(path: &Path) -> std::path::PathBuf {
    let mut p = path.as_os_str().to_owned();
    p.push("-wal");
    p.into()
}

fn is_readonly_directory(e: &anyhow::Error) -> bool {
    matches!(e.downcast_ref::<rusqlite::Error>(), Some(rusqlite::Error::SqliteFailure(f, _)) if f.extended_code == rusqlite::ffi::SQLITE_READONLY_DIRECTORY)
}

/// A path as the path part of an SQLite `file:` URI: the three characters
/// that would end it or start an escape are written as `%xx`.
fn uri_path(path: &Path) -> String {
    path.to_string_lossy().replace('%', "%25").replace('?', "%3f").replace('#', "%23")
}

/// The schema check and the rows, on a connection opened read-only.
fn read_checked(conn: &Connection) -> Result<Vec<Event>> {
    // A log that is being created has no tables yet for a moment: no events.
    let tables: i64 =
        conn.query_row("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN ('meta', 'events')", [], |r| r.get(0))?;
    if tables < 2 {
        return Ok(Vec::new());
    }
    let version: Option<String> =
        conn.query_row("SELECT value FROM meta WHERE key = 'schema_version'", [], |r| r.get(0)).optional()?;
    match version {
        None => return Ok(Vec::new()),
        Some(v) if v == SCHEMA_VERSION.to_string() => {}
        Some(v) => bail!("event log schema version {v} is not supported (expected {SCHEMA_VERSION})"),
    }
    read_rows(conn)
}

fn read_rows(conn: &Connection) -> Result<Vec<Event>> {
    let mut stmt = conn.prepare("SELECT seq, ts, body FROM events ORDER BY seq")?;
    let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)? as u64, r.get::<_, String>(1)?, r.get::<_, String>(2)?)))?;
    let mut out = Vec::new();
    for row in rows {
        let (seq, ts, body) = row?;
        let body: EventBody = serde_json::from_str(&body).with_context(|| format!("event {seq} cannot be decoded"))?;
        out.push(Event { seq, ts, body });
    }
    Ok(out)
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
    /// Terminal: not run, waiting for a person's yes (M20).
    AwaitingApproval { approval: String, rule: String },
}

impl CallState {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            CallState::Denied { .. }
                | CallState::Completed { .. }
                | CallState::Failed { .. }
                | CallState::Reconciled { .. }
                | CallState::AwaitingApproval { .. }
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
            CallState::AwaitingApproval { approval, rule } => {
                Some(ToolOutcome::Waiting { execution: String::new(), approval: approval.clone(), rule: rule.clone() })
            }
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
            (CallState::Requested, EventBody::ApprovalUsed { .. }) => CallState::Requested,
            (CallState::Requested, EventBody::ApprovalRequested { approval, rule, .. }) => {
                CallState::AwaitingApproval { approval: approval.clone(), rule: rule.clone() }
            }
            (CallState::Requested, EventBody::ApprovalRefused { approval, by, .. }) => {
                CallState::Denied { rule: format!("{by} refused approval {approval}") }
            }
            (CallState::IntentLogged { .. }, EventBody::PolicyViolation { .. } | EventBody::ExecNetwork { .. }) => rec.state.clone(),
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

/// Milliseconds since 1970 for a time written by `now_rfc3339`
/// (`YYYY-MM-DDTHH:MM:SS.mmmZ`); `None` for anything else. For durations
/// shown to people only: order always comes from `seq`.
pub fn parse_ts_ms(ts: &str) -> Option<i64> {
    let b = ts.as_bytes();
    if b.len() != 24 || b[4] != b'-' || b[7] != b'-' || b[10] != b'T' || b[13] != b':' || b[16] != b':' || b[19] != b'.' || b[23] != b'Z' {
        return None;
    }
    let num = |r: std::ops::Range<usize>| ts.get(r)?.parse::<i64>().ok();
    let (y, m, d) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (h, min, s, ms) = (num(11..13)?, num(14..16)?, num(17..19)?, num(20..23)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) || h > 23 || min > 59 || s > 60 {
        return None;
    }
    Some((days_from_civil(y, m as u32, d as u32) * 86_400 + h * 3600 + min * 60 + s) * 1000 + ms)
}

/// (year, month, day) to days since 1970-01-01: the inverse of
/// `civil_from_days`, from the same source.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let mp = if m > 2 { m - 3 } else { m + 9 } as i64;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
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
        let d = std::env::temp_dir().join(format!("norn-log-test-{}-{name}", std::process::id()));
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
    fn times_parse_back() {
        assert_eq!(parse_ts_ms("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(parse_ts_ms("2026-11-16T20:42:15.487Z"), Some(1_794_861_735_487));
        for days in [-1000, 0, 19_723, 20_513, 60_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        let now = now_rfc3339();
        let back = parse_ts_ms(&now).unwrap();
        let sys = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as i64;
        assert!((sys - back).abs() < 5_000, "{now} -> {back}, now {sys}");
        for bad in ["", "2026-11-16 20:42:15.487Z", "2026-13-16T20:42:15.487Z", "2026-11-16T20:42:15Z", "2026-11-16T20:42:15.487+00:00"] {
            assert_eq!(parse_ts_ms(bad), None, "{bad}");
        }
    }

    #[test]
    fn types_lists_every_variant() {
        // One body of each kind; `type_name` is an exhaustive match, so a new
        // variant must be added here (and to TYPES) or this file fails to build.
        let all = |b: &EventBody| match b {
            EventBody::ExecutionCreated { .. }
            | EventBody::WorkspaceCreated { .. }
            | EventBody::AgentMessage { .. }
            | EventBody::ToolRequested { .. }
            | EventBody::ToolDenied { .. }
            | EventBody::ToolIntent { .. }
            | EventBody::ToolCompleted { .. }
            | EventBody::ToolFailed { .. }
            | EventBody::PolicyViolation { .. }
            | EventBody::ExecNetwork { .. }
            | EventBody::ApprovalRequested { .. }
            | EventBody::ApprovalUsed { .. }
            | EventBody::ApprovalRefused { .. }
            | EventBody::RecoveryStarted { .. }
            | EventBody::ToolReconciled { .. }
            | EventBody::RecoveryCompleted { .. }
            | EventBody::ExecutionResumed { .. }
            | EventBody::ExecutionFinished { .. }
            | EventBody::ResultApplied { .. } => TYPES.contains(&b.type_name()),
        };
        let samples = [
            EventBody::AgentMessage { message: Value::Null },
            EventBody::ExecutionResumed { by: "x".into(), after_calls: 0 },
            EventBody::RecoveryCompleted { reconciled: 0, duration_ms: 0 },
            EventBody::ResultApplied { target: "t".into(), changes: vec![] },
        ];
        assert!(samples.iter().all(all));
        assert_eq!(TYPES.len(), 19, "one entry per variant, no repeats");
        let mut sorted = TYPES.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), TYPES.len());
    }

    #[test]
    fn a_reader_changes_nothing_and_reads_beside_a_writer() -> Result<()> {
        let path = tmp("reader");
        assert!(read_events(&path).is_err());
        assert!(!path.exists(), "reading a missing log does not create it");
        let mut log = EventLog::create(&path)?;
        log.append(EventBody::AgentMessage { message: Value::Null })?;
        // The writer is still open; the reader sees what is committed.
        assert_eq!(read_events(&path).context("beside the writer")?.len(), 1);
        log.append(EventBody::AgentMessage { message: Value::Null })?;
        assert_eq!(read_events(&path).context("second read")?.len(), 2);
        drop(log);
        // A read-only folder: reading still works (E1).
        let dir = path.parent().unwrap();
        let mut perm = std::fs::metadata(dir)?.permissions();
        use std::os::unix::fs::PermissionsExt;
        perm.set_mode(0o555);
        std::fs::set_permissions(dir, perm.clone())?;
        let read = read_events(&path);
        perm.set_mode(0o755);
        std::fs::set_permissions(dir, perm)?;
        assert_eq!(read?.len(), 2);
        assert!(!wal_of(&path).exists(), "reading wrote nothing beside the log");
        // The fallback's URI keeps odd characters in a folder name.
        assert_eq!(uri_path(Path::new("/s/a?b#c%d/log.db")), "/s/a%3fb%23c%25d/log.db");
        Ok(())
    }

    #[test]
    fn append_and_read_back_in_order() -> Result<()> {
        let path = tmp("order");
        let mut log = EventLog::create(&path)?;
        let call = serde_json::to_value(ToolCall::FsRead { path: "a".into(), start_line: None, max_lines: None, encoding: None }.logged())?;
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
        let call = serde_json::to_value(ToolCall::FsRead { path: "a".into(), start_line: None, max_lines: None, encoding: None }.logged())?;
        log.append(EventBody::ToolRequested { call_id: 1, call })?;
        // COMPLETED straight after REQUESTED skips the intent: must be refused.
        log.append(EventBody::ToolCompleted { call_id: 1, summary: "".into(), output: "".into(), checkpoint: None })?;
        assert!(calls(&log.events()?).is_err());
        Ok(())
    }
}
