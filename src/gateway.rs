//! The gateway: the only way the agent can affect anything (design rule 1).
//!
//! For each call, in this order:
//!
//! 1. `tool.requested` is logged, with the call's arguments.
//! 2. The policy decides. A refusal is logged as `tool.denied` and returned.
//! 3. `tool.intent` is logged and fsynced: what is about to happen, plus what
//!    recovery will need to check it later (design rule 3).
//! 4. The tool runs (the effect).
//! 5. `tool.completed` (or `tool.failed`) is logged.
//!
//! The steps are numbered P1..P7 in the fault-injection plan; M3 adds the
//! crash points and the checkpoint between 4 and 5.

use anyhow::Result;
use serde_json::json;

use crate::blob::BlobStore;
use crate::log::{EventBody, EventLog};
use crate::policy::{Access, Decision, Policy};
use crate::tools::exec::{self, ExecRequest, Programs};
use crate::tools::fs::{self as fstool, Resolved};
use crate::tools::{ToolCall, ToolError, ToolOutcome, ToolOutput, ToolResult};
use crate::workspace::ExecDir;

pub struct Gateway {
    pub dir: ExecDir,
    pub log: EventLog,
    pub policy: Policy,
    pub blobs: BlobStore,
    programs: Programs,
    lang: String,
    next_call_id: u64,
}

/// What the policy step produced for a call that is allowed to go ahead.
enum Approved {
    Read(Resolved),
    List(Resolved),
    Write { target: Resolved, content: String },
    Exec { argv: Vec<String> },
}

impl Gateway {
    pub fn new(dir: ExecDir, log: EventLog, policy: Policy, programs: Programs, next_call_id: u64) -> Gateway {
        let blobs = BlobStore::new(&dir.blobs());
        // LANG is one of the three variables passed through (with PATH and HOME).
        let lang = std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into());
        Gateway { dir, log, policy, blobs, programs, lang, next_call_id }
    }

    pub fn programs(&self) -> &Programs {
        &self.programs
    }

    /// Handle one tool call from start to end. An `Err` here means the runtime
    /// itself failed (for example the log could not be written); anything the
    /// tool can go wrong with is returned inside `ToolResult`.
    pub fn handle(&mut self, call: ToolCall) -> Result<ToolResult> {
        let call_id = self.next_call_id;
        self.next_call_id += 1;

        if let ToolCall::FsWrite { content, .. } = &call {
            // The new content goes to the blob store before anything is logged,
            // so every hash the log mentions can be looked up.
            self.blobs.put(content.as_bytes())?;
        }
        self.log.append(EventBody::ToolRequested { call_id, call: serde_json::to_value(call.logged())? })?;

        let approved = match self.decide(&call) {
            Ok(Ok((decision, approved))) => (decision, approved),
            Ok(Err(decision)) => {
                self.log.append(EventBody::ToolDenied {
                    call_id,
                    rule: decision.rule.clone(),
                    reason: "policy".into(),
                })?;
                return Ok(ToolResult { call_id, outcome: ToolOutcome::Denied { rule: decision.rule }, output: None });
            }
            Err(bad) => {
                // The request itself is malformed (for example `..` climbing out).
                // It is refused before any intent, like a policy denial.
                self.log.append(EventBody::ToolDenied { call_id, rule: "invalid request".into(), reason: bad.0.clone() })?;
                return Ok(ToolResult {
                    call_id,
                    outcome: ToolOutcome::Denied { rule: format!("invalid request: {}", bad.0) },
                    output: None,
                });
            }
        };
        let (decision, approved) = approved;

        let plan = match self.plan(call_id, &approved) {
            Ok(plan) => plan,
            Err(e) => return self.fail_before_intent(call_id, &decision, e),
        };
        self.log.append(EventBody::ToolIntent { call_id, rule: decision.rule.clone(), plan })?;

        match self.effect(call_id, approved) {
            Ok(output) => {
                let blob = self.blobs.put(&serde_json::to_vec(&output)?)?;
                self.log.append(EventBody::ToolCompleted {
                    call_id,
                    summary: output.summary(),
                    output: blob,
                    checkpoint: None,
                })?;
                Ok(ToolResult { call_id, outcome: ToolOutcome::Ok, output: Some(output) })
            }
            Err(e) => {
                self.log.append(EventBody::ToolFailed { call_id, error: e.0.clone() })?;
                Ok(ToolResult { call_id, outcome: ToolOutcome::Failed { error: e.0 }, output: None })
            }
        }
    }

    /// A call that was allowed but failed while preparing (for example the
    /// file to read does not exist). No effect can have started, so it gets
    /// an intent and a failure record straight after, keeping the state
    /// machine simple: every allowed call has an intent.
    fn fail_before_intent(&mut self, call_id: u64, decision: &Decision, e: ToolError) -> Result<ToolResult> {
        self.log.append(EventBody::ToolIntent { call_id, rule: decision.rule.clone(), plan: json!({"prepare_failed": true}) })?;
        self.log.append(EventBody::ToolFailed { call_id, error: e.0.clone() })?;
        Ok(ToolResult { call_id, outcome: ToolOutcome::Failed { error: e.0 }, output: None })
    }

    /// Policy step. Outer `Err`: the request is malformed. Inner `Err`: the
    /// policy denies it.
    #[allow(clippy::type_complexity)]
    fn decide(&self, call: &ToolCall) -> Result<Result<(Decision, Approved), Decision>, ToolError> {
        let work = self.dir.work();
        let check = |access: Access, r: &Resolved| self.policy.check_path(access, &r.rel);
        Ok(match call {
            ToolCall::FsRead { path } => {
                let r = fstool::resolve(&work, path)?;
                let d = check(Access::Read, &r);
                if d.allowed { Ok((d, Approved::Read(r))) } else { Err(d) }
            }
            ToolCall::FsList { path } => {
                let r = fstool::resolve(&work, path)?;
                let d = check(Access::Read, &r);
                if d.allowed { Ok((d, Approved::List(r))) } else { Err(d) }
            }
            ToolCall::FsWrite { path, content } => {
                let r = fstool::resolve(&work, path)?;
                let d = check(Access::Write, &r);
                if d.allowed { Ok((d, Approved::Write { target: r, content: content.clone() })) } else { Err(d) }
            }
            ToolCall::Exec { argv } => {
                if argv.is_empty() {
                    return Err(ToolError::new("argv is empty"));
                }
                let d = self.policy.check_exec(argv);
                if d.allowed { Ok((d, Approved::Exec { argv: argv.clone() })) } else { Err(d) }
            }
        })
    }

    /// What goes into the intent record. For `fs.write` this is the data that
    /// recovery needs to decide whether the write happened (D-016).
    fn plan(&self, call_id: u64, a: &Approved) -> Result<serde_json::Value, ToolError> {
        Ok(match a {
            Approved::Read(r) | Approved::List(r) => json!({ "path": r.rel }),
            Approved::Write { target, content } => json!({
                "path": target.rel,
                "pre_hash": fstool::current_hash(target)?,
                "new_hash": crate::blob::content_hash(content.as_bytes()),
                "temp": fstool::temp_name(call_id),
            }),
            Approved::Exec { argv } => {
                let program = self
                    .programs
                    .lookup(&argv[0])
                    .ok_or_else(|| ToolError::new(format!("program `{}` was not resolved at startup", argv[0])))?;
                json!({ "argv": argv, "program": program })
            }
        })
    }

    fn effect(&mut self, call_id: u64, a: Approved) -> Result<ToolOutput, ToolError> {
        let max = self.policy.limits.max_output_kb * 1024;
        match a {
            Approved::Read(r) => Ok(ToolOutput::Read { content: fstool::read(&r, max)? }),
            Approved::List(r) => Ok(ToolOutput::List { entries: fstool::list(&r)? }),
            Approved::Write { target, content } => Ok(ToolOutput::Write { bytes: fstool::write(&target, &content, call_id)? }),
            Approved::Exec { argv } => {
                let program = self.programs.lookup(&argv[0]).expect("checked in plan").to_path_buf();
                let work = self.dir.work();
                let env = vec![
                    ("PATH".to_string(), self.programs.search_path()),
                    ("HOME".to_string(), work.display().to_string()),
                    ("LANG".to_string(), self.lang.clone()),
                ];
                let req = ExecRequest {
                    program: &program,
                    argv: &argv,
                    cwd: &work,
                    env,
                    timeout: std::time::Duration::from_secs(self.policy.limits.call_timeout_s),
                    max_output: max as usize,
                };
                Ok(ToolOutput::Exec(exec::run_direct(&req)?))
            }
        }
    }
}
