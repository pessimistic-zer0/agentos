//! The four tools the agent can ask for, and what it gets back.
//!
//! The agent never runs a tool itself (design rule 1). It hands a `ToolCall`
//! to the gateway, and the gateway decides, logs and dispatches it.

use serde::{Deserialize, Serialize};

use crate::blob::content_hash;

/// A request from the agent. Serialized with a `"tool"` field, e.g.
/// `{"tool": "fs.read", "path": "src/stats.py"}`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool")]
pub enum ToolCall {
    #[serde(rename = "fs.read")]
    FsRead { path: String },
    #[serde(rename = "fs.list")]
    FsList { path: String },
    #[serde(rename = "fs.write")]
    FsWrite { path: String, content: String },
    #[serde(rename = "exec")]
    Exec { argv: Vec<String> },
}

impl ToolCall {
    pub fn name(&self) -> &'static str {
        match self {
            ToolCall::FsRead { .. } => "fs.read",
            ToolCall::FsList { .. } => "fs.list",
            ToolCall::FsWrite { .. } => "fs.write",
            ToolCall::Exec { .. } => "exec",
        }
    }

    /// Does this tool change the workspace? Only these get a checkpoint.
    pub fn is_mutating(&self) -> bool {
        matches!(self, ToolCall::FsWrite { .. } | ToolCall::Exec { .. })
    }

    /// The form stored in the log. File contents are replaced by their hash;
    /// the bytes themselves go to the blob store (ADR 0004).
    pub fn logged(&self) -> LoggedCall {
        match self {
            ToolCall::FsRead { path } => LoggedCall::FsRead { path: path.clone() },
            ToolCall::FsList { path } => LoggedCall::FsList { path: path.clone() },
            ToolCall::FsWrite { path, content } => LoggedCall::FsWrite {
                path: path.clone(),
                content_hash: content_hash(content.as_bytes()),
                bytes: content.len() as u64,
            },
            ToolCall::Exec { argv } => LoggedCall::Exec { argv: argv.clone() },
        }
    }
}

/// A `ToolCall` as written in the event log.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "tool")]
pub enum LoggedCall {
    #[serde(rename = "fs.read")]
    FsRead { path: String },
    #[serde(rename = "fs.list")]
    FsList { path: String },
    #[serde(rename = "fs.write")]
    FsWrite { path: String, content_hash: String, bytes: u64 },
    #[serde(rename = "exec")]
    Exec { argv: Vec<String> },
}

impl LoggedCall {
    pub fn name(&self) -> &'static str {
        match self {
            LoggedCall::FsRead { .. } => "fs.read",
            LoggedCall::FsList { .. } => "fs.list",
            LoggedCall::FsWrite { .. } => "fs.write",
            LoggedCall::Exec { .. } => "exec",
        }
    }

    /// Short human description of the arguments, for `agentos trace`.
    pub fn describe(&self) -> String {
        match self {
            LoggedCall::FsRead { path } | LoggedCall::FsList { path } => path.clone(),
            LoggedCall::FsWrite { path, bytes, .. } => format!("{path} ({bytes} bytes)"),
            LoggedCall::Exec { argv } => argv.join(" "),
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
    Read { content: String },
    List { entries: Vec<ListEntry> },
    Write { bytes: u64 },
    Exec(ExecOutput),
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
            ToolOutput::Read { content } => format!("{} bytes", content.len()),
            ToolOutput::List { entries } => format!("{} entries", entries.len()),
            ToolOutput::Write { bytes } => format!("wrote {bytes} bytes"),
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
