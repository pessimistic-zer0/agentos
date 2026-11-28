//! `norn diff <a> <b>`: two executions side by side (R3, v4 M25). Their
//! calls are lined up by the longest run of identical calls in the same
//! order (a longest common subsequence of the logged calls, E2), and the
//! rest is shown as calls only in one run, or as the same call with other
//! arguments, another decision or another outcome. Then the totals: time,
//! tokens, how each run ended, and whether their patches make the same
//! changes.

use std::fmt::Write as _;

use anyhow::Result;
use serde_json::Value;

use super::timing;
use crate::log::{self, CallRecord, CallState, Event, EventBody};
use crate::workspace::ExecDir;

/// One execution, read for the comparison.
pub struct Side<'a> {
    pub id: &'a str,
    pub dir: &'a ExecDir,
    pub events: &'a [Event],
}

/// What a call came to, in a few words, for comparing two runs.
fn outcome_word(events: &[Event], c: &CallRecord) -> String {
    match &c.state {
        CallState::Completed { .. } => events
            .iter()
            .find_map(|e| match &e.body {
                EventBody::ToolCompleted { call_id, summary, .. } if *call_id == c.call_id => Some(summary_kernel(summary)),
                _ => None,
            })
            .unwrap_or_else(|| "done".into()),
        CallState::Denied { rule } => format!("DENIED ({rule})"),
        CallState::AwaitingApproval { .. } => "WAITED for approval".into(),
        CallState::Failed { error } => format!("FAILED: {}", super::truncate(error, 50)),
        CallState::Reconciled { applied, .. } => format!("settled: {}", if *applied { "applied" } else { "not applied" }),
        CallState::IntentLogged { .. } => "UNCERTAIN".into(),
        CallState::Requested => "no decision".into(),
    }
}

/// A summary without the parts that change from run to run for the same
/// result: a command's time (`exit 0, 0.2s` -> `exit 0`).
fn summary_kernel(s: &str) -> String {
    match s.split_once(", ") {
        Some((head, tail)) if head.starts_with("exit ") && tail.ends_with('s') => head.to_string(),
        _ => s.to_string(),
    }
}

/// The longest common subsequence of two lists, as pairs of indexes.
fn lcs(a: &[Value], b: &[Value]) -> Vec<(usize, usize)> {
    let (n, m) = (a.len(), b.len());
    // table[i][j]: length of the LCS of a[i..] and b[j..].
    let mut table = vec![vec![0u32; m + 1]; n + 1];
    for i in (0..n).rev() {
        for j in (0..m).rev() {
            table[i][j] = if a[i] == b[j] { table[i + 1][j + 1] + 1 } else { table[i + 1][j].max(table[i][j + 1]) };
        }
    }
    let (mut i, mut j, mut out) = (0, 0, Vec::new());
    while i < n && j < m {
        if a[i] == b[j] {
            out.push((i, j));
            i += 1;
            j += 1;
        } else if table[i + 1][j] >= table[i][j + 1] {
            i += 1;
        } else {
            j += 1;
        }
    }
    out
}

/// The tool and path of a call: two calls with the same, and other
/// arguments, are shown as one changed call rather than as two.
fn place(c: &CallRecord) -> (String, Option<String>) {
    (c.call.name().to_string(), super::path_of(&c.call).map(str::to_string))
}

/// The changed lines of a patch (`+`/`-` lines, not the file headers), in
/// order. Two patches with the same changed lines make the same changes.
fn changed_lines(dir: &ExecDir) -> Option<Vec<String>> {
    let text = std::fs::read_to_string(dir.patch()).ok()?;
    Some(text.lines().filter(|l| (l.starts_with('+') || l.starts_with('-')) && !l.starts_with("+++") && !l.starts_with("---")).map(str::to_string).collect())
}

pub fn render_run_diff(a: &Side, b: &Side) -> Result<String> {
    let (ca, cb) = (log::calls(a.events)?, log::calls(b.events)?);
    let keys = |cs: &[CallRecord]| -> Result<Vec<Value>> { cs.iter().map(|c| Ok(serde_json::to_value(&c.call)?)).collect() };
    let (ka, kb) = (keys(&ca)?, keys(&cb)?);
    let pairs = lcs(&ka, &kb);
    let mut out = String::new();

    let created = |e: &[Event]| match e.first().map(|e| &e.body) {
        Some(EventBody::ExecutionCreated { repo, policy_hash, agent, task, .. }) => (repo.clone(), policy_hash.clone(), agent.clone(), task.clone()),
        _ => Default::default(),
    };
    let (ra, rb) = (created(a.events), created(b.events));
    writeln!(out, "A {} ({}, {} calls)\nB {} ({}, {} calls)", a.id, ra.2, ca.len(), b.id, rb.2, cb.len())?;
    let same = |x: bool| if x { "same" } else { "DIFFERENT" };
    writeln!(out, "  task {}, repository {}, policy {}", same(ra.3 == rb.3), same(ra.0 == rb.0), same(ra.1 == rb.1))?;
    writeln!(out, "  {} call(s) the same and in the same order\n", pairs.len())?;

    // Walk both lists, using the common calls as anchors.
    let mut lines = Vec::new();
    let mut differences = 0usize;
    let (mut i, mut j) = (0usize, 0usize);
    let anchors = pairs.iter().copied().chain(std::iter::once((ca.len(), cb.len())));
    for (ai, bj) in anchors {
        // The calls between the anchors: pair those at the same place.
        let (mut only_a, mut only_b): (Vec<usize>, Vec<usize>) = ((i..ai).collect(), (j..bj).collect());
        let mut k = 0;
        while k < only_a.len() {
            let x = only_a[k];
            if let Some(pos) = only_b.iter().position(|&y| place(&ca[x]) == place(&cb[y])) {
                let y = only_b.remove(pos);
                lines.push(format!("~ #{:<3} #{:<3} {} {}: other arguments: {}", ca[x].call_id, cb[y].call_id, ca[x].call.name(), super::truncate(&ca[x].call.describe(), 40), fields(&ka[x], &kb[y]).join(", ")));
                only_a.remove(k);
                differences += 1;
            } else {
                k += 1;
            }
        }
        for x in only_a {
            lines.push(format!("- #{:<3}      {} {}  (only in A)", ca[x].call_id, ca[x].call.name(), super::truncate(&ca[x].call.describe(), 50)));
            differences += 1;
        }
        for y in only_b {
            lines.push(format!("+      #{:<3} {} {}  (only in B)", cb[y].call_id, cb[y].call.name(), super::truncate(&cb[y].call.describe(), 50)));
            differences += 1;
        }
        if ai < ca.len() {
            let (x, y) = (&ca[ai], &cb[bj]);
            let (oa, ob) = (outcome_word(a.events, x), outcome_word(b.events, y));
            if oa == ob {
                lines.push(format!("= #{:<3} #{:<3} {} {}  {oa}", x.call_id, y.call_id, x.call.name(), super::truncate(&x.call.describe(), 50)));
            } else {
                lines.push(format!("! #{:<3} #{:<3} {} {}  A: {oa}  B: {ob}", x.call_id, y.call_id, x.call.name(), super::truncate(&x.call.describe(), 40)));
                differences += 1;
            }
        }
        i = ai + 1;
        j = bj + 1;
    }
    writeln!(out, "  =: same call, same result   !: same call, other result   ~: same tool and path, other arguments   -/+: only in A / B")?;
    for l in &lines {
        writeln!(out, "{l}")?;
    }

    // Totals.
    let sums = |events: &[Event]| {
        let t = timing::of_calls(events);
        (t.values().filter_map(|x| x.agent_ms).sum::<i64>(), t.values().filter_map(|x| x.took_ms).sum::<i64>())
    };
    let ((aa, at), (ba, bt)) = (sums(a.events), sums(b.events));
    writeln!(out, "\ntime in the agent: A {}  B {}", timing::fmt_ms(aa), timing::fmt_ms(ba))?;
    writeln!(out, "time in the tools: A {}  B {}", timing::fmt_ms(at), timing::fmt_ms(bt))?;
    let (ua, ub) = (crate::replay::usage_of(a.events), crate::replay::usage_of(b.events));
    writeln!(out, "tokens: A {}\n        B {}", ua.describe(), ub.describe())?;
    let status = |e: &[Event]| {
        e.iter().rev().find_map(|e| match &e.body {
            EventBody::ExecutionFinished { status, .. } => Some(status.clone()),
            _ => None,
        })
    };
    let (sa, sb) = (status(a.events).unwrap_or_else(|| "not finished".into()), status(b.events).unwrap_or_else(|| "not finished".into()));
    writeln!(out, "ended: A {sa}  B {sb}")?;
    if sa != sb {
        differences += 1;
    }
    match (changed_lines(a.dir), changed_lines(b.dir)) {
        (Some(pa), Some(pb)) if pa == pb => writeln!(out, "patches: the same changes ({} changed line(s))", pa.len())?,
        (Some(pa), Some(pb)) => {
            differences += 1;
            writeln!(out, "patches: DIFFERENT ({} and {} changed line(s))", pa.len(), pb.len())?
        }
        _ => writeln!(out, "patches: not both written yet")?,
    }
    if differences == 0 {
        writeln!(out, "\nno differences in calls, results, ending or patch")?;
    } else {
        writeln!(out, "\n{differences} difference(s)")?;
    }
    Ok(out)
}

fn fields(a: &Value, b: &Value) -> Vec<String> {
    let (Some(a), Some(b)) = (a.as_object(), b.as_object()) else { return vec![] };
    let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
    keys.sort();
    keys.dedup();
    keys.into_iter().filter(|k| a.get(*k) != b.get(*k)).cloned().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn the_longest_common_run_of_calls() {
        let v = |xs: &[i64]| xs.iter().map(|x| json!(x)).collect::<Vec<_>>();
        assert_eq!(lcs(&v(&[1, 2, 3, 4]), &v(&[1, 2, 3, 4])), [(0, 0), (1, 1), (2, 2), (3, 3)]);
        assert_eq!(lcs(&v(&[1, 2, 3]), &v(&[1, 9, 3])), [(0, 0), (2, 2)]);
        assert_eq!(lcs(&v(&[1, 2, 3]), &v(&[4, 1, 2, 5, 3])), [(0, 1), (1, 2), (2, 4)]);
        assert_eq!(lcs(&v(&[]), &v(&[1])), []);
    }

    #[test]
    fn a_command_s_time_is_not_a_difference() {
        assert_eq!(summary_kernel("exit 0, 0.2s"), "exit 0");
        assert_eq!(summary_kernel("exit 1, 12.0s"), "exit 1");
        assert_eq!(summary_kernel("wrote 37 bytes"), "wrote 37 bytes");
    }
}
