//! The scripted agent: plays back a fixed list of tool calls from a TOML file.
//!
//! It makes the runtime testable without an LLM: the same script always makes
//! the same calls. The fault-injection harness and the security suite are
//! built on it.
//!
//! One rule makes it useful after a crash: if a call comes back NOT APPLIED
//! (recovery undid it or found it never happened), the agent asks for the same
//! step again instead of moving on.

use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::agent::{Action, Agent};
use crate::tools::{ToolCall, ToolOutcome, ToolResult};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    pub task: String,
    #[serde(rename = "step", default)]
    pub steps: Vec<ToolCall>,
}

impl Script {
    pub fn load(path: &Path) -> Result<Script> {
        let text = std::fs::read_to_string(path).with_context(|| format!("reading script {}", path.display()))?;
        let script: Script = toml::from_str(&text).with_context(|| format!("parsing script {}", path.display()))?;
        if script.steps.is_empty() {
            bail!("script {} has no [[step]] entries", path.display());
        }
        Ok(script)
    }
}

pub struct ScriptedAgent {
    script: Script,
    /// Index of the step most recently handed out.
    pos: Option<usize>,
}

impl ScriptedAgent {
    pub fn new(script: Script) -> ScriptedAgent {
        ScriptedAgent { script, pos: None }
    }
}

impl Agent for ScriptedAgent {
    fn next_action(&mut self, last: Option<&ToolResult>) -> Result<Action> {
        let next = match (self.pos, last) {
            (None, _) => 0,
            // Not applied: the step did not happen, so ask for it again.
            (Some(p), Some(r)) if matches!(r.outcome, ToolOutcome::NotApplied { .. }) => p,
            (Some(p), _) => p + 1,
        };
        self.pos = Some(next);
        match self.script.steps.get(next) {
            Some(call) => Ok(Action::Call(call.clone())),
            None => Ok(Action::Finish { summary: format!("script finished after {} steps", self.script.steps.len()) }),
        }
    }

    fn kind(&self) -> &'static str {
        "scripted"
    }

    fn step(&self) -> Option<u64> {
        self.pos.map(|p| p as u64 + 1)
    }

    fn task(&self) -> String {
        self.script.task.clone()
    }

    fn persist(&self, dir: &crate::workspace::ExecDir) -> Result<()> {
        std::fs::write(dir.script(), toml::to_string(&self.script)?)?;
        Ok(())
    }

    /// Replay the same rule over the recorded results: each one either moved
    /// the script forward or (NOT APPLIED) left it on the same step. The last
    /// result is not consumed here; the runtime passes it to `next_action`.
    fn restore(&mut self, history: &[ToolResult], _events: &[crate::log::Event]) -> Result<()> {
        self.pos = None;
        for i in 0..history.len() {
            let prev = if i == 0 { None } else { Some(&history[i - 1]) };
            self.next_action(prev)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result(outcome: ToolOutcome) -> ToolResult {
        ToolResult { call_id: 1, outcome, output: None }
    }

    #[test]
    fn plays_steps_in_order_and_repeats_not_applied() -> Result<()> {
        let script: Script = toml::from_str(
            r#"
task = "t"
[[step]]
tool = "fs.list"
path = "."
[[step]]
tool = "exec"
argv = ["pytest"]
"#,
        )?;
        let mut a = ScriptedAgent::new(script);
        assert_eq!(a.next_action(None)?, Action::Call(ToolCall::FsList { path: ".".into() }));
        let exec = Action::Call(ToolCall::Exec { argv: vec!["pytest".into()] });
        assert_eq!(a.next_action(Some(&result(ToolOutcome::Ok)))?, exec);
        let again = a.next_action(Some(&result(ToolOutcome::NotApplied { reason: "rolled back".into() })))?;
        assert_eq!(again, exec, "a not-applied step is asked for again");
        assert!(matches!(a.next_action(Some(&result(ToolOutcome::Ok)))?, Action::Finish { .. }));
        Ok(())
    }

    #[test]
    fn restore_finds_the_position_again() -> Result<()> {
        let script: Script = toml::from_str(
            "task = \"t\"\n[[step]]\ntool = \"fs.list\"\npath = \"a\"\n[[step]]\ntool = \"fs.list\"\npath = \"b\"\n[[step]]\ntool = \"fs.list\"\npath = \"c\"\n",
        )?;
        let step = |p: &str| Action::Call(ToolCall::FsList { path: p.into() });
        // Step a done, step b cut off and not applied: b must be asked again.
        let history = vec![result(ToolOutcome::Ok), result(ToolOutcome::NotApplied { reason: "x".into() })];
        let mut a = ScriptedAgent::new(script.clone());
        a.restore(&history, &[])?;
        assert_eq!(a.next_action(history.last())?, step("b"));
        // Step b applied by recovery: go on to c.
        let history = vec![result(ToolOutcome::Ok), result(ToolOutcome::Ok)];
        let mut a = ScriptedAgent::new(script.clone());
        a.restore(&history, &[])?;
        assert_eq!(a.next_action(history.last())?, step("c"));
        // Round trip through the saved file.
        let again: Script = toml::from_str(&toml::to_string(&script)?)?;
        assert_eq!(again.steps, script.steps);
        Ok(())
    }

    #[test]
    fn denied_and_failed_steps_are_not_repeated() -> Result<()> {
        let script: Script = toml::from_str("task = \"t\"\n[[step]]\ntool = \"fs.read\"\npath = \".env\"\n")?;
        let mut a = ScriptedAgent::new(script);
        a.next_action(None)?;
        let r = result(ToolOutcome::Denied { rule: "x".into() });
        assert!(matches!(a.next_action(Some(&r))?, Action::Finish { .. }));
        Ok(())
    }
}
