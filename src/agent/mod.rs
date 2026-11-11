//! Agents: the part that decides which tool to call next.
//!
//! An agent only ever *asks*. It returns an `Action`; the runtime passes any
//! tool call to the gateway and hands the result back. It has no access to
//! the workspace, the log or the host.

pub mod scripted;

use anyhow::Result;

use crate::tools::{ToolCall, ToolResult};

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Call(ToolCall),
    Finish { summary: String },
}

pub trait Agent {
    /// The next thing to do, given the result of the previous call (`None`
    /// before the first call).
    fn next_action(&mut self, last: Option<&ToolResult>) -> Result<Action>;
    /// Short name stored in the log (`scripted`, `llm`).
    fn kind(&self) -> &'static str;
    /// The task, in words, for the log and the trace.
    fn task(&self) -> String;
}
