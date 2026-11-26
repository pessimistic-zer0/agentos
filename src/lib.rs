//! Norn: runs a coding agent's tool calls through a gateway that checks a
//! policy and records every step in an event log before it happens.
//!
//! The library holds all the logic; `main.rs` is only the command line. That
//! split lets the integration tests in `tests/` use the same code.

pub mod agent;
pub mod approval;
pub mod baseline;
pub mod blob;
pub mod checkpoint;
pub mod faults;
pub mod gateway;
pub mod interrupt;
pub mod log;
pub mod mcp;
pub mod policy;
pub mod recovery;
pub mod runtime;
pub mod sandbox;
pub mod secrets;
pub mod tools;
pub mod trace;
pub mod workspace;

/// Reads the setting `NORN_<name>` from the environment, or else the name it
/// had before the project was renamed, `AGENTOS_<name>` (D-134). The fallback
/// keeps a shell set up for v0.1 (an `AGENTOS_LLM_MODEL` in a profile, say)
/// working instead of silently falling back to the defaults.
pub fn env_var(name: &str) -> Option<String> {
    std::env::var(format!("NORN_{name}")).or_else(|_| std::env::var(format!("AGENTOS_{name}"))).ok()
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_setting_under_the_old_name_is_still_read() {
        // Names used by no other test, so that tests running in parallel do
        // not see each other's settings.
        std::env::set_var("AGENTOS_RENAME_TEST_A", "old");
        assert_eq!(super::env_var("RENAME_TEST_A").as_deref(), Some("old"));
        std::env::set_var("NORN_RENAME_TEST_B", "new");
        std::env::set_var("AGENTOS_RENAME_TEST_B", "old");
        assert_eq!(super::env_var("RENAME_TEST_B").as_deref(), Some("new"), "the new name wins");
        assert_eq!(super::env_var("RENAME_TEST_C"), None);
    }
}
