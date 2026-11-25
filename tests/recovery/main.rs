//! Fault-injection harness (v1 "Evaluation", suite 1: the headline result).
//!
//! For each crash point P1..P8 and each tool, many times over:
//!
//! 1. run the scripted agent with `NORN_CRASH_AT=<point>` so the runtime
//!    kills itself (SIGKILL) at that point while handling a chosen call;
//! 2. look at what was left behind (before recovery touches it);
//! 3. run `norn recover`, timing it;
//! 4. check the five invariants from the v1 design;
//! 5. run `norn run --resume` to the end and check the final workspace is
//!    exactly what an uninterrupted run produces (no lost and no doubled effect).
//!
//! Runs per (point, tool): `NORN_RECOVERY_RUNS` (default 3, so `cargo test`
//! stays quick; `make eval` uses 200). Parallel workers: `NORN_RECOVERY_JOBS`
//! (default 8). The report goes to stdout and to `target/recovery-report.json`.
//!
//! The second big test, `external_calls_hold_under_kills_and_timeouts`, does
//! the same for the bonus tool `ext.call`, whose effect cannot be rolled
//! back. Its oracle is the fake service's own record of what it applied.
//! Report: `target/recovery-ext-report.json`.
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

use norn::blob::content_hash;
use common::fake_service::{FakeService, Fault};
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
    // I5: the log is a gap-free prefix. (`norn trace` already refused any
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
        .env("NORN_CRASH_AT", first_point)
        .env("NORN_CRASH_CALL", call.to_string())
        .env("NORN_CRASH_DELAY_US", delay_us.to_string())
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
        let st = t.cmd(&["recover", "--quiet", &id]).env("NORN_CRASH_AT", "P8").env("NORN_CRASH_HIT", (1 + rng.below(5)).to_string()).output().unwrap();
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
        .env("NORN_CRASH_AT", "P5")
        .env("NORN_CRASH_CALL", "2")
        .env("NORN_CRASH_DELAY_US", "1000")
        .output()
        .unwrap();
    assert_eq!(st.status.signal(), Some(9), "the run must be killed during the copy: {}", String::from_utf8_lossy(&st.stderr));
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();

    // Recovery uses the normal cp, straight away, as a person or script would.
    t.norn(&["recover", "--quiet", &id]);
    // Longer than the wrapper's sleep: a copy that survived would be done by now.
    std::thread::sleep(Duration::from_millis(900));
    let mut v = Vec::new();
    check_invariants(&t, &id, &events_of(&t, &id), &mut v, "after recovery and a pause");
    assert!(v.is_empty(), "{v:?}");
    t.norn(&["run", "--quiet", "--resume", &id]);
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
        .env("NORN_CRASH_AT", "P4")
        .env("NORN_CRASH_CALL", "2")
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
    assert!(String::from_utf8_lossy(&out.stderr).contains("in use by another norn process"));
    assert!(started.elapsed() >= Duration::from_secs(2), "it should have waited before giving up");
    assert!(!events_of(&t, &id).iter().any(|e| e["type"] == "recovery.started"));

    // Released after 0.4 s: recovery waits, then does its work.
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(400));
        drop(held);
    });
    t.norn(&["recover", "--quiet", &id]);
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
    let runs: usize = std::env::var("NORN_RECOVERY_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let workers: usize = std::env::var("NORN_RECOVERY_JOBS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
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
         {total_cls_ok}/{total_cls} cut-off calls classified correctly; `norn recover` wall time p50 {p50:.1} ms, p99 {p99:.1} ms; \
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

// ---------------------------------------------------------------------------
// The bonus: `ext.call`, an effect outside the workspace (D-080).
//
// Same idea as above, with a different oracle. The fake service runs inside
// this test program, so it survives every kill of `norn`, and its own
// list of applied requests is the ground truth: each order must be there
// exactly once at the end, whatever was killed or lost on the way.
// ---------------------------------------------------------------------------

const EXT_TIMEOUT_MS: u64 = 300;
/// How long the service sits on a dropped request; longer than the timeout.
const EXT_HOLD_MS: u64 = 500;
/// The two `ext.call` steps: (payload, call id on a first run).
const ORDERS: [(&str, u64); 2] = [("order-1", 2), ("order-2", 4)];

fn ext_script_text() -> String {
    let ext = |payload: &str| format!("[[step]]\ntool = \"ext.call\"\nservice = \"ledger\"\npayload = \"{payload}\"\n");
    let exec = format!("[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", '''\n{APPEND}\n''']\n");
    format!(
        "task = \"external-call workload\"\n[[step]]\ntool = \"fs.write\"\npath = \"src/a.txt\"\ncontent = \"A1\\n\"\n{}{exec}{}[[step]]\ntool = \"fs.write\"\npath = \"src/new.txt\"\ncontent = \"new\\n\"\n",
        ext(ORDERS[0].0),
        ext(ORDERS[1].0),
    )
}

fn ext_policy_text(url: &str) -> String {
    format!("{}\n[external]\ntimeout_ms = {EXT_TIMEOUT_MS}\nattempts = 3\n\n[[external.service]]\nname = \"ledger\"\nurl = \"{url}\"\n", policy_text())
}

/// Kill cells use the crash points that exist for a call without a
/// checkpoint. The T cells kill nothing: the *service* misbehaves.
const EXT_CELLS: [(&str, &str); 8] = [
    ("P1", "killed before the intent"),
    ("P2", "killed after the intent, before the request"),
    ("P3", "killed while the request is in flight"),
    ("P7", "killed after the completion record"),
    ("P8", "killed at P2 or P3, then recovery killed too"),
    ("T1", "request lost (no kill)"),
    ("T2", "answer lost (no kill)"),
    ("T3", "no answer to any attempt (no kill)"),
];

#[derive(Default, Clone)]
struct ExtOutcome {
    crashed: bool,
    recovery_crashed: bool,
    /// How often a run or a recovery stopped and left the call uncertain.
    uncertain_stops: u32,
    /// The cell's intended situation really occurred (see the asserts at the end).
    as_intended: bool,
    violations: Vec<String>,
    duplicates: u64,
    classified: Option<bool>,
    recover_ms: Option<f64>,
}

fn intent_keys(events: &[Value]) -> BTreeSet<String> {
    events.iter().filter(|e| e["type"] == "tool.intent").filter_map(|e| e["plan"]["key"].as_str().map(str::to_string)).collect()
}

fn says_uncertain(out: &std::process::Output) -> bool {
    String::from_utf8_lossy(&out.stderr).contains("UNCERTAIN")
}

fn run_ext_trial(cell: &'static str, n: usize, seed: u64) -> ExtOutcome {
    let mut rng = Rng::new(seed);
    let mut out = ExtOutcome::default();
    let svc = FakeService::start(0, Duration::from_millis(EXT_HOLD_MS)).unwrap();
    let t = TestEnv::new(&format!("ext-{cell}-{n}")).with_policy_text(&ext_policy_text(&svc.url()));
    let repo = t.root.join("repo");
    make_repo(&repo);
    let script = t.write_script("workload.toml", &ext_script_text());
    let (payload, call) = ORDERS[rng.below(2) as usize];
    let lost = |rng: &mut Rng| if rng.below(2) == 0 { Fault::DropBefore } else { Fault::DropAfter };

    // What the service will do to requests for the chosen order, and where
    // (if anywhere) the runtime is killed.
    let first_point = match cell {
        "P8" => Some(["P2", "P3"][rng.below(2) as usize]),
        "P1" | "P2" | "P3" | "P7" => Some(cell),
        _ => None,
    };
    let mut kill_delay_us = 1_500;
    match (cell, first_point) {
        (_, Some("P3")) => {
            // Either a slow request (the kill lands before or after the
            // service applies it) or a lost answer (the kill lands while the
            // runtime waits or retries).
            if rng.below(2) == 0 {
                svc.inject(payload, &[Fault::DelayMs(20 + rng.below(60))]);
                kill_delay_us = 100_000;
            } else {
                svc.inject(payload, &[Fault::DropAfter]);
                kill_delay_us = 250_000; // inside the first wait (the timeout is 300 ms)
            }
        }
        ("T1", _) => svc.inject(payload, &vec![Fault::DropBefore; 1 + rng.below(2) as usize]),
        ("T2", _) => {
            let mut f = vec![Fault::DropAfter];
            if rng.below(2) == 0 {
                f.insert(rng.below(2) as usize, Fault::DropBefore);
            }
            svc.inject(payload, &f);
        }
        ("T3", _) => {
            // Three attempts, all unanswered; half the time the first
            // recovery finds the service still down as well.
            let rounds = 3 * (1 + rng.below(2) as usize);
            let f: Vec<Fault> = (0..rounds).map(|_| lost(&mut rng)).collect();
            svc.inject(payload, &f);
        }
        _ => match rng.below(3) {
            0 => {}
            1 => svc.inject(payload, &[Fault::DelayMs(rng.below(40))]),
            _ => svc.inject(payload, &[Fault::DropAfter]),
        },
    }

    let mut cmd = t.cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()]);
    if let Some(point) = first_point {
        cmd.env("NORN_CRASH_AT", point).env("NORN_CRASH_CALL", call.to_string()).env("NORN_CRASH_DELAY_US", kill_delay_us.to_string());
    }
    let first = cmd.output().unwrap();
    out.crashed = first.status.signal() == Some(9);
    let finished_first_time = first.status.success();
    if !out.crashed && !finished_first_time {
        if says_uncertain(&first) {
            out.uncertain_stops += 1;
        } else {
            out.violations.push(format!("run failed without being killed: {}", String::from_utf8_lossy(&first.stderr)));
            return out;
        }
    }
    out.as_intended = match cell {
        "T1" | "T2" => finished_first_time,
        "T3" => out.uncertain_stops == 1,
        _ => out.crashed,
    };
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let dir = t.exec_dir(&id);
    let work = dir.join("work");
    let key_of = |cid: u64| format!("agentos-{id}-c{cid}");
    let applied_at_service = |key: &str| svc.applied().iter().any(|a| a.key == key);

    // --- Before recovery ---
    let before = events_of(&t, &id);
    // I1: no effect without a durable intent before it. Every request the
    // service has received carries a key that an intent in the log names.
    let keys = intent_keys(&before);
    for s in svc.seen() {
        if !keys.contains(&s.key) {
            out.violations.push(format!("I1: the service received key {} but the log has no intent for it", s.key));
        }
    }
    let (_, all_open) = pending(&before);
    let cut_off = all_open.first().copied();
    let requested = |events: &[Value]| events.iter().filter(|e| e["type"] == "tool.requested").count();

    // --- Recover and resume until the run is finished. A recovery that gets
    //     no answer stops and is simply tried again, as a person would. ---
    let mut done = finished_first_time;
    let mut first_recovery = true;
    for _round in 0..8 {
        if done {
            break;
        }
        if cell == "P8" && first_recovery {
            let st = t.cmd(&["recover", "--quiet", &id]).env("NORN_CRASH_AT", "P8").env("NORN_CRASH_HIT", (1 + rng.below(3)).to_string()).output().unwrap();
            out.recovery_crashed = st.status.signal() == Some(9);
        }
        let calls_before = requested(&events_of(&t, &id));
        let started = Instant::now();
        let st = t.output(&["recover", "--quiet", &id]);
        let ms = started.elapsed().as_secs_f64() * 1000.0;
        if !st.status.success() {
            if !says_uncertain(&st) {
                out.violations.push(format!("recover failed: {}", String::from_utf8_lossy(&st.stderr)));
                return out;
            }
            // Stopped without an answer: it must not have decided anything.
            out.uncertain_stops += 1;
            let now = events_of(&t, &id);
            let (_, open_now) = pending(&now);
            if open_now.is_empty() {
                out.violations.push("recovery said a call was still uncertain, but every call in the log has an ending".into());
            }
            if requested(&now) != calls_before {
                out.violations.push("I4: a new call was made while one was still uncertain".into());
            }
            continue;
        }
        if first_recovery {
            out.recover_ms = Some(ms);
            first_recovery = false;
            let after = events_of(&t, &id);
            check_invariants(&t, &id, &after, &mut out.violations, "after recovery");
            // A P3 timer can fire after the chosen call has ended, inside a
            // later file or command call; those have their own oracle above.
            if let Some(cid) = cut_off.filter(|c| call_of(&after, *c)["tool"] == "ext.call") {
                // The verdict against the service's own record.
                let got = after.iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == cid).map(|e| e["applied"] == true);
                let truth = applied_at_service(&key_of(cid));
                out.classified = Some(got == Some(truth));
                if got != Some(truth) {
                    out.violations.push(format!("call {cid}: the service says applied={truth}, recovery said {got:?}"));
                }
            }
        }
        let st = t.output(&["run", "--quiet", "--resume", &id]);
        if st.status.success() {
            done = true;
        } else if says_uncertain(&st) {
            out.uncertain_stops += 1; // a later call got no answer; go round again
        } else {
            out.violations.push(format!("resume failed: {}", String::from_utf8_lossy(&st.stderr)));
            return out;
        }
    }
    if !done {
        out.violations.push("the run did not finish within 8 rounds of recover and resume".into());
        return out;
    }

    // --- The end state. Wait first, so that a request that was still in
    //     flight when its sender was killed has landed. ---
    std::thread::sleep(Duration::from_millis(150));
    let end = events_of(&t, &id);
    check_invariants(&t, &id, &end, &mut out.violations, "at the end");
    if !end.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed") {
        out.violations.push("the run did not complete".into());
    }
    let applied = svc.applied();
    for (order, _) in ORDERS {
        let count = applied.iter().filter(|a| a.payload == order).count() as u64;
        if count == 0 {
            out.violations.push(format!("final: {order} was never applied at the service (lost effect)"));
        }
        out.duplicates += count.saturating_sub(1);
    }
    let keys = intent_keys(&end);
    for s in svc.seen() {
        if !keys.contains(&s.key) {
            out.violations.push(format!("I1 at the end: the service received key {} with no intent in the log", s.key));
        }
    }
    // Every ending the log gives an ext.call must agree with the service.
    for e in &end {
        let says_applied = match e["type"].as_str().unwrap() {
            "tool.completed" => true,
            "tool.reconciled" => e["applied"] == true,
            "tool.failed" | "tool.denied" => false,
            _ => continue,
        };
        let cid = e["call_id"].as_u64().unwrap();
        if call_of(&end, cid)["tool"] != "ext.call" {
            continue;
        }
        if says_applied != applied_at_service(&key_of(cid)) {
            out.violations.push(format!("call {cid}: the log says applied={says_applied}, the service disagrees"));
        }
    }
    let (counter, side) = (lines(&work.join("src/counter.txt")), lines(&work.join("src/side.txt")));
    if counter != 1 || side != 1 {
        out.violations.push(format!("final: counter={counter} side={side}, expected 1 and 1"));
    }
    out.duplicates += counter.saturating_sub(1) + side.saturating_sub(1);
    for (p, h) in [("src/a.txt", content_hash(b"A1\n")), ("src/new.txt", content_hash(b"new\n"))] {
        if file_hash(&work.join(p)).as_deref() != Some(h.as_str()) {
            out.violations.push(format!("final: {p} is not what an uninterrupted run writes"));
        }
    }
    if out.violations.is_empty() {
        let _ = std::fs::remove_dir_all(&t.root);
    }
    out
}

#[test]
fn external_calls_hold_under_kills_and_timeouts() {
    let runs: usize = std::env::var("NORN_RECOVERY_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let workers: usize = std::env::var("NORN_RECOVERY_JOBS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let jobs: Vec<(&'static str, usize)> = EXT_CELLS.iter().flat_map(|(cell, _)| (0..runs).map(move |n| (*cell, n))).collect();
    let seed0 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(&'static str, usize, ExtOutcome)>> = Mutex::new(Vec::new());
    let started = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some((cell, n)) = jobs.get(i) else { break };
                let outcome = run_ext_trial(cell, *n, seed0.wrapping_add(i as u64 * 7919));
                results.lock().unwrap().push((cell, *n, outcome));
            });
        }
    });
    let elapsed = started.elapsed();
    let results = results.into_inner().unwrap();

    let mut rows = Vec::new();
    let mut times = Vec::new();
    let mut not_as_intended = Vec::new();
    let (mut t_viol, mut t_dup, mut t_runs, mut t_cls, mut t_cls_ok, mut t_stops) = (0, 0, 0, 0, 0, 0);
    println!("\n| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | cut-off calls classified correctly |");
    println!("|---|---|---|---|---|---|---|---|---|---|");
    for (cell, what) in EXT_CELLS {
        let rs: Vec<&ExtOutcome> = results.iter().filter(|(c, _, _)| *c == cell).map(|(_, _, o)| o).collect();
        let intended = rs.iter().filter(|o| o.as_intended).count();
        let crashed = rs.iter().filter(|o| o.crashed).count();
        let rcrashed = rs.iter().filter(|o| o.recovery_crashed).count();
        let stops: u32 = rs.iter().map(|o| o.uncertain_stops).sum();
        let viol: usize = rs.iter().map(|o| o.violations.len()).sum();
        let dup: u64 = rs.iter().map(|o| o.duplicates).sum();
        let cls = rs.iter().filter(|o| o.classified.is_some()).count();
        let cls_ok = rs.iter().filter(|o| o.classified == Some(true)).count();
        times.extend(rs.iter().filter_map(|o| o.recover_ms));
        println!("| {cell} | {what} | {} | {intended} | {crashed} | {} | {stops} | {viol} | {dup} | {cls_ok}/{cls} |", rs.len(), if cell == "P8" { rcrashed.to_string() } else { "-".into() });
        rows.push(json!({"cell": cell, "what": what, "runs": rs.len(), "as_intended": intended, "killed": crashed, "recovery_killed": rcrashed,
            "stopped_uncertain": stops, "violations": viol, "duplicated_effects": dup, "classified": cls, "classified_correctly": cls_ok}));
        if intended * 2 <= rs.len() {
            not_as_intended.push(format!("{cell}: {intended} of {}", rs.len()));
        }
        t_viol += viol;
        t_dup += dup;
        t_runs += rs.len();
        t_cls += cls;
        t_cls_ok += cls_ok;
        t_stops += stops;
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (p50, p99) = (percentile(&times, 0.50), percentile(&times, 0.99));
    println!(
        "\nexternal total: {t_runs} runs, {t_viol} invariant violations, {t_dup} duplicated effects, {t_cls_ok}/{t_cls} cut-off calls classified correctly, \
         {t_stops} stops with the call left uncertain; successful `norn recover` wall time (includes waiting for the service) p50 {p50:.1} ms, p99 {p99:.1} ms; \
         harness took {:.0} s with {workers} workers",
        elapsed.as_secs_f64()
    );
    for (cell, n, o) in results.iter().filter(|(_, _, o)| !o.violations.is_empty()).take(20) {
        println!("VIOLATION {cell} #{n}: {:?}", o.violations);
    }
    let report = json!({
        "runs_per_cell": runs, "preset": preset(), "rows": rows,
        "total": {"runs": t_runs, "violations": t_viol, "duplicated_effects": t_dup, "classified": t_cls, "classified_correctly": t_cls_ok, "stopped_uncertain": t_stops},
        "recover_ms": {"p50": p50, "p99": p99, "samples": times.len()},
        "harness_seconds": elapsed.as_secs_f64(),
    });
    let path = repo_root().join("target/recovery-ext-report.json");
    std::fs::write(&path, serde_json::to_string_pretty(&report).unwrap()).unwrap();
    println!("report written to {}", path.display());

    assert_eq!(t_viol, 0, "invariant violations");
    assert_eq!(t_dup, 0, "duplicated side effects");
    assert_eq!(t_cls_ok, t_cls, "misclassified cut-off calls");
    // Each cell must really have produced its situation, or it proves nothing.
    assert!(not_as_intended.is_empty(), "cells that mostly did not go as intended: {not_as_intended:?}");
}

// ---------------------------------------------------------------------------
// fs.edit (D-120): an edit is a write whose content the runtime works out, so
// a kill must leave it either not done or wholly done, never in between, and
// a resumed run must apply it exactly once.
// ---------------------------------------------------------------------------

fn edit_file(mark: &str) -> String {
    // ~400 KB with one marked line in the middle, so the temp-file write
    // takes long enough for a kill to land in it.
    let filler = "a line of text for the fs.edit fault-injection workload\n".repeat(4000);
    format!("{filler}value = {mark}\n{filler}")
}

/// Kill the run at `point` while it handles the first edit (call 2), recover,
/// resume. Returns (killed, violations).
fn run_edit_trial(point: &'static str, n: usize, seed: u64) -> (bool, Vec<String>) {
    let mut rng = Rng::new(seed);
    let mut v = Vec::new();
    let t = TestEnv::new(&format!("rec-edit-{point}-{n}")).with_policy_text(&policy_text());
    let repo = t.root.join("repo");
    make_repo(&repo);
    std::fs::write(repo.join("src/a.txt"), edit_file("OLD")).unwrap();
    // The second edit only fits if the first was applied exactly once.
    let script = t.write_script(
        "edit.toml",
        "task = \"edit workload\"\n[[step]]\ntool = \"fs.read\"\npath = \"src/small.txt\"\n\
         [[step]]\ntool = \"fs.edit\"\npath = \"src/a.txt\"\nold_str = \"value = OLD\\n\"\nnew_str = \"value = MID\\nextra = 1\\n\"\n\
         [[step]]\ntool = \"fs.edit\"\npath = \"src/a.txt\"\nold_str = \"value = MID\"\nnew_str = \"value = NEW\"\n",
    );
    let (old, mid, new) = (content_hash(edit_file("OLD").as_bytes()), content_hash(edit_file("MID\nextra = 1").as_bytes()), content_hash(edit_file("NEW\nextra = 1").as_bytes()));
    let first_point = if point == "P8" { ["P2", "P3", "P4", "P5", "P6"][rng.below(5) as usize] } else { point };
    let status = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .env("NORN_CRASH_AT", first_point)
        .env("NORN_CRASH_CALL", "2")
        .env("NORN_CRASH_DELAY_US", "4000")
        .output()
        .unwrap();
    let crashed = status.status.signal() == Some(9);
    if !crashed && !status.status.success() {
        return (false, vec![format!("run failed without being killed: {}", String::from_utf8_lossy(&status.stderr))]);
    }
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let dir = t.exec_dir(&id);
    let work = dir.join("work");
    let file = work.join("src/a.txt");

    // What the kill left, read from the file itself.
    let before = events_of(&t, &id);
    let (_, all_open) = pending(&before);
    let at_crash = file_hash(&file);
    if at_crash.as_deref() != Some(old.as_str()) && at_crash.as_deref() != Some(mid.as_str()) && at_crash.as_deref() != Some(new.as_str()) {
        v.push("after the kill the file is neither the old nor the edited text".into());
    }
    let expect_applied = all_open.contains(&2).then(|| intent_of(&before, 2).is_some() && at_crash.as_deref() == Some(mid.as_str()));

    if point == "P8" {
        let _ = t.cmd(&["recover", "--quiet", &id]).env("NORN_CRASH_AT", "P8").env("NORN_CRASH_HIT", (1 + rng.below(5)).to_string()).output().unwrap();
    }
    let st = t.output(&["recover", "--quiet", &id]);
    if !st.status.success() {
        v.push(format!("recover failed: {}", String::from_utf8_lossy(&st.stderr)));
        return (crashed, v);
    }
    let after = events_of(&t, &id);
    let settled = |events: &[Value], when: &str, v: &mut Vec<String>| {
        let named = named_checkpoints(events);
        if tree_digest(&work) != tree_digest(&dir.join("checkpoints").join(named.last().unwrap())) {
            v.push(format!("{when}: workspace differs from the last checkpoint"));
        }
        let mut on_disk: Vec<String> = std::fs::read_dir(dir.join("checkpoints")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        on_disk.sort();
        let mut want = named.clone();
        want.sort();
        if on_disk != want || dir.join("work.restore-tmp").exists() {
            v.push(format!("{when}: leftovers (checkpoints on disk {on_disk:?}, named {want:?})"));
        }
        if std::fs::read_dir(work.join("src")).unwrap().any(|e| e.unwrap().file_name().to_string_lossy().starts_with(".norn-tmp-")) {
            v.push(format!("{when}: a temporary file was left in the workspace"));
        }
    };
    settled(&after, "after recovery", &mut v);
    if let Some(want) = expect_applied {
        let got = after.iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == 2).map(|e| e["applied"] == true);
        if got != Some(want) {
            v.push(format!("edit: the file says applied={want}, recovery said {got:?}"));
        }
        let now = file_hash(&file);
        if now.as_deref() != Some(if want { mid.as_str() } else { old.as_str() }) {
            v.push("after recovery the file does not match what recovery decided".into());
        }
    }

    let st = t.output(&["run", "--quiet", "--resume", &id]);
    if !st.status.success() {
        v.push(format!("resume failed: {}", String::from_utf8_lossy(&st.stderr)));
        return (crashed, v);
    }
    let end = events_of(&t, &id);
    settled(&end, "after resume", &mut v);
    if !end.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed") {
        v.push("resumed run did not complete".into());
    }
    if file_hash(&file).as_deref() != Some(new.as_str()) {
        v.push("final: the file is not what an uninterrupted run leaves (an edit was lost or made twice)".into());
    }
    if end.iter().any(|e| e["type"] == "tool.failed") {
        v.push("final: an edit failed (the second edit did not find its text)".into());
    }
    if v.is_empty() {
        let _ = std::fs::remove_dir_all(&t.root);
    }
    (crashed, v)
}

/// (crash point, run number, was killed, violations)
type EditTrial = (&'static str, usize, bool, Vec<String>);

#[test]
fn an_edit_is_recovered_like_a_write() {
    let runs: usize = std::env::var("NORN_RECOVERY_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let workers: usize = std::env::var("NORN_RECOVERY_JOBS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let points = ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8"];
    let jobs: Vec<(&'static str, usize)> = points.iter().flat_map(|p| (0..runs).map(move |n| (*p, n))).collect();
    let seed0 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<EditTrial>> = Mutex::new(Vec::new());
    let started = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some((point, n)) = jobs.get(i) else { break };
                let (crashed, v) = run_edit_trial(point, *n, seed0.wrapping_add(i as u64 * 7919));
                results.lock().unwrap().push((point, *n, crashed, v));
            });
        }
    });
    let results = results.into_inner().unwrap();
    println!("\n| point | tool | runs | killed | violations |");
    println!("|---|---|---|---|---|");
    let (mut total_viol, mut total_killed) = (0, 0);
    for p in points {
        let rs: Vec<_> = results.iter().filter(|r| r.0 == p).collect();
        let killed = rs.iter().filter(|r| r.2).count();
        let viol: usize = rs.iter().map(|r| r.3.len()).sum();
        println!("| {p} | fs.edit | {} | {killed} | {viol} |", rs.len());
        total_viol += viol;
        total_killed += killed;
        // P3 kills at a random moment in the effect and can come too late;
        // every other point is a fixed place and must always kill.
        if p != "P3" {
            assert_eq!(killed, rs.len(), "{p}: every run must be killed, or the cell proves nothing");
        }
    }
    println!("fs.edit total: {} runs, {total_killed} killed, {total_viol} violations; harness took {:.0} s with {workers} workers", results.len(), started.elapsed().as_secs_f64());
    for r in results.iter().filter(|r| !r.3.is_empty()).take(20) {
        println!("VIOLATION {} #{}: {:?}", r.0, r.1, r.3);
    }
    assert_eq!(total_viol, 0, "violations");
}

// ---------------------------------------------------------------------------
// The same kills with an MCP client as the agent (v2, M16, D-141).
//
// The seven workload steps are sent as `tools/call` messages to `norn mcp`,
// which kills itself at the chosen point. The client then does what Claude
// Code does (experiments/m15-mcp/NOTES.md, E3): it starts the server again
// with the same command line, and its next call makes the server continue
// the execution, recovering the cut-off call first. The checks are those of
// `run_trial`; the person ends the execution with `norn finish`.
// ---------------------------------------------------------------------------

use common::mcp_client::McpClient;

/// The workload of `script_text`, as MCP calls (wire names, D-142).
fn mcp_steps() -> Vec<(&'static str, Value)> {
    let exec = json!({"argv": ["python3", "-c", format!("{APPEND}\n")]});
    vec![
        ("fs_list", json!({"path": "src"})),
        ("fs_write", json!({"path": "src/a.txt", "content": big("A1")})),
        ("exec", exec.clone()),
        ("fs_read", json!({"path": "src/small.txt"})),
        ("fs_write", json!({"path": "src/a.txt", "content": big("A2")})),
        ("exec", exec),
        ("fs_write", json!({"path": "src/new.txt", "content": "new\n"})),
    ]
}

/// How many workload steps the log shows as done (completed, or reconciled
/// as applied). Calls are made in order, so the client goes on from there,
/// as `norn run --resume` does for a script.
fn steps_done(events: &[Value]) -> usize {
    events
        .iter()
        .filter(|e| e["type"] == "tool.completed" || (e["type"] == "tool.reconciled" && e["applied"] == true))
        .filter(|e| call_of(events, e["call_id"].as_u64().unwrap())["path"] != "probe")
        .count()
}

fn run_mcp_trial(job: &Job, seed: u64) -> Outcome {
    let mut rng = Rng::new(seed);
    let mut out = Outcome::default();
    let name = format!("rec-mcp-{}-{}-{}", job.point, job.tool.replace('.', ""), job.n);
    let t = TestEnv::new(&name).with_policy_text(&policy_text());
    let repo = t.root.join("repo");
    make_repo(&repo);
    std::fs::create_dir_all(repo.join("probe")).unwrap();
    let steps = TOOL_STEPS.iter().find(|(tool, _)| *tool == job.tool).unwrap().1;
    let call = steps[rng.below(steps.len() as u64) as usize] as u64 + 1;
    let first_point = if job.point == "P8" { ["P2", "P3", "P4", "P5", "P6"][rng.below(5) as usize] } else { job.point };
    let delay_us = match (first_point, job.tool) {
        ("P3", "exec") => 120_000,
        ("P3", "fs.write") => 4_000,
        ("P5", _) => 4_000,
        _ => 1_500,
    };
    let crash = [("NORN_CRASH_AT", first_point.to_string()), ("NORN_CRASH_CALL", call.to_string()), ("NORN_CRASH_DELAY_US", delay_us.to_string())];

    // --- The first session, killed at the crash point. ---
    let mut c = McpClient::start_with(&t, &repo, &crash);
    for (tool, args) in mcp_steps() {
        if c.try_call(tool, args).is_none() {
            break;
        }
    }
    let status = c.wait();
    out.crashed = status.signal() == Some(9);
    if !out.crashed {
        // Not killed (P3 can come too late): the session simply ended.
        out.violations.extend(status.code().filter(|c| *c != 0).map(|c| format!("server exited with {c} without being killed")));
    }
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let dir = t.exec_dir(&id);
    let work = dir.join("work");

    // --- What the kill left, before anything touches it (as in run_trial). ---
    let before = events_of(&t, &id);
    let (with_intent, all_open) = pending(&before);
    if with_intent.is_empty() {
        let last = dir.join("checkpoints").join(named_checkpoints(&before).last().unwrap());
        if tree_digest(&work) != tree_digest(&last) {
            out.violations.push("I1: workspace changed although no call has an open intent".into());
        }
    }
    let expect_applied: Option<(u64, bool)> = all_open.first().map(|cid| {
        let c = call_of(&before, *cid);
        let applied = c["tool"] == "fs.write"
            && intent_of(&before, *cid).is_some()
            && file_hash(&work.join(c["path"].as_str().unwrap())).as_deref() == c["content_hash"].as_str();
        (*cid, applied)
    });

    // --- The client starts the server again. For P8 the recovery inside
    // that start is killed first, and the client starts it once more. ---
    if job.point == "P8" {
        let env = [("NORN_CRASH_AT", "P8".to_string()), ("NORN_CRASH_HIT", (1 + rng.below(5)).to_string())];
        let mut c = McpClient::start_with(&t, &repo, &env);
        let _ = c.try_call("fs_list", json!({"path": "probe"}));
        out.recovery_crashed = c.wait().signal() == Some(9);
    }
    let started = Instant::now();
    let mut c = McpClient::start(&t, &repo);
    // A call that changes nothing, so the state right after recovery can be
    // checked before the client goes on.
    match c.try_call("fs_list", json!({"path": "probe"})) {
        Some((false, _)) => {}
        other => {
            out.violations.push(format!("the restarted server did not answer: {other:?}"));
            return out;
        }
    }
    out.recover_ms = Some(started.elapsed().as_secs_f64() * 1000.0);
    let after = events_of(&t, &id);
    check_invariants(&t, &id, &after, &mut out.violations, "after recovery through mcp");
    if let Some((cid, want)) = expect_applied {
        let got = after.iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == cid).map(|e| e["applied"] == true);
        out.classified = Some(got == Some(want));
        if got != Some(want) {
            out.violations.push(format!("call {cid}: files say applied={want}, recovery said {got:?}"));
        }
    }

    // --- The client goes on from the first step that is not done. ---
    for (tool, args) in mcp_steps().into_iter().skip(steps_done(&after)) {
        match c.try_call(tool, args) {
            Some((false, _)) => {}
            other => out.violations.push(format!("{tool} after the restart: {other:?}")),
        }
    }
    c.stop();
    let st = t.output(&["finish", &id]);
    if !st.status.success() {
        out.violations.push(format!("finish failed: {}", String::from_utf8_lossy(&st.stderr)));
        return out;
    }
    let end = events_of(&t, &id);
    check_invariants(&t, &id, &end, &mut out.violations, "after finish");
    let (counter, side) = (lines(&work.join("src/counter.txt")), lines(&work.join("src/side.txt")));
    out.duplicates = counter.saturating_sub(2) + side.saturating_sub(2);
    let want = [("src/a.txt", content_hash(big("A2").as_bytes())), ("src/new.txt", content_hash(b"new\n"))];
    for (p, h) in want {
        if file_hash(&work.join(p)).as_deref() != Some(h.as_str()) {
            out.violations.push(format!("final: {p} is not what an uninterrupted session writes"));
        }
    }
    if counter != 2 || side != 2 {
        out.violations.push(format!("final: counter={counter} side={side}, expected 2 and 2"));
    }
    if out.violations.is_empty() {
        let _ = std::fs::remove_dir_all(&t.root);
    }
    out
}

#[test]
fn recovery_holds_through_mcp() {
    let runs: usize = std::env::var("NORN_RECOVERY_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let workers: usize = std::env::var("NORN_RECOVERY_JOBS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let all = ["fs.read", "fs.list", "fs.write", "exec"];
    let mutating = ["fs.write", "exec"];
    let matrix: Vec<(&'static str, &[&'static str])> = vec![
        ("P1", &all), ("P2", &all), ("P3", &all), ("P4", &mutating), ("P5", &mutating), ("P6", &mutating), ("P7", &all), ("P8", &mutating),
    ];
    let jobs: Vec<Job> = matrix.iter().flat_map(|(point, tools)| tools.iter().flat_map(move |tool| (0..runs).map(move |n| Job { point, tool, n }))).collect();
    let seed0 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<(Job, Outcome)>> = Mutex::new(Vec::new());
    let started = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some(job) = jobs.get(i) else { break };
                let outcome = run_mcp_trial(job, seed0.wrapping_add(i as u64 * 7919));
                results.lock().unwrap().push((job.clone(), outcome));
            });
        }
    });
    let results = results.into_inner().unwrap();
    let mut rows = Vec::new();
    let (mut viol, mut dup, mut killed, mut cls, mut cls_ok) = (0, 0, 0, 0, 0);
    let mut times: Vec<f64> = Vec::new();
    println!("\n| point | tool | runs | killed | recovery killed (P8) | violations | duplicated effects | cut-off calls classified correctly |");
    println!("|---|---|---|---|---|---|---|---|");
    for (point, tools) in &matrix {
        for tool in tools.iter() {
            let rs: Vec<&Outcome> = results.iter().filter(|(j, _)| j.point == *point && j.tool == *tool).map(|(_, o)| o).collect();
            let rk = rs.iter().filter(|o| o.recovery_crashed).count();
            let k = rs.iter().filter(|o| o.crashed).count();
            let v: usize = rs.iter().map(|o| o.violations.len()).sum();
            let d: u64 = rs.iter().map(|o| o.duplicates).sum();
            let c = rs.iter().filter(|o| o.classified.is_some()).count();
            let c_ok = rs.iter().filter(|o| o.classified == Some(true)).count();
            times.extend(rs.iter().filter_map(|o| o.recover_ms));
            println!("| {point} | {tool} | {} | {k} | {} | {v} | {d} | {c_ok}/{c} |", rs.len(), if *point == "P8" { rk.to_string() } else { "-".into() });
            rows.push(json!({"point": point, "tool": tool, "runs": rs.len(), "killed": k, "recovery_killed": rk, "violations": v, "duplicated_effects": d, "classified": c, "classified_correctly": c_ok}));
            (viol, dup, killed, cls, cls_ok) = (viol + v, dup + d, killed + k, cls + c, cls_ok + c_ok);
        }
    }
    times.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let (p50, p99) = (percentile(&times, 0.50), percentile(&times, 0.99));
    println!(
        "\nmcp total: {} runs, {killed} killed, {viol} invariant violations, {dup} duplicated effects, {cls_ok}/{cls} cut-off calls classified correctly; \
         restart to first answer p50 {p50:.1} ms, p99 {p99:.1} ms; harness took {:.0} s with {workers} workers",
        results.len(),
        started.elapsed().as_secs_f64()
    );
    for (job, o) in results.iter().filter(|(_, o)| !o.violations.is_empty()).take(20) {
        println!("VIOLATION {} {} #{}: {:?}", job.point, job.tool, job.n, o.violations);
    }
    let report = json!({"runs_per_cell": runs, "preset": preset(), "rows": rows,
        "total": {"runs": results.len(), "killed": killed, "violations": viol, "duplicated_effects": dup, "classified": cls, "classified_correctly": cls_ok},
        "restart_to_first_answer_ms": {"p50": p50, "p99": p99, "samples": times.len()}});
    std::fs::write(repo_root().join("target/recovery-mcp-report.json"), serde_json::to_string_pretty(&report).unwrap()).unwrap();
    assert_eq!(viol, 0, "invariant violations");
    assert_eq!(dup, 0, "duplicated side effects");
    assert_eq!(cls_ok, cls, "misclassified cut-off calls");
    assert!(killed * 2 > results.len(), "only {killed} of {} runs were killed", results.len());
}

// ---------------------------------------------------------------------------
// `fs.delete` (v2, M17, D-147): a write whose new state is "no file". Killed
// while it deletes `src/a.txt` (call 2), recovered, resumed; the third step
// writes the file again, so the end state shows whether the delete was lost,
// or happened after the new write.
// ---------------------------------------------------------------------------

fn run_delete_trial(point: &'static str, n: usize, seed: u64) -> (bool, Vec<String>) {
    let mut rng = Rng::new(seed);
    let mut v = Vec::new();
    let t = TestEnv::new(&format!("rec-delete-{point}-{n}")).with_policy_text(&policy_text());
    let repo = t.root.join("repo");
    make_repo(&repo);
    std::fs::write(repo.join("src/a.txt"), big("D")).unwrap();
    let script = t.write_script(
        "delete.toml",
        "task = \"delete workload\"\n[[step]]\ntool = \"fs.read\"\npath = \"src/small.txt\"\n\
         [[step]]\ntool = \"fs.delete\"\npath = \"src/a.txt\"\n\
         [[step]]\ntool = \"fs.write\"\npath = \"src/a.txt\"\ncontent = \"after\\n\"\n",
    );
    let first_point = if point == "P8" { ["P2", "P3", "P4", "P5", "P6"][rng.below(5) as usize] } else { point };
    let status = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .env("NORN_CRASH_AT", first_point)
        .env("NORN_CRASH_CALL", "2")
        .env("NORN_CRASH_DELAY_US", "1500")
        .output()
        .unwrap();
    let crashed = status.status.signal() == Some(9);
    if !crashed && !status.status.success() {
        return (false, vec![format!("run failed without being killed: {}", String::from_utf8_lossy(&status.stderr))]);
    }
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let dir = t.exec_dir(&id);
    let work = dir.join("work");
    let file = work.join("src/a.txt");

    // The truth about the cut-off delete, read from the folder itself.
    let before = events_of(&t, &id);
    let (_, all_open) = pending(&before);
    let expect_applied = all_open.contains(&2).then(|| intent_of(&before, 2).is_some() && !file.exists());

    if point == "P8" {
        let _ = t.cmd(&["recover", "--quiet", &id]).env("NORN_CRASH_AT", "P8").env("NORN_CRASH_HIT", (1 + rng.below(5)).to_string()).output().unwrap();
    }
    let st = t.output(&["recover", "--quiet", &id]);
    if !st.status.success() {
        v.push(format!("recover failed: {}", String::from_utf8_lossy(&st.stderr)));
        return (crashed, v);
    }
    let settled = |events: &[Value], when: &str, v: &mut Vec<String>| {
        let named = named_checkpoints(events);
        if tree_digest(&work) != tree_digest(&dir.join("checkpoints").join(named.last().unwrap())) {
            v.push(format!("{when}: workspace differs from the last checkpoint"));
        }
        let mut on_disk: Vec<String> = std::fs::read_dir(dir.join("checkpoints")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        on_disk.sort();
        let mut want = named.clone();
        want.sort();
        if on_disk != want || dir.join("work.restore-tmp").exists() {
            v.push(format!("{when}: leftovers (checkpoints on disk {on_disk:?}, named {want:?})"));
        }
    };
    let after = events_of(&t, &id);
    settled(&after, "after recovery", &mut v);
    if let Some(want) = expect_applied {
        let got = after.iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == 2).map(|e| e["applied"] == true);
        if got != Some(want) {
            v.push(format!("delete: the folder says applied={want}, recovery said {got:?}"));
        }
        if file.exists() == want {
            v.push("after recovery the file does not match what recovery decided".into());
        }
    }

    let st = t.output(&["run", "--quiet", "--resume", &id]);
    if !st.status.success() {
        v.push(format!("resume failed: {}", String::from_utf8_lossy(&st.stderr)));
        return (crashed, v);
    }
    let end = events_of(&t, &id);
    settled(&end, "after resume", &mut v);
    if !end.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed") {
        v.push("resumed run did not complete".into());
    }
    if std::fs::read_to_string(&file).ok().as_deref() != Some("after\n") {
        v.push("final: src/a.txt is not what an uninterrupted run leaves".into());
    }
    if end.iter().any(|e| e["type"] == "tool.failed") {
        v.push("final: a call failed (a delete that was repeated finds no file)".into());
    }
    if v.is_empty() {
        let _ = std::fs::remove_dir_all(&t.root);
    }
    (crashed, v)
}

#[test]
fn a_delete_is_recovered_like_a_write() {
    let runs: usize = std::env::var("NORN_RECOVERY_RUNS").ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    let workers: usize = std::env::var("NORN_RECOVERY_JOBS").ok().and_then(|v| v.parse().ok()).unwrap_or(8);
    let points = ["P1", "P2", "P3", "P4", "P5", "P6", "P7", "P8"];
    let jobs: Vec<(&'static str, usize)> = points.iter().flat_map(|p| (0..runs).map(move |n| (*p, n))).collect();
    let seed0 = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos() as u64;
    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<EditTrial>> = Mutex::new(Vec::new());
    let started = Instant::now();
    std::thread::scope(|s| {
        for _ in 0..workers {
            s.spawn(|| loop {
                let i = next.fetch_add(1, Ordering::SeqCst);
                let Some((point, n)) = jobs.get(i) else { break };
                let (crashed, v) = run_delete_trial(point, *n, seed0.wrapping_add(i as u64 * 7919));
                results.lock().unwrap().push((point, *n, crashed, v));
            });
        }
    });
    let results = results.into_inner().unwrap();
    println!("\n| point | tool | runs | killed | violations |");
    println!("|---|---|---|---|---|");
    let (mut total_viol, mut total_killed) = (0, 0);
    for p in points {
        let rs: Vec<_> = results.iter().filter(|r| r.0 == p).collect();
        let killed = rs.iter().filter(|r| r.2).count();
        let viol: usize = rs.iter().map(|r| r.3.len()).sum();
        println!("| {p} | fs.delete | {} | {killed} | {viol} |", rs.len());
        total_viol += viol;
        total_killed += killed;
        if p != "P3" {
            assert_eq!(killed, rs.len(), "{p}: every run must be killed, or the cell proves nothing");
        }
    }
    println!("fs.delete total: {} runs, {total_killed} killed, {total_viol} violations; harness took {:.0} s with {workers} workers", results.len(), started.elapsed().as_secs_f64());
    for r in results.iter().filter(|r| !r.3.is_empty()).take(20) {
        println!("VIOLATION {} #{}: {:?}", r.0, r.1, r.3);
    }
    assert_eq!(total_viol, 0, "violations");
}
