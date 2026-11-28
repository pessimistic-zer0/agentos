//! `norn replay <id>`: run an execution's agent again, on the same task,
//! and answer every call it asks for from the recording instead of running
//! it (R2, v4 M25). Nothing is run, written or sent, apart from the model's
//! own requests: the gateway, the sandbox and the state folder are never
//! touched. The replay goes on while the agent asks for exactly the calls
//! that were recorded, and stops at the first one that differs: the
//! divergence.
//!
//! Its limits are printed with every report (R4): a model may answer
//! differently to the same input; the files and programs on the machine may
//! have changed since the recording; and an MCP client's run cannot be
//! replayed, because its reasoning is not in the log.

use std::fmt::Write as _;

use anyhow::{bail, Result};
use serde_json::Value;

use crate::agent::{Action, Agent};
use crate::blob::BlobStore;
use crate::log::{self, CallState, Event, EventBody, Usage};
use crate::tools::{LoggedCall, ToolResult};
use crate::workspace::ExecDir;

/// The words printed at the top of every replay.
pub const LIMITS: &str = "Nothing is run: every call is answered from the recording. A model may answer differently to the same \
input; the machine may have changed since; the replay stops at the first call that differs from the recording.";

/// Where a replay stopped.
#[derive(Debug, Clone, PartialEq)]
pub enum End {
    /// It asked for every recorded call, then ended as the recording did.
    Same { status: String },
    /// Call `call_id` differs: the recording had `recorded`, the replay
    /// asked for `replayed`.
    Differs { call_id: u64, recorded: Value, replayed: Value },
    /// The replay asked for a call after the recording's last one.
    Longer { call_id: u64, replayed: Value },
    /// The replay ended (finished or gave up) before the recording did.
    Shorter { after_calls: u64, status: String, why: String },
    /// The recording's next call has no ending (it was cut off by a stop and
    /// never settled), so there is nothing to answer it with.
    RecordingUncertain { call_id: u64 },
    /// The replay ended as the recording did, but with another status.
    OtherEnding { recorded: String, replayed: String, why: String },
}

#[derive(Debug)]
pub struct Report {
    pub execution: String,
    pub recorded_calls: u64,
    pub matched: u64,
    pub end: End,
    /// Token counts: of the recording, and of the replay.
    pub recorded_usage: Totals,
    pub replay_usage: Totals,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub struct Totals {
    pub replies: u64,
    pub prompt_tokens: u64,
    pub completion_tokens: u64,
    pub models: Vec<String>,
}

impl Totals {
    fn add(&mut self, u: &Usage) {
        self.replies += 1;
        self.prompt_tokens += u.prompt_tokens;
        self.completion_tokens += u.completion_tokens;
        if !self.models.contains(&u.model) {
            self.models.push(u.model.clone());
        }
    }

    pub fn describe(&self) -> String {
        if self.replies == 0 {
            return "not recorded".into();
        }
        format!("{} reply(ies), {} prompt + {} answer tokens, model {}", self.replies, self.prompt_tokens, self.completion_tokens, self.models.join(", "))
    }
}

/// The token counts stored in a log.
pub fn usage_of(events: &[Event]) -> Totals {
    let mut t = Totals::default();
    for e in events {
        if let EventBody::AgentMessage { usage: Some(u), .. } = &e.body {
            t.add(u);
        }
    }
    t
}

/// Replay `dir`'s execution with `agent`, which must be new (not restored).
pub fn replay(dir: &ExecDir, agent: &mut dyn Agent) -> Result<Report> {
    let events = log::read_events(&dir.log())?;
    let Some(EventBody::ExecutionCreated { execution_id, agent: kind, .. }) = events.first().map(|e| &e.body) else {
        bail!("{} has no execution.created event", dir.log().display())
    };
    if kind == "mcp" {
        bail!("execution {execution_id} was driven by an MCP client; its reasoning is not in the log, so it cannot be replayed");
    }
    let blobs = BlobStore::new(&dir.blobs());
    let calls = log::calls(&events)?;
    let recorded_status = events.iter().rev().find_map(|e| match &e.body {
        EventBody::ExecutionFinished { status, .. } => Some(status.clone()),
        _ => None,
    });
    let mut replay_usage = Totals::default();
    let mut last: Option<ToolResult> = None;
    let mut i = 0usize;
    let end = loop {
        let action = agent.next_action(last.as_ref());
        for (_, usage) in agent.take_messages() {
            if let Some(u) = usage {
                replay_usage.add(&u);
            }
        }
        match action? {
            Action::Call(call) => {
                let replayed = serde_json::to_value(call.logged())?;
                let Some(rec) = calls.get(i) else {
                    break End::Longer { call_id: i as u64 + 1, replayed };
                };
                let recorded = serde_json::to_value(&rec.call)?;
                if recorded != replayed {
                    break End::Differs { call_id: rec.call_id, recorded, replayed };
                }
                let Some(outcome) = rec.outcome() else {
                    break End::RecordingUncertain { call_id: rec.call_id };
                };
                let output = match &rec.state {
                    CallState::Completed { output, .. } => blobs.get(output).ok().and_then(|b| serde_json::from_slice(&b).ok()),
                    _ => None,
                };
                let mut outcome = outcome;
                if let crate::tools::ToolOutcome::Waiting { execution, .. } = &mut outcome {
                    execution.clone_from(execution_id);
                }
                last = Some(ToolResult { call_id: rec.call_id, outcome, output });
                i += 1;
            }
            Action::Finish { summary } => break ending(i, calls.len(), "completed", &summary, &recorded_status),
            Action::GiveUp { reason } => break ending(i, calls.len(), "stopped", &reason, &recorded_status),
        }
    };
    Ok(Report {
        execution: execution_id.clone(),
        recorded_calls: calls.len() as u64,
        matched: i as u64,
        end,
        recorded_usage: usage_of(&events),
        replay_usage,
    })
}

fn ending(done: usize, recorded: usize, status: &str, why: &str, recorded_status: &Option<String>) -> End {
    if done < recorded {
        return End::Shorter { after_calls: done as u64, status: status.into(), why: why.into() };
    }
    match recorded_status {
        Some(r) if r != status => End::OtherEnding { recorded: r.clone(), replayed: status.into(), why: why.into() },
        Some(r) => End::Same { status: r.clone() },
        // The recording never finished; the replay did, after the same calls.
        None => End::OtherEnding { recorded: "not finished".into(), replayed: status.into(), why: why.into() },
    }
}

/// The report for people.
pub fn render(r: &Report) -> Result<String> {
    let mut out = String::new();
    writeln!(out, "replay of {}", r.execution)?;
    writeln!(out, "  {LIMITS}")?;
    writeln!(out, "  recorded: {} call(s); tokens {}", r.recorded_calls, r.recorded_usage.describe())?;
    writeln!(out, "  replay:   tokens {}", r.replay_usage.describe())?;
    writeln!(out)?;
    writeln!(out, "the replay asked for the same {} call(s) as the recording", r.matched)?;
    match &r.end {
        End::Same { status } => writeln!(out, "and then ended the same way ({status}): no divergence")?,
        End::Differs { call_id, recorded, replayed } => {
            writeln!(out, "DIVERGENCE at call #{call_id}:")?;
            writeln!(out, "  recorded  {}", describe(recorded))?;
            writeln!(out, "  replayed  {}", describe(replayed))?;
            let fields = differing_fields(recorded, replayed);
            if !fields.is_empty() {
                writeln!(out, "  differs in: {}", fields.join(", "))?;
            }
        }
        End::Longer { call_id, replayed } => {
            writeln!(out, "DIVERGENCE: the recording ended here, and the replay asked for call #{call_id}: {}", describe(replayed))?
        }
        End::Shorter { after_calls, status, why } => writeln!(
            out,
            "DIVERGENCE: the replay ended ({status}) after {after_calls} call(s); the recording went on to {} call(s). Its words: {}",
            r.recorded_calls,
            crate::runtime::truncate(why, 200)
        )?,
        End::RecordingUncertain { call_id } => writeln!(
            out,
            "and stopped at call #{call_id}: the recording has no ending for it (cut off by a stop, not settled), so there is nothing to answer with"
        )?,
        End::OtherEnding { recorded, replayed, why } => writeln!(
            out,
            "DIVERGENCE at the end: the recording ended {recorded}, the replay {replayed}. Its words: {}",
            crate::runtime::truncate(why, 200)
        )?,
    }
    Ok(out)
}

fn describe(call: &Value) -> String {
    match serde_json::from_value::<LoggedCall>(call.clone()) {
        Ok(c) => format!("{} {}", c.name(), c.describe()),
        Err(_) => call.to_string(),
    }
}

/// The names of the fields whose values differ between two logged calls.
fn differing_fields(a: &Value, b: &Value) -> Vec<String> {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else { return vec![] };
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter().filter(|k| a.get(*k) != b.get(*k)).cloned().collect()
}
