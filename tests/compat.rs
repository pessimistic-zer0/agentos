//! The compatibility promise (`docs/compatibility.md`, D-126): files written
//! by an earlier release keep working.
//!
//! The fixtures in `tests/fixtures/compat/` are frozen: they were written by
//! the binary built from the tag `v0.1` and must never be regenerated with
//! newer code. `v0.1-executions.tar` holds two execution folders, one
//! finished and one killed right after the effect of call 6 (crash point
//! P4), packed as a tar file so that the trees are exact. The two policy
//! files are the presets as v0.1 shipped them.
//!
//! Needs a delegated cgroup for the resume: systemd-run --user --scope -p Delegate=yes cargo test --test compat

mod common;

use norn::policy::Policy;
use common::*;

fn fixture(name: &str) -> std::path::PathBuf {
    repo_root().join("tests/fixtures/compat").join(name)
}

/// A state folder holding the two executions that v0.1 wrote.
fn unpack(t: &TestEnv) {
    std::fs::create_dir_all(&t.state).unwrap();
    let ok = std::process::Command::new("tar").arg("-xf").arg(fixture("v0.1-executions.tar")).arg("-C").arg(&t.state).status().unwrap();
    assert!(ok.success());
}

#[test]
fn a_policy_file_from_v0_1_is_still_valid_and_means_the_same() {
    for name in ["policy-v0.1-nix.toml", "policy-v0.1-fhs.toml"] {
        let text = std::fs::read_to_string(fixture(name)).unwrap();
        let p = Policy::parse(&text).unwrap_or_else(|e| panic!("{name}: {e}"));
        // What it said then, it says now.
        assert!(p.check_path(norn::policy::Access::Write, "src/a.py").allowed);
        assert!(!p.check_path(norn::policy::Access::Write, "README.md").allowed);
        assert!(!p.check_path(norn::policy::Access::Read, ".env").allowed);
        assert!(p.check_exec(&["pytest".to_string(), "-q".to_string()]).allowed);
        assert!(!p.check_exec(&["sh".to_string()]).allowed);
        // Everything added since has a default that changes nothing for it.
        assert!(p.filesystem.skip.is_empty() && p.filesystem.scratch.is_empty() && p.filesystem.ro_dirs.is_empty() && p.filesystem.deny_except.is_empty());
        assert!(p.exec.helpers.is_empty() && p.sandbox.env.is_empty() && p.external.services.is_empty());
        assert_eq!(p.limits.workspace_mb, 2048);
    }
}

#[test]
fn a_finished_execution_from_v0_1_can_be_listed_traced_and_applied() {
    let t = TestEnv::new("compat-finished");
    unpack(&t);
    let status = t.norn(&["status"]);
    assert!(status.lines().any(|l| l.starts_with("finished") && l.contains("completed") && l.contains("7 calls")), "{status}");
    assert!(status.lines().any(|l| l.starts_with("killed-p4") && l.contains("interrupted")), "{status}");
    let trace = t.norn(&["trace", "finished"]);
    for line in ["fs.write src/stats.py (548 bytes)", "exit 0", "DENY", "finished: completed"] {
        assert!(trace.contains(line), "`{line}` is missing from:\n{trace}");
    }
    // Apply it to a copy of what the run started from (its first checkpoint).
    let target = t.root.join("target-repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(t.exec_dir("finished").join("checkpoints/cp-0000")).arg(&target).status().unwrap().success());
    let shown = t.norn(&["apply", "finished", "--to", target.to_str().unwrap()]);
    assert!(shown.contains("M src/stats.py"), "{shown}");
    assert_eq!(std::fs::read(target.join("src/stats.py")).unwrap(), std::fs::read(t.exec_dir("finished").join("work/src/stats.py")).unwrap());
    // And it can be deleted.
    t.norn(&["rm", "finished"]);
    assert!(!t.exec_dir("finished").exists() && t.exec_dir("killed-p4").exists());
}

#[test]
fn an_execution_that_v0_1_left_interrupted_is_recovered_and_resumed() {
    let t = TestEnv::new("compat-killed");
    unpack(&t);
    let out = t.norn(&["recover", "killed-p4"]);
    assert!(out.contains("#6") && out.contains("fs.write APPLIED"), "{out}");
    let events = t.events("killed-p4");
    let rec = events.iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == 6).expect("call 6 reconciled");
    assert_eq!(rec["applied"], true);
    let cp = t.exec_dir("killed-p4").join("checkpoints").join(rec["checkpoint"].as_str().unwrap());
    assert_eq!(tree_digest(&cp), tree_digest(&t.exec_dir("killed-p4").join("work")), "the checkpoint recovery took equals the workspace");
    // A second recovery finds nothing to do.
    assert!(t.norn(&["recover", "killed-p4"]).contains("nothing to recover"));
    // The resume runs the last step in the sandbox with the policy saved in
    // the execution, which is v0.1's `nix` preset (it names /nix/store). On
    // another system that policy's folders do not exist, so the resume is
    // only tried where it can work.
    if preset() == "nix" {
        t.norn(&["run", "--quiet", "--resume", "killed-p4"]);
        let events = t.events("killed-p4");
        assert!(events.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"), "{events:#?}");
        // (Read by folder name: the fixture's folders have fixed names, not the ids in their logs.)
        let last = t.output_of("killed-p4", &events, 7).expect("call 7 completed");
        assert_eq!(last["exit_code"], 0, "pytest passes after the resumed run");
    }
}

#[test]
fn a_log_or_policy_of_a_later_format_is_refused_with_its_version_named() {
    // The promise covers format version 1. A file that says it is something
    // else must be refused clearly, not half-read.
    let t = TestEnv::new("compat-future");
    unpack(&t);
    let db = rusqlite::Connection::open(t.exec_dir("finished").join("log.db")).unwrap();
    db.execute("UPDATE meta SET value = '2' WHERE key = 'schema_version'", []).unwrap();
    drop(db);
    let out = t.output(&["trace", "finished"]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && err.contains("schema version 2 is not supported"), "{err}");

    let text = std::fs::read_to_string(fixture("policy-v0.1-nix.toml")).unwrap().replace("policy_version = 1", "policy_version = 2");
    let err = Policy::parse(&text).unwrap_err().to_string();
    assert!(err.contains("policy_version 2 is not supported"), "{err}");
    // A key this version does not know is an error too, never ignored: a
    // misspelt `deny` must not silently protect nothing.
    let text = std::fs::read_to_string(fixture("policy-v0.1-nix.toml")).unwrap().replace("[limits]", "[limits]\nfuture_limit = 1");
    assert!(Policy::parse(&text).is_err());
}
