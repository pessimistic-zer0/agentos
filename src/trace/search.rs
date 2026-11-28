//! `norn search`: calls across every execution in the state folder (X2, v4
//! M26), by tool, path, outcome and date. Each log is read read-only, one
//! after another; there is no index, because the runs on one machine are
//! few (D-168).

use std::fmt::Write as _;
use std::path::Path;

use anyhow::{bail, Result};

use crate::log::{self, CallState, EventBody};
use crate::tools::LoggedCall;
use crate::workspace::{self, ExecDir};

/// What a call came to, as `--outcome` names it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Denied,
    Failed,
    Waiting,
    Uncertain,
    Settled,
}

impl Outcome {
    pub fn parse(s: &str) -> Result<Outcome> {
        Ok(match s {
            "done" => Outcome::Done,
            "denied" => Outcome::Denied,
            "failed" => Outcome::Failed,
            "waiting" => Outcome::Waiting,
            "uncertain" => Outcome::Uncertain,
            "settled" => Outcome::Settled,
            other => bail!("unknown outcome `{other}`: use done, denied, failed, waiting, uncertain or settled"),
        })
    }

    fn of(state: &CallState) -> Outcome {
        match state {
            CallState::Completed { .. } => Outcome::Done,
            CallState::Denied { .. } => Outcome::Denied,
            CallState::Failed { .. } => Outcome::Failed,
            CallState::AwaitingApproval { .. } => Outcome::Waiting,
            CallState::Requested | CallState::IntentLogged { .. } => Outcome::Uncertain,
            CallState::Reconciled { .. } => Outcome::Settled,
        }
    }

    fn word(self) -> &'static str {
        match self {
            Outcome::Done => "done",
            Outcome::Denied => "DENIED",
            Outcome::Failed => "FAILED",
            Outcome::Waiting => "WAITED",
            Outcome::Uncertain => "UNCERTAIN",
            Outcome::Settled => "settled",
        }
    }
}

#[derive(Debug, Default)]
pub struct Query {
    pub tool: Option<String>,
    /// A glob in the policy's syntax.
    pub path: Option<String>,
    pub outcome: Option<Outcome>,
    /// `YYYY-MM-DD`: executions created on that day or later.
    pub since: Option<String>,
}

pub fn search(state_dir: &Path, q: &Query) -> Result<String> {
    if let Some(d) = &q.since {
        let b = d.as_bytes();
        if b.len() != 10 || b[4] != b'-' || b[7] != b'-' || !d.chars().filter(|c| *c != '-').all(|c| c.is_ascii_digit()) {
            bail!("--since takes a date as YYYY-MM-DD, not `{d}`");
        }
    }
    let mut out = String::new();
    let (mut runs, mut found, mut unreadable) = (0, 0, Vec::new());
    for id in workspace::list_executions(state_dir)? {
        let dir = ExecDir::new(state_dir, &id);
        let events = match log::read_events(&dir.log()) {
            Ok(e) => e,
            Err(e) => {
                unreadable.push(format!("{id} ({e:#})"));
                continue;
            }
        };
        // Times in the log are UTC and sort as text, so a date prefix compares.
        if let (Some(since), Some(first)) = (&q.since, events.first()) {
            if first.ts.as_str() < since.as_str() {
                continue;
            }
        }
        let Ok(calls) = log::calls(&events) else {
            unreadable.push(format!("{id} (its calls do not follow the rules of the log)"));
            continue;
        };
        runs += 1;
        let agent = match events.first().map(|e| &e.body) {
            Some(EventBody::ExecutionCreated { agent, .. }) => agent.as_str(),
            _ => "?",
        };
        for c in &calls {
            let outcome = Outcome::of(&c.state);
            let keep = q.tool.as_deref().is_none_or(|t| t == c.call.name())
                && q.outcome.is_none_or(|o| o == outcome)
                && q.path.as_deref().is_none_or(|g| path_of(&c.call).is_some_and(|p| crate::policy::glob_match(g, &super::normal(p))));
            if keep {
                found += 1;
                // The rule or error says why, which is what a search for
                // refusals is usually after (found in the v4 trial, D-175).
                let why = match &c.state {
                    CallState::Denied { rule } => format!(" ({rule})"),
                    CallState::Failed { error } => format!(" ({})", super::truncate(error, 60)),
                    _ => String::new(),
                };
                writeln!(out, "{id}  {agent:<8} #{:<3} {:<9} {:<50} {}{why}", c.call_id, c.call.name(), super::truncate(&c.call.describe(), 50), outcome.word())?;
            }
        }
    }
    writeln!(out, "{found} call(s) in {runs} execution(s)")?;
    for u in unreadable {
        writeln!(out, "not read: {u}")?;
    }
    Ok(out)
}

fn path_of(call: &LoggedCall) -> Option<&str> {
    super::path_of(call)
}
