//! Fault-injection harness (v1 "Evaluation", suite 1: the headline result).
//!
//! For each crash point P1..P8 and each tool, many times over:
//!
//! 1. run the scripted agent with `AGENTOS_CRASH_AT=<point>` so the runtime
//!    kills itself (SIGKILL) at that point while handling a chosen call;
//! 2. look at what was left behind (before recovery touches it);
//! 3. run `agentos recover`, timing it;
//! 4. check the five invariants from the v1 design;
//! 5. run `agentos run --resume` to the end and check the final workspace is
//!    exactly what an uninterrupted run produces (no lost and no doubled effect).
//!
//! Runs per (point, tool): `AGENTOS_RECOVERY_RUNS` (default 3, so `cargo test`
//! stays quick; `make eval` uses 200). Parallel workers: `AGENTOS_RECOVERY_JOBS`
//! (default 8). The report goes to stdout and to `target/recovery-report.json`.
//!
//! Needs a delegated cgroup:
//!   systemd-run --user --scope -p Delegate=yes cargo test --test recovery

#[path = "../common/mod.rs"]
mod common;

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::process::ExitStatusExt;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use agentos::blob::content_hash;
use common::*;
use serde_json::{json, Value};

// ---------------------------------------------------------------------------
// The workload: seven steps that use all four tools.
// ---------------------------------------------------------------------------

/// Python run by the two `exec` steps. Each run adds one line to two files,
/// with a pause in between, so a kill can land between the two writes, and a
/// doubled run would show as an extra line.
const APPEND: &str = "import time\nf = open('src/counter.txt', 'a'); f.write('x\\n'); f.flush()\ntime.sleep(0.03)\nopen('src/side.txt', 'a').write('y\\n')";

fn big(tag: &str) -> String {
    // ~400 KB, so the temp-file write takes long enough for a kill to land in it.
    format!("{tag} line of text for the fault-injection workload\n").repeat(8000)
}

/// (tool name, step indexes that use it). Call id on a first run = index + 1.
const TOOL_STEPS: [(&str, &[usize]); 4] = [("fs.list", &[0]), ("fs.write", &[1, 4, 6]), ("exec", &[2, 5]), ("fs.read", &[3])];

fn script_text() -> String {
    let w = |path: &str, content: &str| format!("[[step]]\ntool = \"fs.write\"\npath = \"{path}\"\ncontent = '''\n{content}'''\n");
    let exec = format!("[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", '''\n{APPEND}\n''']\n");
    format!(
        "task = \"fault-injection workload\"\n[[step]]\ntool = \"fs.list\"\npath = \"src\"\n{}{exec}[[step]]\ntool = \"fs.read\"\npath = \"src/small.txt\"\n{}{exec}{}",
        w("src/a.txt", &big("A1")),
        w("src/a.txt", &big("A2")),
        w("src/new.txt", "new\n"),
    )
}

fn policy_text() -> String {
    let base = std::fs::read_to_string(preset_policy()).unwrap();
    let mut out = String::new();
    let mut skip = false;
    for line in base.lines() {
        if line.starts_with("allow = [") {
            skip = true;
            out.push_str("allow = [[\"python3\", \"-c\"]]\n");
            continue;
        }
        if skip {
            skip = !line.starts_with(']');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

fn make_repo(root: &Path) {
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::write(root.join("src/a.txt"), "A0\n").unwrap();
    std::fs::write(root.join("src/small.txt"), "small\n").unwrap();
    std::fs::write(root.join("README.md"), "workload repo\n").unwrap();
}

// ---------------------------------------------------------------------------
// Tiny random numbers (no crate needed): xorshift64*.
// ---------------------------------------------------------------------------

struct Rng(u64);
impl Rng {
    fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
}

// ---------------------------------------------------------------------------
// One trial.
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Job {
    point: &'static str,
    tool: &'static str,
    n: usize,
}

#[derive(Default, Clone)]
struct Outcome {
    crashed: bool,
    /// Recovery itself was killed at P8 (only for P8 jobs).
    recovery_crashed: bool,
    violations: Vec<String>,
    duplicates: u64,
    /// (classified, correctly) for the cut-off call, if there was one.
    classified: Option<bool>,
    recover_ms: Option<f64>,
}

fn events_of(t: &TestEnv, id: &str) -> Vec<Value> {
    t.events(id)
}

fn file_hash(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| content_hash(&b))
}

fn lines(p: &Path) -> u64 {
    std::fs::read_to_string(p).map(|s| s.lines().count() as u64).unwrap_or(0)
}

fn named_checkpoints(events: &[Value]) -> Vec<String> {
    events
        .iter()
        .filter(|e| ["workspace.created", "tool.completed", "tool.reconciled"].contains(&e["type"].as_str().unwrap()))
        .filter_map(|e| e["checkpoint"].as_str().map(str::to_string))
        .collect()
}

/// Call ids that have an intent but no ending, and those with no ending at all.
fn pending(events: &[Value]) -> (Vec<u64>, Vec<u64>) {
    let mut open: BTreeMap<u64, bool> = BTreeMap::new(); // id -> has intent
    for e in events {
        let Some(id) = e["call_id"].as_u64() else { continue };
        match e["type"].as_str().unwrap() {
            "tool.requested" => {
                open.insert(id, false);
            }
            "tool.intent" => {
                open.insert(id, true);
            }
            "tool.denied" | "tool.completed" | "tool.failed" | "tool.reconciled" => {
                open.remove(&id);
            }
            _ => {}
        }
    }
    (open.iter().filter(|(_, i)| **i).map(|(k, _)| *k).collect(), open.keys().copied().collect())
}

fn call_of(events: &[Value], id: u64) -> Value {
    events.iter().find(|e| e["type"] == "tool.requested" && e["call_id"] == id).map(|e| e["call"].clone()).unwrap()
}

fn intent_of(events: &[Value], id: u64) -> Option<Value> {
    events.iter().find(|e| e["type"] == "tool.intent" && e["call_id"] == id).map(|e| e["plan"].clone())
}

/// What the workspace must contain, worked out from the log alone: start
/// from the repo and apply every call that ended as applied, in order.
struct Model {
    files: BTreeMap<String, String>, // path -> content hash
    execs: u64,
}

fn model(events: &[Value]) -> Model {
    let mut m = Model { files: BTreeMap::new(), execs: 0 };
    m.files.insert("src/a.txt".into(), content_hash(b"A0\n"));
    for e in events {
        let applied = match e["type"].as_str().unwrap() {
            "tool.completed" => true,
            "tool.reconciled" => e["applied"] == true,
            _ => false,
        };
        if !applied {
            continue;
        }
        let call = call_of(events, e["call_id"].as_u64().unwrap());
        match call["tool"].as_str().unwrap() {
            "fs.write" => {
                m.files.insert(call["path"].as_str().unwrap().into(), call["content_hash"].as_str().unwrap().into());
            }
            "exec" => m.execs += 1,
            _ => {}
        }
    }
    m
}

fn check_model(work: &Path, m: &Model, v: &mut Vec<String>, when: &str) {
    for path in ["src/a.txt", "src/new.txt"] {
        let got = file_hash(&work.join(path));
        let want = m.files.get(path).cloned();
        if got != want {
            v.push(format!("I3 {when}: {path} does not match the applied calls in the log"));
        }
    }
    for (path, what) in [("src/counter.txt", "counter"), ("src/side.txt", "side")] {
        let got = lines(&work.join(path));
        if got != m.execs {
            v.push(format!("I3 {when}: {what} has {got} lines, the log says {} applied exec calls", m.execs));
        }
    }
}

/// Invariants that must hold after recovery (and after the resumed run).
fn check_invariants(t: &TestEnv, id: &str, events: &[Value], v: &mut Vec<String>, when: &str) {
    let dir = t.exec_dir(id);
    let work = dir.join("work");
    // I5: the log is a gap-free prefix. (`agentos trace` already refused any
    // illegal order of records, or `t.events` would have panicked.)
    let seqs: Vec<u64> = events.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    if seqs != (1..=seqs.len() as u64).collect::<Vec<_>>() {
        v.push(format!("I5 {when}: sequence numbers have a gap"));
    }
    // I4: no intent is written while an older call is still unsettled.
    let mut open: BTreeSet<u64> = BTreeSet::new();
    for e in events {
        let Some(cid) = e["call_id"].as_u64() else { continue };
        match e["type"].as_str().unwrap() {
            "tool.requested" => {
                if !open.is_empty() {
                    v.push(format!("I4 {when}: call {cid} requested while {open:?} unsettled"));
                }
                open.insert(cid);
            }
            "tool.denied" | "tool.completed" | "tool.failed" | "tool.reconciled" => {
                open.remove(&cid);
            }
            _ => {}
        }
    }
    if !open.is_empty() {
        v.push(format!("I4 {when}: calls {open:?} have no ending after recovery"));
    }
    // I2: every completed (or applied) mutating call names a checkpoint that
    // exists and that holds the call's effect.
    for e in events {
        let done = e["type"] == "tool.completed" || (e["type"] == "tool.reconciled" && e["applied"] == true);
        if !done {
            continue;
        }
        let cid = e["call_id"].as_u64().unwrap();
        let call = call_of(events, cid);
        let tool = call["tool"].as_str().unwrap();
        if tool != "fs.write" && tool != "exec" {
            continue;
        }
        let Some(cp) = e["checkpoint"].as_str() else {
            v.push(format!("I2 {when}: call {cid} is done but names no checkpoint"));
            continue;
        };
        let cp_dir = dir.join("checkpoints").join(cp);
        if !cp_dir.is_dir() {
            v.push(format!("I2 {when}: checkpoint {cp} of call {cid} is missing"));
        } else if tool == "fs.write" {
            let path = call["path"].as_str().unwrap();
            if file_hash(&cp_dir.join(path)).as_deref() != call["content_hash"].as_str() {
                v.push(format!("I2 {when}: checkpoint {cp} does not hold the write of call {cid}"));
            }
        }
    }
    // I3: the workspace equals the last checkpoint, and both equal what the
    // applied calls in the log add up to.
    let named = named_checkpoints(events);
    let last = dir.join("checkpoints").join(named.last().unwrap());
    if tree_digest(&work) != tree_digest(&last) {
        v.push(format!("I3 {when}: workspace differs from the last checkpoint {}", named.last().unwrap()));
    }
    check_model(&work, &model(events), v, when);
    // No leftovers: only named checkpoints on disk, no temp folders.
    let mut on_disk: Vec<String> =
        std::fs::read_dir(dir.join("checkpoints")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    on_disk.sort();
    let mut want = named.clone();
    want.sort();
    if on_disk != want {
        v.push(format!("leftover {when}: checkpoints on disk {on_disk:?}, named in the log {want:?}"));
    }
    if dir.join("work.restore-tmp").exists() {
        v.push(format!("leftover {when}: work.restore-tmp"));
    }
}

fn run_trial(job: &Job, seed: u64) -> Outcome {
    let mut rng = Rng::new(seed);
    let mut out = Outcome::default();
    let name = format!("rec-{}-{}-{}", job.point, job.tool.replace('.', ""), job.n);
    let t = TestEnv::new(&name).with_policy_text(&policy_text());
    let repo = t.root.join("repo");
    make_repo(&repo);
    let script = t.write_script("workload.toml", &script_text());
    let steps = TOOL_STEPS.iter().find(|(tool, _)| *tool == job.tool).unwrap().1;
    let call = steps[rng.below(steps.len() as u64) as usize] as u64 + 1;

    // P8 crashes recovery, so first crash the run somewhere recovery has work.
    let first_point = if job.point == "P8" { ["P2", "P3", "P4", "P5", "P6"][rng.below(5) as usize] } else { job.point };
    let delay_us = match (first_point, job.tool) {
        ("P3", "exec") => 120_000,
        ("P3", "fs.write") => 4_000,
        ("P5", _) => 4_000,
        _ => 1_500,
    };
    let status = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .env("AGENTOS_CRASH_AT", first_point)
        .env("AGENTOS_CRASH_CALL", call.to_string())
        .env("AGENTOS_CRASH_DELAY_US", delay_us.to_string())
        .output()
        .unwrap();
    out.crashed = status.status.signal() == Some(9);
    if !out.crashed && !status.status.success() {
        out.violations.push(format!("run failed without being killed: {}", String::from_utf8_lossy(&status.stderr)));
        return out;
    }
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let dir = t.exec_dir(&id);
    let work = dir.join("work");

    // --- Before recovery: what did the crash leave? ---
    let before = events_of(&t, &id);
    let (with_intent, all_open) = pending(&before);
    if with_intent.is_empty() {
        // I1: no effect without a durable intent. With no intent open, the
        // workspace must still equal the last checkpoint the log names.
        let last = dir.join("checkpoints").join(named_checkpoints(&before).last().unwrap());
        if tree_digest(&work) != tree_digest(&last) {
            out.violations.push("I1: workspace changed although no call has an open intent".into());
        }
    }
    // Ground truth for the cut-off call, taken from the files, not the runtime.
    let expect_applied: Option<(u64, bool)> = all_open.first().map(|cid| {
        let c = call_of(&before, *cid);
        let applied = c["tool"] == "fs.write"
            && intent_of(&before, *cid).is_some()
            && file_hash(&work.join(c["path"].as_str().unwrap())).as_deref() == c["content_hash"].as_str();
        (*cid, applied)
    });

    // --- Recovery (for P8: killed part-way first, then run again). ---
    if job.point == "P8" {
        let st = t.cmd(&["recover", "--quiet", &id]).env("AGENTOS_CRASH_AT", "P8").env("AGENTOS_CRASH_HIT", (1 + rng.below(5)).to_string()).output().unwrap();
        out.recovery_crashed = st.status.signal() == Some(9);
    }
    let started = Instant::now();
    let st = t.output(&["recover", "--quiet", &id]);
    out.recover_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
    if !st.status.success() {
        out.violations.push(format!("recover failed: {}", String::from_utf8_lossy(&st.stderr)));
        return out;
    }
    let after = events_of(&t, &id);
    check_invariants(&t, &id, &after, &mut out.violations, "after recovery");
    if let Some((cid, want)) = expect_applied {
        let got = after.iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == cid).map(|e| e["applied"] == true);
        out.classified = Some(got == Some(want));
        if got != Some(want) {
            out.violations.push(format!("call {cid}: files say applied={want}, recovery said {got:?}"));
        }
    }

    // --- Resume to the end: nothing lost, nothing doubled. ---
    let st = t.output(&["run", "--quiet", "--resume", &id]);
    if !st.status.success() {
        out.violations.push(format!("resume failed: {}", String::from_utf8_lossy(&st.stderr)));
        return out;
    }
    let end = events_of(&t, &id);
    check_invariants(&t, &id, &end, &mut out.violations, "after resume");
    if !end.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed") {
        out.violations.push("resumed run did not complete".into());
    }
    let (counter, side) = (lines(&work.join("src/counter.txt")), lines(&work.join("src/side.txt")));
    out.duplicates = counter.saturating_sub(2) + side.saturating_sub(2);
    let want = [("src/a.txt", content_hash(big("A2").as_bytes())), ("src/new.txt", content_hash(b"new\n"))];
    for (p, h) in want {
        if file_hash(&work.join(p)).as_deref() != Some(h.as_str()) {
            out.violations.push(format!("final: {p} is not what an uninterrupted run writes"));
        }
    }
    if counter != 2 || side != 2 {
        out.violations.push(format!("final: counter={counter} side={side}, expected 2 and 2"));
    }
    if out.violations.is_empty() {
        let _ = std::fs::remove_dir_all(&t.root); // keep only failing cases on disk
    }
    out
}

/// Regression test for B-007. The checkpoint copy is a `cp` child process.
/// When the runtime is killed during the copy, that child must die with it.
/// If it lives on, it creates its `.tmp-cp-…` folder *after* recovery has
/// cleaned up. The window is normally well under a millisecond, so a slow
/// `cp` (a wrapper script first in PATH that sleeps, then runs the real one)
/// makes it wide enough to hit every time.
#[test]
fn checkpoint_copy_dies_with_the_runtime() {
    let t = TestEnv::new("rec-orphan-cp").with_policy_text(&policy_text());
    let repo = t.root.join("repo");
    make_repo(&repo);
    let script = t.write_script("workload.toml", &script_text());
    let host_path = std::env::var("PATH").unwrap();
    let real_cp = host_path.split(':').map(|d| Path::new(d).join("cp")).find(|p| p.is_file()).expect("cp on PATH");
    let shim_dir = t.root.join("shim");
    std::fs::create_dir_all(&shim_dir).unwrap();
    let shim = shim_dir.join("cp");
    std::fs::write(&shim, format!("#!/bin/sh\nsleep 0.4\nexec {} \"$@\"\n", real_cp.display())).unwrap();
    std::fs::set_permissions(&shim, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let slow_path = format!("{}:{host_path}", shim_dir.display());

    // Call 2 is the first fs.write; kill within 1 ms of starting its checkpoint copy.
    let st = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .env("PATH", &slow_path)
        .env("AGENTOS_CRASH_AT", "P5")
        .env("AGENTOS_CRASH_CALL", "2")
        .env("AGENTOS_CRASH_DELAY_US", "1000")
        .output()
        .unwrap();
    assert_eq!(st.status.signal(), Some(9), "the run must be killed during the copy: {}", String::from_utf8_lossy(&st.stderr));
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();

    // Recovery uses the normal cp, straight away, as a person or script would.
    t.agentos(&["recover", "--quiet", &id]);
    // Longer than the wrapper's sleep: a copy that survived would be done by now.
    std::thread::sleep(Duration::from_millis(900));
    let mut v = Vec::new();
    check_invariants(&t, &id, &events_of(&t, &id), &mut v, "after recovery and a pause");
    assert!(v.is_empty(), "{v:?}");
    t.agentos(&["run", "--quiet", "--resume", &id]);
    check_invariants(&t, &id, &events_of(&t, &id), &mut v, "after resume");
    assert!(v.is_empty(), "{v:?}");
}

/// Regression test for B-008. Right after a kill, the execution's lock can
/// still be held for a moment by a child of the dead runtime that was caught
/// between `fork` and `exec`. Recovery must wait for it instead of failing.
/// The test plays that child: it holds the lock itself and lets go after
/// 0.4 s. A lock that is never released must still be reported.
#[test]
fn recovery_waits_for_a_lock_that_is_about_to_be_released() {
    let t = TestEnv::new("rec-lock-wait").with_policy_text(&policy_text());
    let repo = t.root.join("repo");
    make_repo(&repo);
    let script = t.write_script("workload.toml", &script_text());
    let st = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .env("AGENTOS_CRASH_AT", "P4")
        .env("AGENTOS_CRASH_CALL", "2")
        .output()
        .unwrap();
    assert_eq!(st.status.signal(), Some(9));
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let lock_path = t.exec_dir(&id).join("lock");

    // Held for the whole wait: recovery must refuse, and must not have touched anything.
    let held = std::fs::File::options().write(true).open(&lock_path).unwrap();
    held.lock().unwrap();
    let started = Instant::now();
    let out = t.output(&["recover", "--quiet", &id]);
    assert!(!out.status.success(), "recover must not run while the lock is held");
    assert!(String::from_utf8_lossy(&out.stderr).contains("in use by another agentos process"));
    assert!(started.elapsed() >= Duration::from_secs(2), "it should have waited before giving up");
    assert!(!events_of(&t, &id).iter().any(|e| e["type"] == "recovery.started"));

    // Released after 0.4 s: recovery waits, then does its work.
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        drop(held);
    });
    t.agentos(&["recover", "--quiet", &id]);
    releaser.join().unwrap();
    let mut v = Vec::new();
    check_invariants(&t, &id, &events_of(&t, &id), &mut v, "after recovery");
    assert!(v.is_empty(), "{v:?}");
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let idx = ((sorted.len() as f64 - 1.0) * p).round() as usize;
    sorted[idx]
}

#[test]
fn recovery_holds_at_every_crash_point() {
    let runs: usize = std::env::var("AGENTOS_RECOVERY_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let workers: usize = std::env::var("AGENTOS_RECOVERY_JOBS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let all = ["fs.read", "fs.list", "fs.write", "exec"];
    let mutating = ["fs.write", "exec"];
    // P4..P6 sit around the checkpoint, which only mutating calls take.
    let matrix: Vec<(&'static str, &[&'static str])> = vec![
        ("P1", &all), ("P2", &all), ("P3", &all), ("P4", &mutating), ("P5", &mutating), ("P6", &mutating), ("P7", &all), ("P8", &mutating),
    ];
    let mut jobs = Vec::new();
    for (point, tools) in &matrix {
        for tool in tools.iter() {
            for n in 0..runs {
                jobs.push(Job { point, tool, n });
            }
        }
    }
    let seed0 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(Job, Outcome)>> = Mutex::new(Vec::new());
    let started = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(i) else { break };
                let outcome = run_trial(job, seed0.wrapping_add(i as u64 * 7919));
                results.lock().unwrap().push((job.clone(), outcome));
            });
        }
    });
    let elapsed = started.elapsed();
    let results = results.into_inner().unwrap();

    // --- Report ---
    let mut rows = Vec::new();
    let mut all_times = Vec::new();
    let (mut total_viol, mut total_dup, mut total_runs, mut total_crashed, mut total_cls, mut total_cls_ok) = (0, 0, 0, 0, 0, 0);
    println!("\n| point | tool | runs | killed | recovery killed (P8) | violations | duplicated effects | cut-off calls classified correctly |");
    println!("|---|---|---|---|---|---|---|---|");
    for (point, tools) in &matrix {
        for tool in tools.iter() {
            let rs: Vec<&Outcome> = results.iter().filter(|(j, _)| j.point == *point && j.tool == *tool).map(|(_, o)| o).collect();
            let crashed = rs.iter().filter(|o| o.crashed).count();
            let rcrashed = rs.iter().filter(|o| o.recovery_crashed).count();
            let viol: usize = rs.iter().map(|o| o.violations.len()).sum();
            let dup: u64 = rs.iter().map(|o| o.duplicates).sum();
            let cls = rs.iter().filter(|o| o.classified.is_some()).count();
            let cls_ok = rs.iter().filter(|o| o.classified == Some(true)).count();
            all_times.extend(rs.iter().filter_map(|o| o.recover_ms));
            println!("| {point} | {tool} | {} | {crashed} | {} | {viol} | {dup} | {cls_ok}/{cls} |", rs.len(), if *point == "P8" { rcrashed.to_string() } else { "-".into() });
            rows.push(json!({"point": point, "tool": tool, "runs": rs.len(), "killed": crashed, "recovery_killed": rcrashed,
                "violations": viol, "duplicated_effects": dup, "classified": cls, "classified_correctly": cls_ok}));
            total_viol += viol;
            total_dup += dup;
            total_runs += rs.len();
            total_crashed += crashed;
            total_cls += cls;
            total_cls_ok += cls_ok;
        }
    }
    all_times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (p50, p99) = (percentile(&all_times, 0.50), percentile(&all_times, 0.99));
    println!(
        "\ntotal: {total_runs} runs, {total_crashed} killed, {total_viol} invariant violations, {total_dup} duplicated effects, \
         {total_cls_ok}/{total_cls} cut-off calls classified correctly; `agentos recover` wall time p50 {p50:.1} ms, p99 {p99:.1} ms; \
         harness took {:.0} s with {workers} workers",
        elapsed.as_secs_f64()
    );
    for (job, o) in results.iter().filter(|(_, o)| !o.violations.is_empty()).take(20) {
        println!("VIOLATION {} {} #{}: {:?}", job.point, job.tool, job.n, o.violations);
    }
    let report = json!({
        "runs_per_cell": runs, "preset": preset(), "rows": rows,
        "total": {"runs": total_runs, "killed": total_crashed, "violations": total_viol, "duplicated_effects": total_dup,
                  "classified": total_cls, "classified_correctly": total_cls_ok},
        "recover_ms": {"p50": p50, "p99": p99, "samples": all_times.len()},
        "harness_seconds": elapsed.as_secs_f64(),
    });
    let path = repo_root().join("target/recovery-report.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("report written to {}", path.display());

    assert_eq!(total_viol, 0, "invariant violations");
    assert_eq!(total_dup, 0, "duplicated side effects");
    assert_eq!(total_cls_ok, total_cls, "misclassified cut-off calls");
    // The harness must actually have killed things, or it proves nothing.
    assert!(total_crashed * 2 > total_runs, "only {total_crashed} of {total_runs} runs were killed");
    let _ = Duration::ZERO;
}
