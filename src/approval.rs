//! Approvals: calls that wait for a person's yes (M20; D-153, D-159).
//!
//! A policy names calls in `[approvals] require`. When such a call arrives
//! and the policy allows it, the gateway does not run it. It logs
//! `approval.requested` and answers the agent "waiting for approval". Nothing
//! blocks: the call is finished, and the agent may ask again (D-159).
//!
//! A pending approval is named by the **call itself**: a short hash of the
//! tool and its exact arguments as the log records them (for a write, the
//! hash of the content). The same call asked again names the same approval;
//! a call with any other argument is a different one. E4 showed why this
//! matters: with a new id per attempt the model stopped asking.
//!
//! A person answers with `norn approve <execution> <approval>` or
//! `norn deny`, from their own shell. The answer is a small file in the
//! execution folder (`approvals/<approval>.json`), which no sandboxed command
//! can see or write (design rule 9), so the agent cannot approve itself. Only
//! the runtime that drives the execution writes its event log, so the
//! person's command never touches the log; the gateway logs the answer when
//! it uses it (`approval.used`, or `approval.refused` for a no).
//!
//! An approval is good for one run of that exact call, until it expires
//! (default 60 minutes). The policy is checked again when the call comes
//! back (spec §9.12: "revalidated at execution time").

use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::log::{Event, EventBody};
use crate::tools::LoggedCall;
use crate::workspace::ExecDir;

/// The id of the approval a call needs: `a` and 10 hex digits of the
/// SHA-256 of the call as logged. `serde_json` writes object keys in a
/// fixed (sorted) order, so the same call always gives the same text.
pub fn approval_id(call: &LoggedCall) -> String {
    let text = serde_json::to_string(call).expect("a logged call serializes");
    let hash = crate::blob::content_hash(text.as_bytes());
    format!("a{}", &hash["sha256:".len().."sha256:".len() + 10])
}

/// A person's answer, as `norn approve` / `norn deny` write it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Verdict {
    pub approve: bool,
    /// Who answered: the user name of the shell that ran the command.
    pub by: String,
    /// When (RFC 3339), and until when an approval may be used (Unix seconds).
    pub at: String,
    pub expires_unix: u64,
}

fn file(dir: &ExecDir, id: &str) -> PathBuf {
    dir.root.join("approvals").join(format!("{id}.json"))
}

/// The answer on file for `id`, if any.
pub fn read(dir: &ExecDir, id: &str) -> Option<Verdict> {
    let text = std::fs::read_to_string(file(dir, id)).ok()?;
    serde_json::from_str(&text).ok()
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

/// A call that waited and has no answer that can still be used.
#[derive(Debug, Clone)]
pub struct Pending {
    pub approval: String,
    pub call_id: u64,
    pub call: LoggedCall,
    pub rule: String,
    /// For a write or an edit: the hash of the whole new file content.
    pub content: Option<String>,
}

/// Approvals asked for in this log that have not been used since they were
/// asked, newest request per id.
pub fn pending(events: &[Event]) -> Result<Vec<Pending>> {
    let calls = crate::log::calls(events)?;
    let mut out: Vec<Pending> = Vec::new();
    for ev in events {
        match &ev.body {
            EventBody::ApprovalRequested { call_id, approval, rule, content } => {
                let call = calls.iter().find(|c| c.call_id == *call_id).context("approval for an unknown call")?.call.clone();
                out.retain(|p| &p.approval != approval);
                out.push(Pending { approval: approval.clone(), call_id: *call_id, call, rule: rule.clone(), content: content.clone() });
            }
            EventBody::ApprovalUsed { approval, .. } | EventBody::ApprovalRefused { approval, .. } => out.retain(|p| &p.approval != approval),
            _ => {}
        }
    }
    Ok(out)
}

/// Has this very answer (same id, same time given) been used already?
pub fn used(events: &[Event], id: &str, at: &str) -> bool {
    events.iter().any(|e| matches!(&e.body, EventBody::ApprovalUsed { approval, granted_at, .. } if approval == id && granted_at == at))
}

/// `norn approve` / `norn deny`: write a person's answer for a pending
/// approval of this execution. Refuses an id that is not pending, so a
/// typing mistake is never a silent yes to something else.
pub fn answer(state_dir: &Path, exec_id: &str, id: &str, approve: bool, minutes: u64) -> Result<Verdict> {
    let dir = ExecDir::new(state_dir, exec_id);
    if !dir.log().exists() {
        bail!("no execution {exec_id} in {}", state_dir.display());
    }
    let log = crate::log::EventLog::open(&dir.log())?;
    let events = log.events()?;
    if !pending(&events)?.iter().any(|p| p.approval == id) {
        bail!("execution {exec_id} has no pending approval `{id}`; `norn approve {exec_id}` lists them");
    }
    let by = std::env::var("USER").or_else(|_| std::env::var("LOGNAME")).unwrap_or_else(|_| format!("uid {}", nix::unistd::getuid()));
    let v = Verdict { approve, by, at: crate::log::now_rfc3339(), expires_unix: now_unix() + minutes.saturating_mul(60) };
    let path = file(&dir, id);
    std::fs::create_dir_all(path.parent().expect("has a parent"))?;
    // Written whole, then renamed into place, so the gateway never reads half.
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_vec_pretty(&v)?)?;
    std::fs::File::open(&tmp)?.sync_all()?;
    std::fs::rename(&tmp, &path)?;
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_call_gives_the_same_id_and_any_change_another() {
        let a = LoggedCall::Exec { argv: vec!["npm".into(), "publish".into()] };
        let b = LoggedCall::Exec { argv: vec!["npm".into(), "publish".into()] };
        let c = LoggedCall::Exec { argv: vec!["npm".into(), "publish".into(), "--force".into()] };
        assert_eq!(approval_id(&a), approval_id(&b));
        assert_ne!(approval_id(&a), approval_id(&c));
        assert_eq!(approval_id(&a).len(), 11);
        let d = LoggedCall::FsDelete { path: "npm".into() };
        assert_ne!(approval_id(&a), approval_id(&d), "the tool is part of the id");
    }
}
