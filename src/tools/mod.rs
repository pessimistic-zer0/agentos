//! The tools the agent can ask for, and what it gets back: the four of v1
//! (`fs.read`, `fs.list`, `fs.write`, `exec`) and the bonus tool `ext.call`.
//!
//! The agent never runs a tool itself (design rule 1). It hands a `ToolCall`
//! to the gateway, and the gateway decides, logs and dispatches it.

pub mod exec;
pub mod ext;
pub mod fs;

use serde::{Deserialize, Serialize};

use crate::blob::content_hash;

/// A request from the agent. Serialized with a `"tool"` field, e.g.
/// `{"tool": "fs.read", "path": "src/stats.py"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool")]
pub enum ToolCall {
    /// Read a text file: all of it, or with `start_line` / `max_lines`
    /// only some of its lines (the way to look into a file that is larger
    /// than `limits.max_output_kb`; D-102). Lines count from 1.
    #[serde(rename = "fs.read")]
    FsRead {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_line: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_lines: Option<u64>,
    },
    #[serde(rename = "fs.list")]
    FsList { path: String },
    #[serde(rename = "fs.write")]
    FsWrite { path: String, content: String },
    /// Replace one piece of text in an existing file (D-120). `old_str` must
    /// occur in the file exactly once; the rest of the file is untouched, so
    /// a model need not send a whole file to change one line.
    #[serde(rename = "fs.edit")]
    FsEdit { path: String, old_str: String, new_str: String },
    #[serde(rename = "exec")]
    Exec { argv: Vec<String> },
    /// Ask an outside service, named in the policy, to act on `payload`.
    /// The only tool whose effect cannot be rolled back.
    #[serde(rename = "ext.call")]
    ExtCall { service: String, payload: String },
}

impl ToolCall {
    pub fn name(&self) -> &'static str {
        match self {
            ToolCall::FsRead { .. } => "fs.read",
            ToolCall::FsList { .. } => "fs.list",
            ToolCall::FsWrite { .. } => "fs.write",
            ToolCall::FsEdit { .. } => "fs.edit",
            ToolCall::Exec { .. } => "exec",
            ToolCall::ExtCall { .. } => "ext.call",
        }
    }

    /// Does this tool change the workspace? Only these get a checkpoint.
    /// `ext.call` has an effect too, but outside the workspace, where a
    /// checkpoint cannot reach it.
    pub fn is_mutating(&self) -> bool {
        matches!(self, ToolCall::FsWrite { .. } | ToolCall::FsEdit { .. } | ToolCall::Exec { .. })
    }

    /// The form stored in the log. File contents are replaced by their hash;
    /// the bytes themselves go to the blob store (ADR 0004).
    pub fn logged(&self) -> LoggedCall {
        match self {
            ToolCall::FsRead { path, start_line, max_lines } => {
                LoggedCall::FsRead { path: path.clone(), start_line: *start_line, max_lines: *max_lines }
            }
            ToolCall::FsList { path } => LoggedCall::FsList { path: path.clone() },
            ToolCall::FsWrite { path, content } => LoggedCall::FsWrite {
                path: path.clone(),
                content_hash: content_hash(content.as_bytes()),
                bytes: content.len() as u64,
            },
            ToolCall::FsEdit { path, old_str, new_str } => LoggedCall::FsEdit {
                path: path.clone(),
                old_hash: content_hash(old_str.as_bytes()),
                new_hash: content_hash(new_str.as_bytes()),
                old_bytes: old_str.len() as u64,
                new_bytes: new_str.len() as u64,
            },
            ToolCall::Exec { argv } => LoggedCall::Exec { argv: argv.clone() },
            ToolCall::ExtCall { service, payload } => LoggedCall::ExtCall {
                service: service.clone(),
                payload_hash: content_hash(payload.as_bytes()),
                bytes: payload.len() as u64,
            },
        }
    }
}

/// A `ToolCall` as written in the event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool")]
pub enum LoggedCall {
    #[serde(rename = "fs.read")]
    FsRead {
        path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start_line: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max_lines: Option<u64>,
    },
    #[serde(rename = "fs.list")]
    FsList { path: String },
    #[serde(rename = "fs.write")]
    FsWrite { path: String, content_hash: String, bytes: u64 },
    /// The two texts are in the blob store under their hashes.
    #[serde(rename = "fs.edit")]
    FsEdit { path: String, old_hash: String, new_hash: String, old_bytes: u64, new_bytes: u64 },
    #[serde(rename = "exec")]
    Exec { argv: Vec<String> },
    /// The payload is in the blob store under `payload_hash`; recovery
    /// needs it to send the request again.
    #[serde(rename = "ext.call")]
    ExtCall { service: String, payload_hash: String, bytes: u64 },
}

impl LoggedCall {
    pub fn name(&self) -> &'static str {
        match self {
            LoggedCall::FsRead { .. } => "fs.read",
            LoggedCall::FsList { .. } => "fs.list",
            LoggedCall::FsWrite { .. } => "fs.write",
            LoggedCall::FsEdit { .. } => "fs.edit",
            LoggedCall::Exec { .. } => "exec",
            LoggedCall::ExtCall { .. } => "ext.call",
        }
    }

    /// Short human description of the arguments, for `agentos trace`.
    pub fn describe(&self) -> String {
        match self {
            LoggedCall::FsRead { path, start_line: None, max_lines: None } | LoggedCall::FsList { path } => path.clone(),
            LoggedCall::FsRead { path, start_line, max_lines } => {
                let first = start_line.unwrap_or(1);
                match max_lines {
                    Some(n) => format!("{path} (lines {first}-{})", first.saturating_add(*n).saturating_sub(1)),
                    None => format!("{path} (from line {first})"),
                }
            }
            LoggedCall::FsWrite { path, bytes, .. } => format!("{path} ({bytes} bytes)"),
            LoggedCall::FsEdit { path, old_bytes, new_bytes, .. } => format!("{path} ({old_bytes} bytes -> {new_bytes} bytes)"),
            LoggedCall::Exec { argv } => argv.join(" "),
            LoggedCall::ExtCall { service, bytes, .. } => format!("{service} ({bytes} bytes)"),
        }
    }
}

/// How a call ended, as the agent sees it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ToolOutcome {
    /// The tool ran. For `exec` this includes a non-zero exit code.
    Ok,
    /// The policy refused the call; nothing was done.
    Denied { rule: String },
    /// The tool could not do what was asked (missing file, bad argument);
    /// nothing was changed.
    Failed { error: String },
    /// After a crash, recovery found this call had not taken effect (or undid
    /// it). The agent may ask again.
    NotApplied { reason: String },
}

/// The tool-specific result data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ToolOutput {
    Read {
        content: String,
        /// Set when only part of the file was asked for.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        lines: Option<ReadLines>,
    },
    List { entries: Vec<ListEntry> },
    Write { bytes: u64 },
    /// An `fs.edit` replaced its one occurrence, which began on `line`; the
    /// file now has `bytes` bytes.
    Edit { bytes: u64, line: u64 },
    Exec(ExecOutput),
    /// The service's answer to an `ext.call`. `attempts` is how many
    /// requests it took (all with the same idempotency key).
    External { status: u16, body: String, attempts: u32 },
}

/// Which lines a partial `fs.read` returned, and how many the file has.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReadLines {
    pub first: u64,
    pub last: u64,
    pub total: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ListEntry {
    pub name: String,
    /// `file`, `dir`, `symlink` or `other`.
    pub kind: String,
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecOutput {
    /// `None` if the command was killed by a signal.
    pub exit_code: Option<i32>,
    pub signal: Option<i32>,
    /// The watchdog killed it at `limits.call_timeout_s`.
    pub timed_out: bool,
    pub stdout: String,
    pub stderr: String,
    /// stdout or stderr was longer than `limits.max_output_kb` and was cut.
    pub truncated: bool,
    pub duration_ms: u64,
    /// Paths outside the write globs that the command changed and that were
    /// put back from the checkpoint (D-009).
    #[serde(default)]
    pub reverted: Vec<String>,
}

impl ToolOutput {
    /// One line for the trace and for `tool.completed.summary`.
    pub fn summary(&self) -> String {
        match self {
            ToolOutput::Read { content, lines: None } => format!("{} bytes", content.len()),
            ToolOutput::Read { content, lines: Some(l) } => format!("{} bytes, lines {}-{} of {}", content.len(), l.first, l.last, l.total),
            ToolOutput::List { entries } => format!("{} entries", entries.len()),
            ToolOutput::Write { bytes } => format!("wrote {bytes} bytes"),
            ToolOutput::Edit { bytes, line } => format!("replaced at line {line}; file is {bytes} bytes"),
            ToolOutput::External { status, attempts, .. } => {
                format!("HTTP {status} after {attempts} request{}", if *attempts == 1 { "" } else { "s" })
            }
            ToolOutput::Exec(e) => {
                let status = match (e.exit_code, e.signal) {
                    _ if e.timed_out => "timed out".to_string(),
                    (Some(c), _) => format!("exit {c}"),
                    (None, Some(s)) => format!("killed by signal {s}"),
                    (None, None) => "no status".to_string(),
                };
                let mut s = format!("{status}, {:.1}s", e.duration_ms as f64 / 1000.0);
                if e.truncated {
                    s.push_str(", output truncated");
                }
                if !e.reverted.is_empty() {
                    s.push_str(&format!(", {} change(s) outside write globs reverted", e.reverted.len()));
                }
                s
            }
        }
    }
}

/// What the gateway hands back to the agent for one call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ToolResult {
    pub call_id: u64,
    pub outcome: ToolOutcome,
    pub output: Option<ToolOutput>,
}

/// An error inside a tool, reported to the agent as `ToolOutcome::Failed`.
/// Kept separate from `anyhow::Error`, which means "the runtime itself broke".
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct ToolError(pub String);

impl ToolError {
    pub fn new(msg: impl Into<String>) -> Self {
        ToolError(msg.into())
    }
}
