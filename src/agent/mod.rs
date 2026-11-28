//! Agents: the part that decides which tool to call next.
//!
//! An agent only ever *asks*. It returns an `Action`; the runtime passes any
//! tool call to the gateway and hands the result back. It has no access to
//! the workspace, the log or the host.

pub mod llm;
pub mod scripted;

use anyhow::Result;

use crate::log::Event;
use crate::tools::{ToolCall, ToolResult};
use crate::workspace::ExecDir;

#[derive(Debug, Clone, PartialEq)]
pub enum Action {
    Call(ToolCall),
    Finish { summary: String },
    /// The agent cannot go on (for example, the model keeps repeating itself).
    GiveUp { reason: String },
}

pub trait Agent {
    /// The next thing to do, given the result of the previous call (`None`
    /// before the first call).
    fn next_action(&mut self, last: Option<&ToolResult>) -> Result<Action>;
    /// Short name stored in the log (`scripted`, `llm`).
    fn kind(&self) -> &'static str;
    /// The task, in words, for the log and the trace.
    fn task(&self) -> String;
    /// Conversation messages produced since the last call of this method.
    /// The runtime stores each one in the event log as `agent.message`
    /// (ADR 0005). The scripted agent has no conversation.
    fn take_messages(&mut self) -> Vec<serde_json::Value> {
        Vec::new()
    }
    /// For an agent that follows a list of steps: the step (from 1) of the
    /// action just returned, recorded as the cause of its call (C1).
    fn step(&self) -> Option<u64> {
        None
    }
    /// Save into the execution folder whatever is needed to build this agent
    /// again after a crash.
    fn persist(&self, dir: &ExecDir) -> Result<()>;
    /// After a restart: bring the agent back to where it was. `history` holds
    /// the result of every earlier call, in order (reconciled calls included);
    /// `events` is the whole log. The runtime then calls `next_action` with
    /// the last result of `history`.
    fn restore(&mut self, history: &[ToolResult], events: &[Event]) -> Result<()>;
}
