//! The test baseline (v2, M17, D-145): what a command failed *before* the
//! agent changed anything, so that a project that is already red does not
//! keep an agent from finishing its own task.
//!
//! No command is run for it. Agents run the tests first almost always, so
//! the baseline of a command is its result the first time it ran before the
//! first successful change by a file tool (`fs.write`, `fs.edit`,
//! `fs.delete`) in this execution. A command that itself edits files before
//! that moment would make the baseline later than "untouched"; that is the
//! price of not running anything the agent did not ask for.
//!
//! Failing tests are read from the output of three test runners, the ones
//! the trials used: pytest, cargo test, and node's test runner (both of its
//! reporters). For anything else `failures` says "unknown" and the old rule
//! (D-107: any failure means not done) applies.

use std::collections::{BTreeMap, BTreeSet};

use crate::log::{CallState, Event};
use crate::tools::{ExecOutput, LoggedCall, ToolCall, ToolOutcome, ToolOutput, ToolResult};

/// The names of the failing tests in a command's output. `Some(empty)` for a
/// command that succeeded; `None` when it failed and no names could be read.
pub fn failures(e: &ExecOutput) -> Option<BTreeSet<String>> {
    if e.exit_code == Some(0) {
        return Some(BTreeSet::new());
    }
    let mut names = BTreeSet::new();
    for line in e.stdout.lines().chain(e.stderr.lines()) {
        let l = line.trim();
        // pytest: `FAILED tests/test_x.py::test_y - AssertionError: …`
        // (and `ERROR` for a test that could not even be collected).
        if let Some(rest) = l.strip_prefix("FAILED ").or_else(|| l.strip_prefix("ERROR ")) {
            let name = rest.split(" - ").next().unwrap_or(rest).trim();
            if !name.is_empty() {
                names.insert(name.to_string());
            }
        // cargo test: `test module::name ... FAILED`
        } else if let Some(rest) = l.strip_prefix("test ").and_then(|r| r.strip_suffix(" ... FAILED")) {
            names.insert(rest.trim().to_string());
        // node --test, TAP reporter: `not ok 3 - name`
        } else if let Some(rest) = l.strip_prefix("not ok ") {
            if let Some((_, name)) = rest.split_once(" - ") {
                names.insert(name.trim().to_string());
            }
        // node --test, spec reporter: `✖ name (1.23ms)`; its summary line
        // `✖ failing tests:` is not a test.
        } else if let Some(rest) = l.strip_prefix("✖ ") {
            let name = match rest.rfind(" (") {
                Some(i) if rest.ends_with(')') => &rest[..i],
                _ => rest,
            };
            if name != "failing tests:" && !name.is_empty() {
                names.insert(name.to_string());
            }
        }
    }
    (!names.is_empty()).then_some(names)
}

/// A command's failures now, set against its baseline.
#[derive(Debug, PartialEq)]
pub struct Comparison {
    pub before: BTreeSet<String>,
    pub now: BTreeSet<String>,
}

impl Comparison {
    pub fn new_failures(&self) -> Vec<&String> {
        self.now.difference(&self.before).collect()
    }
    pub fn fixed(&self) -> Vec<&String> {
        self.before.difference(&self.now).collect()
    }
    /// Fewer failures than before the first change, and none that were not
    /// there already: the agent made progress and broke nothing the command
    /// can see. Stricter than "no new failures", so an agent that changed
    /// nothing is not done either.
    pub fn progressed(&self) -> bool {
        self.new_failures().is_empty() && self.now.len() < self.before.len()
    }

    /// One paragraph for the agent, after the command's own output.
    pub fn note(&self, argv: &[String]) -> String {
        let list = |v: Vec<&String>| if v.is_empty() { "none".to_string() } else { v.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ") };
        format!(
            "\n[norn] Before the first file change in this execution, `{}` already failed {} test(s). Now {} fail. \
             New failures since then: {}. Fixed since then: {}.",
            argv.join(" "),
            self.before.len(),
            self.now.len(),
            list(self.new_failures()),
            list(self.fixed())
        )
    }
}

/// The baselines of one execution.
#[derive(Debug, Default)]
pub struct Baseline {
    /// argv → its failures the first time it ran before the first change
    /// (`None`: it failed and the names could not be read).
    first: BTreeMap<Vec<String>, Option<BTreeSet<String>>>,
    /// A file tool has changed the workspace; no new baselines from here on.
    changed: bool,
}

impl Baseline {
    /// Take in one call and its result, in the order they happened.
    pub fn observe(&mut self, call: &ToolCall, result: &ToolResult) {
        match (call, &result.output) {
            (ToolCall::Exec { argv }, Some(ToolOutput::Exec(e))) if !self.changed => {
                self.first.entry(argv.clone()).or_insert_with(|| failures(e));
            }
            (ToolCall::FsWrite { .. } | ToolCall::FsEdit { .. } | ToolCall::FsDelete { .. }, _) => self.note_change(&result.outcome),
            _ => {}
        }
    }

    fn note_change(&mut self, outcome: &ToolOutcome) {
        if matches!(outcome, ToolOutcome::Ok) {
            self.changed = true;
        }
    }

    /// Rebuild from the log after a restart: the calls in order, each with
    /// its result (`history`, as `recovery::history` gives it).
    pub fn from_log(events: &[Event], history: &[ToolResult]) -> Baseline {
        let mut b = Baseline::default();
        let Ok(calls) = crate::log::calls(events) else { return b };
        for c in calls {
            let Some(result) = history.iter().find(|r| r.call_id == c.call_id) else { continue };
            match (&c.call, &result.output) {
                (LoggedCall::Exec { argv }, Some(ToolOutput::Exec(e))) if !b.changed => {
                    b.first.entry(argv.clone()).or_insert_with(|| failures(e));
                }
                (LoggedCall::FsWrite { .. } | LoggedCall::FsEdit { .. } | LoggedCall::FsDelete { .. }, _) => {
                    // A write cut off by a crash and found applied counts too.
                    let applied = matches!(c.state, CallState::Completed { .. }) || matches!(result.outcome, ToolOutcome::Ok);
                    if applied {
                        b.changed = true;
                    }
                }
                _ => {}
            }
        }
        b
    }

    /// How a later result of `argv` compares with its baseline, when both
    /// are readable and there is something to say (a failure then or now).
    pub fn compare(&self, argv: &[String], now: &ExecOutput) -> Option<Comparison> {
        if !self.changed {
            return None; // nothing changed yet: this result *is* the baseline
        }
        let before = self.first.get(argv)?.clone()?;
        let now = failures(now)?;
        (!before.is_empty() || !now.is_empty()).then_some(Comparison { before, now })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(code: i32, stdout: &str) -> ExecOutput {
        ExecOutput { exit_code: Some(code), signal: None, timed_out: false, stdout: stdout.into(), stderr: String::new(), truncated: false, duration_ms: 1, reverted: vec![], network: None }
    }
    fn set(v: &[&str]) -> BTreeSet<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn failing_tests_are_read_from_three_test_runners() {
        let pytest = "..F.F\n=== short test summary info ===\nFAILED tests/test_a.py::test_one - AssertionError: 1 != 2\nFAILED tests/test_b.py::test_two\nERROR tests/test_c.py - ImportError\n2 failed, 3 passed";
        assert_eq!(failures(&out(1, pytest)), Some(set(&["tests/test_a.py::test_one", "tests/test_b.py::test_two", "tests/test_c.py"])));
        let cargo = "running 3 tests\ntest stock::tests::adds ... ok\ntest stock::tests::removes ... FAILED\ntest report::totals ... FAILED\n";
        assert_eq!(failures(&out(101, cargo)), Some(set(&["stock::tests::removes", "report::totals"])));
        let tap = "TAP version 13\nok 1 - adds\nnot ok 2 - removes\n  ---\nnot ok 3 - totals # TODO\n";
        assert_eq!(failures(&out(1, tap)), Some(set(&["removes", "totals # TODO"])));
        let spec = "✔ adds (0.5ms)\n✖ removes (1.2ms)\nℹ tests 2\n✖ failing tests:\n\n✖ removes (1.2ms)\n";
        assert_eq!(failures(&out(1, spec)), Some(set(&["removes"])));
        assert_eq!(failures(&out(0, "all good")), Some(BTreeSet::new()));
        assert_eq!(failures(&out(2, "make: *** [all] Error 2")), None, "unreadable: the old rule applies");
    }

    #[test]
    fn the_baseline_is_the_first_run_before_the_first_change() {
        let argv = vec!["pytest".to_string(), "-q".to_string()];
        let exec = ToolCall::Exec { argv: argv.clone() };
        let write = ToolCall::FsWrite { path: "a.py".into(), content: "x".into(), encoding: None, executable: None };
        let res = |id, o: ExecOutput| ToolResult { call_id: id, outcome: ToolOutcome::Ok, output: Some(ToolOutput::Exec(o)) };
        let red = out(1, "FAILED t::mine\nFAILED t::other\n");
        let mut b = Baseline::default();
        b.observe(&exec, &res(1, red.clone()));
        assert_eq!(b.compare(&argv, &red), None, "before any change there is nothing to compare");
        b.observe(&write, &ToolResult { call_id: 2, outcome: ToolOutcome::Ok, output: None });
        // A later run is not a baseline.
        b.observe(&exec, &res(3, out(1, "FAILED t::other\n")));
        let c = b.compare(&argv, &out(1, "FAILED t::other\n")).unwrap();
        assert!(c.progressed(), "one fixed, none new");
        assert_eq!(c.fixed(), vec!["t::mine"]);
        assert!(c.note(&argv).contains("New failures since then: none. Fixed since then: t::mine."));
        let same = b.compare(&argv, &red).unwrap();
        assert!(!same.progressed(), "nothing fixed is not progress");
        let worse = b.compare(&argv, &out(1, "FAILED t::other\nFAILED t::new\n")).unwrap();
        assert!(!worse.progressed() && worse.new_failures() == vec!["t::new"]);
        assert!(b.compare(&["pytest".to_string()], &red).is_none(), "another argv has no baseline");
        let denied = ToolResult { call_id: 4, outcome: ToolOutcome::Denied { rule: "x".into() }, output: None };
        let mut b2 = Baseline::default();
        b2.observe(&write, &denied);
        b2.observe(&exec, &res(5, red.clone()));
        b2.observe(&write, &ToolResult { call_id: 6, outcome: ToolOutcome::Ok, output: None });
        assert!(b2.compare(&argv, &red).is_some(), "a refused write is not a change; the run after it is the baseline");
    }
}
