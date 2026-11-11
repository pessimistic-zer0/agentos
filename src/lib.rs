//! AgentOS: runs a coding agent's tool calls through a gateway that checks a
//! policy and records every step in an event log before it happens.
//!
//! The library holds all the logic; `main.rs` is only the command line. That
//! split lets the integration tests in `tests/` use the same code.

pub mod blob;
pub mod checkpoint;
pub mod log;
pub mod policy;
pub mod tools;
pub mod workspace;
