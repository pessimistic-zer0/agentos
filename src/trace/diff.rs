//! `norn trace <id> --diff [N]`: the files one call changed, as a patch from
//! the checkpoint before it to the one taken after it (C4). Without N, the
//! whole run: the base checkpoint to the last one, the same as
//! `result.patch`. Made on demand from the kept checkpoints (E5: a few
//! milliseconds), and never written into the execution's folder.

use anyhow::{bail, Context, Result};

use crate::checkpoint;
use crate::log::{self, CallState, Event};
use crate::policy::Policy;
use crate::workspace::ExecDir;

pub fn render_diff(dir: &ExecDir, events: &[Event], call: Option<u64>) -> Result<String> {
    let named = log::named_checkpoints(events);
    let Some(base) = named.first() else { bail!("this execution has no base checkpoint: it was stopped before its workspace was ready") };
    let (from, to, what) = match call {
        None => {
            let last = named.last().expect("not empty");
            (base.clone(), last.clone(), format!("the whole run so far: {base} -> {last}"))
        }
        Some(n) => {
            let calls = log::calls(events)?;
            let Some(c) = calls.iter().find(|c| c.call_id == n) else {
                bail!("there is no call #{n}; this execution has {} call(s)", calls.len());
            };
            let after = match &c.state {
                CallState::Completed { checkpoint: Some(cp), .. } | CallState::Reconciled { checkpoint: Some(cp), .. } => cp.clone(),
                CallState::Completed { checkpoint: None, .. } => {
                    return Ok(format!("call #{n} ({}) changed no files: only calls that can change files get a checkpoint\n", c.call.name()))
                }
                CallState::Reconciled { applied: false, .. } => return Ok(format!("call #{n} was cut off by a stop and recovery found it not applied: no change\n")),
                CallState::Denied { .. } | CallState::AwaitingApproval { .. } => return Ok(format!("call #{n} ({}) was not run: no change\n", c.call.name())),
                CallState::Failed { .. } => return Ok(format!("call #{n} ({}) failed, and a failed call changes nothing\n", c.call.name())),
                CallState::Requested | CallState::IntentLogged { .. } | CallState::Reconciled { .. } => {
                    return Ok(format!("call #{n} has no checkpoint after it (no ending recorded): nothing to compare\n"))
                }
            };
            let at = named.iter().position(|c| *c == after).context("the call's checkpoint is not in the log's list")?;
            let before = named[at.saturating_sub(1)].clone();
            (before.clone(), after.clone(), format!("call #{n} {} {}: {before} -> {after}", c.call.name(), super::truncate(&c.call.describe(), 60)))
        }
    };
    let (a, b) = (dir.checkpoints().join(&from), dir.checkpoints().join(&to));
    for p in [&a, &b] {
        if !p.is_dir() {
            bail!("checkpoint {} is not on disk any more", p.display());
        }
    }
    // The execution's own policy decides which paths are scratch, as it did
    // for its patch.
    let (policy, _) = Policy::load(&dir.policy())?;
    let (text, report) = checkpoint::patch_text(&a, &b, |rel| policy.scratch(rel))?;
    if !report.changed {
        return Ok(format!("# {what}\nno change to the files\n"));
    }
    Ok(format!("# {what}\n{text}"))
}
