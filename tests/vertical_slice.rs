//! M1 exit criterion: the scripted agent fixes the demo repo's failing test,
//! and `agentos trace` prints every call.
//!
//! These tests run the real `agentos` binary, the way a user would. They need
//! `pytest` on PATH (run them inside `nix develop`).

mod common;

use common::*;

#[test]
fn scripted_agent_fixes_the_demo_repo() {
    let t = TestEnv::new("slice-fix");
    let id = t.run_script("examples/scripts/fix-demo.toml");

    let events = t.events(&id);
    let finished = events.iter().rev().find(|e| e["type"] == "execution.finished").expect("finished event");
    assert_eq!(finished["status"], "completed");

    // Every call in the script reached a terminal record.
    let requested: Vec<u64> = events
        .iter()
        .filter(|e| e["type"] == "tool.requested")
        .map(|e| e["call_id"].as_u64().unwrap())
        .collect();
    assert_eq!(requested, vec![1, 2, 3, 4, 5, 6, 7]);

    // The first pytest fails, the last one passes: the fix worked.
    let exits = exec_exit_codes(&t, &events);
    assert_eq!(exits.first(), Some(&Some(1)), "pytest fails before the fix");
    assert_eq!(exits.last(), Some(&Some(0)), "pytest passes after the fix");

    // The read of .env was refused by the deny rule, before any intent.
    let denied = events.iter().find(|e| e["type"] == "tool.denied").expect("a denial");
    assert_eq!(denied["call_id"], 5);
    assert!(denied["rule"].as_str().unwrap().starts_with("filesystem.deny"));
    assert!(!events.iter().any(|e| e["type"] == "tool.intent" && e["call_id"] == 5));

    // The patch holds the one-line fix.
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    assert!(patch.contains("-    return sum(values) / (len(values) - 1)"));
    assert!(patch.contains("+    return sum(values) / len(values)"));
}

#[test]
fn trace_prints_every_call() {
    let t = TestEnv::new("slice-trace");
    let id = t.run_script("examples/scripts/fix-demo.toml");
    let out = t.agentos(&["trace", &id]);
    for n in 1..=7 {
        assert!(out.contains(&format!("#{n:<3} ")), "trace is missing call #{n}:\n{out}");
    }
    assert!(out.contains("DENY"));
    assert!(out.contains("finished: completed"));
    // --events prints one line per event; --json one object per line.
    let n_events = t.events(&id).len();
    assert_eq!(t.agentos(&["trace", &id, "--events"]).lines().count(), n_events);
}

#[test]
fn original_repo_is_never_modified_and_denied_files_are_not_copied() {
    let t = TestEnv::new("slice-orig");
    let before = tree_digest(&repo_root().join("examples/demo-repo"));
    let id = t.run_script("examples/scripts/fix-demo.toml");
    let after = tree_digest(&repo_root().join("examples/demo-repo"));
    assert_eq!(before, after, "design rule 10: the original repo is never modified");
    let work = t.exec_dir(&id).join("work");
    assert!(!work.join(".env").exists(), "D-008: denied files are not copied");
    assert!(work.join("src/stats.py").exists());
}

#[test]
fn denied_command_and_unknown_path_are_reported() {
    let t = TestEnv::new("slice-deny");
    let script = t.write_script(
        "deny.toml",
        r#"
task = "try things the policy does not allow"
[[step]]
tool = "exec"
argv = ["sh", "-c", "cat .env"]
[[step]]
tool = "fs.write"
path = "pytest.ini"
content = "[pytest]\n"
[[step]]
tool = "fs.read"
path = "../../etc/passwd"
[[step]]
tool = "fs.read"
path = "does-not-exist.txt"
"#,
    );
    let id = t.run_script(script.to_str().unwrap());
    let events = t.events(&id);
    let rule_of = |call: u64| {
        events
            .iter()
            .find(|e| e["type"] == "tool.denied" && e["call_id"] == call)
            .map(|e| e["rule"].as_str().unwrap().to_string())
    };
    assert_eq!(rule_of(1).as_deref(), Some("no matching rule"), "sh is not in exec.allow");
    assert_eq!(rule_of(2).as_deref(), Some("no matching rule"), "pytest.ini is not in the write globs");
    assert!(rule_of(3).unwrap().starts_with("invalid request"), "../ is refused");
    // A missing file is not a policy question: it is allowed, then fails.
    assert!(rule_of(4).is_none());
    assert!(events.iter().any(|e| e["type"] == "tool.failed" && e["call_id"] == 4));
}
