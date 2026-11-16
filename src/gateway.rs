//! The gateway: the only way the agent can affect anything (design rule 1).
//!
//! For each call, in this order:
//!
//! 1. `tool.requested` is logged, with the call's arguments.
//! 2. The policy decides. File paths are opened with `openat2` first, so the
//!    decision is made on the path the kernel resolved. A refusal is logged as
//!    `tool.denied` and returned.
//! 3. `tool.intent` is logged and fsynced: what is about to happen, plus what
//!    recovery will need to check it later (design rule 3).
//! 4. The tool runs (the effect). `exec` runs in the sandbox; afterwards,
//!    changes outside the write globs are put back (D-009).
//! 5. For a call that changes the workspace: a checkpoint is taken.
//! 6. `tool.completed` is logged, naming that checkpoint (design rule 4), or
//!    `tool.failed`.
//!
//! The bonus tool `ext.call` follows the same steps. Its effect is an HTTP
//! request to a service named in the policy, sent with an idempotency key
//! that is written into the intent first. It takes no checkpoint, because
//! its effect is not in the workspace.
//!
//! The `faults::point` calls mark the crash points P1..P7 of the
//! fault-injection plan. They do nothing unless a test asks for a crash.

use std::os::fd::OwnedFd;
use std::path::PathBuf;

use anyhow::Result;
use serde_json::json;

use crate::blob::BlobStore;
use crate::checkpoint;
use crate::faults;
use crate::log::{EventBody, EventLog};
use crate::policy::{Access, Decision, Policy};
use crate::sandbox::{self, cgroup::CgroupRoot, beneath, SandboxSpec, Stopped};
use crate::tools::exec::Programs;
use crate::tools::ext::{self, Answer};
use crate::tools::fs::{self as fstool, Opened, OpenedParent, WriteTarget};
use crate::tools::{ToolCall, ToolError, ToolOutcome, ToolOutput, ToolResult};
use crate::workspace::ExecDir;

/// What the sandbox needs from the runtime.
pub struct SandboxCtx {
    pub cgroups: CgroupRoot,
    /// The agentos binary, started again as the sandbox helper.
    pub helper: PathBuf,
}

pub struct Gateway {
    pub dir: ExecDir,
    pub log: EventLog,
    pub policy: Policy,
    pub blobs: BlobStore,
    programs: Programs,
    sandbox: SandboxCtx,
    lang: String,
    next_call_id: u64,
    /// The last checkpoint named in the log. The workspace equals it between
    /// calls; `exec` changes are compared against it.
    pub last_checkpoint: String,
    /// The original repository. Only read, and only for the folders in
    /// `filesystem.ro_dirs`: they are mounted into the sandbox from here,
    /// and `fs.read` / `fs.list` of paths in them are answered from here
    /// (D-122).
    repo: PathBuf,
}

/// What the policy step produced for a call that may go ahead. It holds the
/// already-opened file handles, so the effect uses exactly what was checked.
enum Approved {
    Read { fd: OwnedFd, rel: String, start_line: Option<u64>, max_lines: Option<u64> },
    List { fd: OwnedFd, rel: String },
    /// `edit_line` is set when the content came from an `fs.edit`: the line
    /// where the replaced text began.
    Write { target: WriteTarget, content: String, edit_line: Option<u64> },
    Exec { argv: Vec<String> },
    /// `url` comes from the policy rule that allowed the call, never from the agent.
    Ext { service: String, url: String, payload: String },
}

/// The outcome of the policy step.
enum Decided {
    Allow(Decision, Approved),
    Deny(Decision),
    /// Malformed request (empty argv, a name like `..`): refused like a denial.
    Invalid(String),
    /// Allowed, but cannot be done (no such file): intent + failed.
    Fail(Decision, ToolError),
}

impl Gateway {
    // One argument per thing the runtime hands over at a start or a resume;
    // a struct for them would only be built to be taken apart here.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        dir: ExecDir,
        log: EventLog,
        policy: Policy,
        programs: Programs,
        sandbox: SandboxCtx,
        next_call_id: u64,
        last_checkpoint: String,
        repo: PathBuf,
    ) -> Gateway {
        let blobs = BlobStore::new(&dir.blobs());
        // LANG is one of the three variables passed through (with PATH and HOME).
        let lang = std::env::var("LANG").unwrap_or_else(|_| "C.UTF-8".into());
        Gateway { dir, log, policy, blobs, programs, sandbox, lang, next_call_id, last_checkpoint, repo }
    }

    /// Handle one tool call from start to end. An `Err` here means the runtime
    /// itself failed (for example the log could not be written); anything the
    /// tool can go wrong with is returned inside `ToolResult`.
    pub fn handle(&mut self, call: ToolCall) -> Result<ToolResult> {
        let call_id = self.next_call_id;
        self.next_call_id += 1;

        // File content and request payloads go to the blob store before
        // anything is logged, so every hash the log mentions can be looked up.
        // Recovery needs the payload to send an uncertain request again.
        match &call {
            ToolCall::FsWrite { content, .. } => {
                self.blobs.put(content.as_bytes())?;
            }
            ToolCall::FsEdit { old_str, new_str, .. } => {
                self.blobs.put(old_str.as_bytes())?;
                self.blobs.put(new_str.as_bytes())?;
            }
            ToolCall::ExtCall { payload, .. } => {
                self.blobs.put(payload.as_bytes())?;
            }
            _ => {}
        }
        self.log.append(EventBody::ToolRequested { call_id, call: serde_json::to_value(call.logged())? })?;

        let (decision, approved) = match self.decide(&call) {
            Decided::Allow(d, a) => (d, a),
            Decided::Deny(d) => {
                self.log.append(EventBody::ToolDenied { call_id, rule: d.rule.clone(), reason: "policy".into() })?;
                return Ok(ToolResult { call_id, outcome: ToolOutcome::Denied { rule: d.rule }, output: None });
            }
            Decided::Invalid(msg) => {
                self.log.append(EventBody::ToolDenied { call_id, rule: "invalid request".into(), reason: msg.clone() })?;
                return Ok(ToolResult {
                    call_id,
                    outcome: ToolOutcome::Denied { rule: format!("invalid request: {msg}") },
                    output: None,
                });
            }
            Decided::Fail(d, e) => return self.fail_before_effect(call_id, &d, e),
        };
        // The file an edit will produce is content like any other: stored
        // before the intent names its hash.
        if let Approved::Write { content, edit_line: Some(_), .. } = &approved {
            self.blobs.put(content.as_bytes())?;
        }

        let plan = match self.plan(call_id, &approved) {
            Ok(plan) => plan,
            Err(e) => return self.fail_before_effect(call_id, &decision, e),
        };
        let mutating = call.is_mutating();
        faults::point("P1", Some(call_id)); // before the intent is written
        self.log.append(EventBody::ToolIntent { call_id, rule: decision.rule.clone(), plan })?;
        faults::point("P2", Some(call_id)); // intent is on disk, effect not started
        faults::arm("P3", Some(call_id)); // kill at a random moment during the effect

        match self.effect(call_id, approved)? {
            Ok(output) => {
                // Rule 4: effect -> checkpoint -> COMPLETED naming the checkpoint.
                let checkpoint = if mutating {
                    faults::point("P4", Some(call_id)); // effect done, no checkpoint yet
                    let name = checkpoint::name(checkpoint::number(&self.last_checkpoint).unwrap_or(0) + 1);
                    faults::arm("P5", Some(call_id)); // kill during the copy
                    if let Err(e) = checkpoint::create(&self.dir.work(), &self.dir.checkpoints(), &name, |rel| self.policy.scratch(rel)) {
                        // Only a command can leave files that cannot be
                        // copied; for it this is the call's failure (D-085).
                        if !matches!(call, ToolCall::Exec { .. }) {
                            return Err(e);
                        }
                        let e = self.undo_exec(e)?;
                        self.log.append(EventBody::ToolFailed { call_id, error: e.0.clone() })?;
                        return Ok(ToolResult { call_id, outcome: ToolOutcome::Failed { error: e.0 }, output: None });
                    }
                    faults::point("P6", Some(call_id)); // checkpoint exists, log does not name it
                    Some(name)
                } else {
                    None
                };
                let blob = self.blobs.put(&serde_json::to_vec(&output)?)?;
                self.log.append(EventBody::ToolCompleted {
                    call_id,
                    summary: output.summary(),
                    output: blob,
                    checkpoint: checkpoint.clone(),
                })?;
                if let Some(name) = checkpoint {
                    self.last_checkpoint = name;
                }
                faults::point("P7", Some(call_id)); // completed, agent has not seen the result
                Ok(ToolResult { call_id, outcome: ToolOutcome::Ok, output: Some(output) })
            }
            Err(e) => {
                self.log.append(EventBody::ToolFailed { call_id, error: e.0.clone() })?;
                Ok(ToolResult { call_id, outcome: ToolOutcome::Failed { error: e.0 }, output: None })
            }
        }
    }

    /// A call that was allowed but could not even start (for example the file
    /// to read does not exist). No effect can have happened. It still gets an
    /// intent and then a failure record, so every allowed call has an intent
    /// (D-040).
    fn fail_before_effect(&mut self, call_id: u64, decision: &Decision, e: ToolError) -> Result<ToolResult> {
        self.log.append(EventBody::ToolIntent { call_id, rule: decision.rule.clone(), plan: json!({"prepare_failed": true}) })?;
        self.log.append(EventBody::ToolFailed { call_id, error: e.0.clone() })?;
        Ok(ToolResult { call_id, outcome: ToolOutcome::Failed { error: e.0 }, output: None })
    }

    /// Check the name the agent asked for (deny rules only), then let the
    /// kernel resolve it and check the resolved path against the full policy.
    fn decide(&self, call: &ToolCall) -> Decided {
        let work = self.dir.work();
        let policy = &self.policy;
        let escape = |e: nix::errno::Errno| {
            Decided::Deny(Decision {
                allowed: false,
                rule: format!("path leaves the workspace (openat2: {})", e.desc()),
            })
        };
        // Deny rules on the requested name, before anything is opened: `.env`
        // is refused by its rule even though the file is not in the workspace.
        let requested_denied = |path: &str| {
            beneath::lexical_rel(path).and_then(|rel| policy.denied(&rel)).map(|rule| Decided::Deny(Decision { allowed: false, rule }))
        };
        match call {
            ToolCall::FsRead { path, .. } | ToolCall::FsList { path } => {
                if let Some(d) = requested_denied(path) {
                    return d;
                }
                let is_list = matches!(call, ToolCall::FsList { .. });
                // A path in a read-only folder is not in the workspace (only
                // an empty folder is). It is opened beneath the original
                // repository instead, with the same kernel confinement, and
                // must still be inside that folder once resolved (D-122).
                let ro = beneath::lexical_rel(path).and_then(|rel| policy.ro_dir(&rel).map(str::to_string));
                let root = if ro.is_some() { &self.repo } else { &work };
                match fstool::open_for_read(root, path, is_list) {
                    Ok(Opened::Outside(e)) => escape(e),
                    Ok(Opened::Inside { fd, rel }) => {
                        if let Some(dir) = &ro {
                            if policy.ro_dir(&rel) != Some(dir.as_str()) {
                                return Decided::Deny(Decision { allowed: false, rule: format!("path leaves the read-only folder `{dir}`") });
                            }
                        }
                        let d = policy.check_path(Access::Read, &rel);
                        if !d.allowed {
                            return Decided::Deny(d);
                        }
                        match call {
                            ToolCall::FsRead { start_line, max_lines, .. } => {
                                Decided::Allow(d, Approved::Read { fd, rel, start_line: *start_line, max_lines: *max_lines })
                            }
                            _ => Decided::Allow(d, Approved::List { fd, rel }),
                        }
                    }
                    Err(e) => {
                        // Not found, not a folder, ...: allowed by policy, but impossible.
                        let d = beneath::lexical_rel(path).map(|r| policy.check_path(Access::Read, &r));
                        match d {
                            Some(d) if d.allowed => Decided::Fail(d, e),
                            Some(d) => Decided::Deny(d),
                            None => Decided::Invalid(e.0),
                        }
                    }
                }
            }
            ToolCall::FsWrite { path, content } => {
                if let Some(d) = requested_denied(path) {
                    return d;
                }
                match fstool::open_for_write(&work, path) {
                    Ok(OpenedParent::Outside(e)) => escape(e),
                    Ok(OpenedParent::Inside(target)) => {
                        let d = policy.check_path(Access::Write, &target.rel);
                        if !d.allowed {
                            return Decided::Deny(d);
                        }
                        // A folder the write would make must not be a denied
                        // path either (`src/.env/x.py` would create `src/.env`).
                        if let Some(rule) = target.new_dirs().iter().find_map(|dir| policy.denied(dir)) {
                            return Decided::Deny(Decision { allowed: false, rule });
                        }
                        Decided::Allow(d, Approved::Write { target, content: content.clone(), edit_line: None })
                    }
                    Err(e) => match beneath::lexical_rel(path).map(|r| policy.check_path(Access::Write, &r)) {
                        Some(d) if d.allowed => Decided::Fail(d, e),
                        Some(d) => Decided::Deny(d),
                        None => Decided::Invalid(e.0),
                    },
                }
            }
            // An edit is a write whose content the runtime works out: the
            // file as it is now, with one piece replaced (D-120). From here
            // on it is an `fs.write` in every respect: the same intent, the
            // same atomic replace, the same recovery.
            ToolCall::FsEdit { path, old_str, new_str } => {
                if let Some(d) = requested_denied(path) {
                    return d;
                }
                match fstool::open_for_write(&work, path) {
                    Ok(OpenedParent::Outside(e)) => escape(e),
                    Ok(OpenedParent::Inside(target)) => {
                        let d = policy.check_path(Access::Write, &target.rel);
                        if !d.allowed {
                            return Decided::Deny(d);
                        }
                        // Its answers ("not found", "found twice, at these
                        // lines") say something about the file's content, so
                        // the agent must be allowed to read the file too.
                        let r = policy.check_path(Access::Read, &target.rel);
                        if !r.allowed {
                            return Decided::Deny(r);
                        }
                        let text = match fstool::current_bytes(&target) {
                            Err(e) => return Decided::Fail(d, e),
                            Ok(None) => {
                                return Decided::Fail(d, ToolError::new(format!("`{}` does not exist; fs.edit changes an existing file (use fs.write to make one)", target.rel)))
                            }
                            Ok(Some(bytes)) => match String::from_utf8(bytes) {
                                Ok(text) => text,
                                Err(_) => return Decided::Fail(d, ToolError::new(format!("`{}` is not UTF-8 text; fs.edit only edits text files", target.rel))),
                            },
                        };
                        match fstool::edited(&text, old_str, new_str, &target.rel) {
                            Ok((content, line)) => Decided::Allow(d, Approved::Write { target, content, edit_line: Some(line) }),
                            Err(e) => Decided::Fail(d, e),
                        }
                    }
                    Err(e) => match beneath::lexical_rel(path).map(|r| policy.check_path(Access::Write, &r)) {
                        Some(d) if d.allowed => Decided::Fail(d, e),
                        Some(d) => Decided::Deny(d),
                        None => Decided::Invalid(e.0),
                    },
                }
            }
            ToolCall::Exec { argv } => {
                if argv.is_empty() {
                    return Decided::Invalid("argv is empty".into());
                }
                let d = policy.check_exec(argv);
                if d.allowed { Decided::Allow(d, Approved::Exec { argv: argv.clone() }) } else { Decided::Deny(d) }
            }
            ToolCall::ExtCall { service, payload } => match policy.check_external(service) {
                (d, Some(s)) if d.allowed => {
                    Decided::Allow(d, Approved::Ext { service: service.clone(), url: s.url.clone(), payload: payload.clone() })
                }
                (d, _) => Decided::Deny(d),
            },
        }
    }

    /// What goes into the intent record. For `fs.write` this is the data that
    /// recovery needs to decide whether the write happened (D-016).
    fn plan(&self, call_id: u64, a: &Approved) -> Result<serde_json::Value, ToolError> {
        Ok(match a {
            Approved::Read { rel, start_line: None, max_lines: None, .. } | Approved::List { rel, .. } => json!({ "path": rel }),
            Approved::Read { rel, start_line, max_lines, .. } => json!({ "path": rel, "start_line": start_line, "max_lines": max_lines }),
            Approved::Write { target, content, .. } => {
                let mut plan = json!({
                    "path": target.rel,
                    "pre_hash": fstool::current_hash(target)?,
                    "new_hash": crate::blob::content_hash(content.as_bytes()),
                    "temp": fstool::temp_name(call_id),
                });
                // Folders the write will make first. Recovery rolls back if
                // it finds this and no new file (D-084).
                if !target.missing.is_empty() {
                    plan["created_dirs"] = json!(target.new_dirs());
                }
                plan
            }
            Approved::Exec { argv } => {
                let program = self
                    .programs
                    .lookup(&argv[0])
                    .ok_or_else(|| ToolError::new(format!("program `{}` was not resolved at startup", argv[0])))?;
                json!({
                    "argv": argv,
                    "program": program,
                    // Full path, so recovery can kill a group left by a crashed
                    // runtime that lived in a different systemd scope.
                    "cgroup": self.sandbox.cgroups.path_of(&sandbox::cgroup_name(&self.dir.id, call_id)),
                })
            }
            // Everything needed to send the request again after a crash,
            // exactly as it would have been sent the first time (D-077).
            Approved::Ext { service, url, payload } => json!({
                "service": service,
                "url": url,
                "key": ext::idempotency_key(&self.dir.id, call_id),
                "payload_hash": crate::blob::content_hash(payload.as_bytes()),
                "timeout_ms": self.policy.external.timeout_ms,
                "attempts": self.policy.external.attempts,
            }),
        })
    }

    /// Run the tool. Outer `Err`: the runtime failed. Inner `Err`: the tool failed.
    fn effect(&mut self, call_id: u64, a: Approved) -> Result<Result<ToolOutput, ToolError>> {
        let max = self.policy.limits.max_output_kb * 1024;
        Ok(match a {
            Approved::Read { fd, rel, start_line, max_lines } => {
                fstool::read(fd, &rel, max, start_line, max_lines).map(|(content, lines)| ToolOutput::Read { content, lines })
            }
            Approved::List { fd, rel } => fstool::list(fd, &rel).map(|entries| ToolOutput::List { entries }),
            Approved::Write { target, content, edit_line } => match self.write_fits(&target.rel, content.len() as u64) {
                Err(e) => Err(e),
                Ok(()) => fstool::write(&target, &content, call_id).map(|bytes| match edit_line {
                    Some(line) => ToolOutput::Edit { bytes, line },
                    None => ToolOutput::Write { bytes },
                }),
            },
            Approved::Exec { argv } => {
                let out = match self.run_exec(call_id, &argv)? {
                    Ok(out) => out,
                    Err(e) => return Ok(Err(e)),
                };
                Ok(out)
            }
            Approved::Ext { service, url, payload } => {
                let key = ext::idempotency_key(&self.dir.id, call_id);
                let timeout = std::time::Duration::from_millis(self.policy.external.timeout_ms);
                let (answer, sent) = ext::send_until_answered(&url, &key, &payload, timeout, self.policy.external.attempts, max as usize);
                match answer {
                    Answer::Applied { status, body } => Ok(ToolOutput::External { status, body, attempts: sent }),
                    Answer::Refused { status, body } => Err(ToolError::new(format!(
                        "service `{service}` refused the request (HTTP {status}): {}",
                        body.chars().take(200).collect::<String>()
                    ))),
                    // Neither COMPLETED nor FAILED would be true. Stop with the
                    // intent open: that is exactly the state a crash leaves, and
                    // `agentos recover` settles it the same way (D-079).
                    Answer::Unknown { why } => anyhow::bail!(
                        "call #{call_id} to external service `{service}` is UNCERTAIN: no answer after {sent} request(s) ({why}). \
                         It may or may not have been carried out. The run is stopped here. \
                         When the service is reachable, run `agentos recover {id}` (or `agentos run --resume {id}`): \
                         it sends the same request again with the same idempotency key.",
                        id = self.dir.id
                    ),
                }
            }
        })
    }

    /// The command ran, but the runtime could not finish its own bookkeeping
    /// on what it left behind (the scan for the revert, or the checkpoint
    /// copy): a file name that is not UTF-8, a file or folder with no
    /// permission bits. Code in the sandbox chose that, so it is the call
    /// that fails, not the runtime (bug B-009, D-085). The workspace is put
    /// back to the last checkpoint first, so FAILED keeps its meaning:
    /// nothing was changed. If even that cannot be done, the runtime stops,
    /// with the intent open, and `agentos recover` deals with it.
    fn undo_exec(&self, why: anyhow::Error) -> Result<ToolError> {
        use anyhow::Context;
        self.rollback_workspace().with_context(|| format!("rolling back the workspace after: {why:#}"))?;
        // Host paths mean nothing to the agent; show them from the execution folder.
        let mut text = format!("{why:#}");
        for root in [std::fs::canonicalize(&self.dir.root).ok(), Some(self.dir.root.clone())].into_iter().flatten() {
            text = text.replace(&root.display().to_string(), "<execution>");
        }
        Ok(ToolError::new(format!(
            "the command left files in the workspace that the runtime cannot record ({text}). \
             Everything this command changed was undone; the workspace is as it was before the call."
        )))
    }

    /// `fs.write` may not take the workspace past `limits.workspace_mb`
    /// either (D-117). The size of the file it replaces is looked up by
    /// name; that is an estimate for a limit, not a confinement check, which
    /// stays with `openat2`.
    fn write_fits(&self, rel: &str, len: u64) -> Result<(), ToolError> {
        use std::os::unix::fs::MetadataExt;
        let work = self.dir.work();
        let old = std::fs::symlink_metadata(work.join(rel)).map(|m| m.blocks() * 512).unwrap_or(0);
        let after = sandbox::disk_usage(&work).saturating_sub(old).saturating_add(len);
        let max = self.policy.limits.workspace_bytes();
        if after > max {
            return Err(ToolError::new(format!(
                "writing {len} bytes to `{rel}` would make the workspace larger than limits.workspace_mb ({} MiB); nothing was written",
                self.policy.limits.workspace_mb
            )));
        }
        Ok(())
    }

    /// Put the workspace back to the last checkpoint, after a command whose
    /// changes cannot be kept. If this fails the runtime stops with the
    /// intent open, and `agentos recover` rolls back the same way.
    fn rollback_workspace(&self) -> Result<()> {
        let base = self.dir.checkpoints().join(&self.last_checkpoint);
        checkpoint::restore(&self.dir.work(), &base)
    }

    fn run_exec(&mut self, call_id: u64, argv: &[String]) -> Result<Result<ToolOutput, ToolError>> {
        let program = self.programs.lookup(&argv[0]).expect("checked in plan").to_path_buf();
        let work = std::fs::canonicalize(self.dir.work())?;
        let spec = SandboxSpec {
            workspace: work.clone(),
            ro_mounts: self.policy.sandbox.ro_mounts.iter().map(PathBuf::from).collect(),
            program,
            argv: argv.to_vec(),
            // Three variables from the runtime, then the policy's own
            // (`[sandbox.env]`, fixed text; it cannot name these three).
            env: [
                ("PATH".to_string(), self.programs.search_path()),
                ("HOME".to_string(), "/work".to_string()),
                ("LANG".to_string(), self.lang.clone()),
            ]
            .into_iter()
            .chain(self.policy.sandbox.env.iter().map(|(k, v)| (k.clone(), v.clone())))
            .collect(),
            cgroup: self.sandbox.cgroups.path_of(&sandbox::cgroup_name(&self.dir.id, call_id)),
            parent_pid: std::process::id() as i32,
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
            max_file_bytes: self.policy.limits.workspace_bytes(),
            ro_dirs: self.policy.filesystem.ro_dirs.iter().map(|d| (self.repo.join(d), d.clone())).collect(),
        };
        let cg = self.sandbox.cgroups.create(&sandbox::cgroup_name(&self.dir.id, call_id), &self.policy.limits)?;
        let timeout = std::time::Duration::from_secs(self.policy.limits.call_timeout_s);
        let max = (self.policy.limits.max_output_kb * 1024) as usize;
        let disk = sandbox::DiskLimit { dir: work.clone(), max_bytes: self.policy.limits.workspace_bytes() };
        let result = sandbox::run(&self.sandbox.helper, &spec, &cg, timeout, max, &disk);
        let _ = cg.destroy();
        // Stopped while running, or over the limit when it ended (it may have
        // finished between two measurements): either way it is undone, so no
        // call leaves the workspace over the limit, and the checkpoint after
        // it never copies what it wrote (D-117).
        let over = match &result {
            Ok((_, Some(Stopped::Disk(used)))) => Some(*used),
            Ok(_) => Some(sandbox::disk_usage(&work)).filter(|u| *u > disk.max_bytes),
            Err(_) => None,
        };
        if let Some(used) = over {
            self.rollback_workspace()?;
            return Ok(Err(ToolError::new(format!(
                "the command made the workspace larger than limits.workspace_mb ({} MiB; measured {} MiB). \
                 Everything this command changed was undone; the workspace is as it was before the call.",
                self.policy.limits.workspace_mb,
                used / (1024 * 1024)
            ))));
        }
        let mut out = match result {
            Ok((out, _)) => out,
            Err(e) => return Ok(Err(e)),
        };
        // D-009: whatever the command changed outside the write globs is put back.
        let base = self.dir.checkpoints().join(&self.last_checkpoint);
        let policy = &self.policy;
        let reverted = match checkpoint::revert_outside(&work, &base, |rel| policy.command_may_change(rel)) {
            Ok(reverted) => reverted,
            Err(e) => return Ok(Err(self.undo_exec(e)?)),
        };
        if !reverted.is_empty() {
            self.log.append(EventBody::PolicyViolation { call_id, reverted: reverted.clone() })?;
        }
        out.reverted = reverted;
        Ok(Ok(ToolOutput::Exec(out)))
    }
}
