//! Tests added after v0.1 and M7, for what a hand check found (2026-11-13):
//!
//! - B-009: files left by a sandboxed command that the runtime cannot scan
//!   or copy must fail that call, not stop the run (D-085).
//! - B-010: a workspace with locked folders must still be recoverable (D-086).
//! - D-084: `fs.write` makes missing folders, and a crash in the middle of
//!   that is still recovered to a known state.
//!
//! These are not escape tests (those are in `tests/security/`). They ask a
//! different question: after hostile or merely odd code ran, does the run go
//! on, and can it be recovered?
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test hardening

mod common;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use common::*;
use serde_json::Value;

/// The preset policy with `python3` allowed, so a step can run chosen code.
/// (Any allowed test command can run such code; this only makes it direct.)
fn policy_with_python() -> String {
    let base = std::fs::read_to_string(preset_policy()).unwrap();
    let mut out = String::new();
    let mut in_allow = false;
    for line in base.lines() {
        if line.starts_with("allow = [") {
            in_allow = true;
            out.push_str("allow = [[\"pytest\"], [\"python3\"]]\n");
        } else if in_allow {
            in_allow = !line.starts_with(']');
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

fn exec_step(code: &str) -> String {
    assert!(!code.contains("'''"));
    format!("[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", '''\nimport os, time\n{code}\n''']\n")
}

fn run_args(t: &TestEnv, script: &Path, repo: &Path) -> Vec<String> {
    ["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--agent", "scripted", "--script", script.to_str().unwrap()]
        .iter()
        .map(|s| s.to_string())
        .collect()
}

fn demo_repo() -> PathBuf {
    repo_root().join("examples/demo-repo")
}

/// The one execution in this test's state folder.
fn only_execution(t: &TestEnv) -> String {
    let mut ids: Vec<String> =
        std::fs::read_dir(t.state.join("executions")).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    assert_eq!(ids.len(), 1, "{ids:?}");
    ids.pop().unwrap()
}

fn event<'a>(events: &'a [Value], kind: &str, call: u64) -> Option<&'a Value> {
    events.iter().find(|e| e["type"] == kind && e["call_id"] == call)
}

/// The newest checkpoint folder on disk.
fn last_checkpoint(t: &TestEnv, id: &str) -> PathBuf {
    let dir = t.exec_dir(id).join("checkpoints");
    let mut names: Vec<_> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
    names.sort();
    assert!(names.iter().all(|n| n.starts_with("cp-")), "half-made checkpoint left behind: {names:?}");
    dir.join(names.last().unwrap())
}

fn assert_work_equals_last_checkpoint(t: &TestEnv, id: &str) {
    let work = t.exec_dir(id).join("work");
    assert_eq!(tree_digest(&work), tree_digest(&last_checkpoint(t, id)), "workspace differs from the last checkpoint");
    assert!(!t.exec_dir(id).join("work.restore-tmp").exists(), "work.restore-tmp was left behind");
}

// ---------------------------------------------------------------------------
// B-009: awkward files fail the call; the run goes on.
// ---------------------------------------------------------------------------

/// Run `code` as call 1 and a plain `fs.list` as call 2. Call 1 must end as
/// FAILED with everything undone, call 2 must work, the run must finish.
fn command_fails_and_run_continues(name: &str, code: &str) {
    let t = TestEnv::new(name).with_policy_text(&policy_with_python());
    let script = t.write_script("s.toml", &format!("task = \"hostile leftovers\"\n{}[[step]]\ntool = \"fs.list\"\npath = \"src\"\n", exec_step(code)));
    let id = t.run_script(script.to_str().unwrap()); // panics if agentos exits with an error
    let events = t.events(&id);
    let failed = event(&events, "tool.failed", 1).unwrap_or_else(|| panic!("call 1 did not fail: {events:#?}"));
    let error = failed["error"].as_str().unwrap();
    assert!(error.contains("was undone"), "{error}");
    assert!(!error.contains(t.root.to_str().unwrap()), "host path shown to the agent: {error}");
    assert!(event(&events, "tool.completed", 1).is_none());
    let listed = t.output_of(&id, &events, 2).expect("call 2 completed");
    let names: Vec<&str> = listed["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, vec!["stats.py"], "the command's files are gone");
    assert!(events.iter().any(|e| e["type"] == "execution.finished"));
    assert_work_equals_last_checkpoint(&t, &id);
    assert_eq!(last_checkpoint(&t, &id).file_name().unwrap(), "cp-0000", "a failed call takes no checkpoint");
}

#[test]
fn a_file_name_that_is_not_utf8_fails_the_call_not_the_run() {
    command_fails_and_run_continues("hard-nonutf8", r#"open(b"/work/src/bad\xff.py", "w").write("x")"#);
}

#[test]
fn an_unreadable_file_fails_the_call_not_the_run() {
    command_fails_and_run_continues("hard-file000", r#"open("/work/src/locked.py", "w").write("x"); os.chmod("/work/src/locked.py", 0)"#);
}

#[test]
fn a_locked_folder_inside_the_write_globs_fails_the_call_not_the_run() {
    command_fails_and_run_continues("hard-dir000", r#"os.mkdir("/work/src/d"); open("/work/src/d/f", "w").write("x"); os.chmod("/work/src/d", 0)"#);
}

#[test]
fn a_locked_folder_outside_the_write_globs_fails_the_call_not_the_run() {
    command_fails_and_run_continues("hard-dir000-out", r#"os.mkdir("/work/docs"); open("/work/docs/f", "w").write("x"); os.chmod("/work/docs", 0)"#);
}

// ---------------------------------------------------------------------------
// B-010: recovery works whatever the command did to folder permissions.
// ---------------------------------------------------------------------------

#[test]
fn recovery_works_after_a_kill_while_the_workspace_holds_a_locked_folder() {
    let t = TestEnv::new("hard-recover-locked").with_policy_text(&policy_with_python());
    let code = r#"os.mkdir("/work/src/d"); open("/work/src/d/f", "w").write("x"); os.mkdir("/work/src/d/e"); os.chmod("/work/src/d/e", 0); os.chmod("/work/src/d", 0)
open("/work/src/ready", "w").write("x")
time.sleep(30)"#;
    let script = t.write_script("s.toml", &format!("task = \"locked\"\n{}", exec_step(code)));
    let args = run_args(&t, &script, &demo_repo());
    let mut child = t.cmd(&args.iter().map(String::as_str).collect::<Vec<_>>()).spawn().unwrap();
    // Kill the runtime (as `kill -9` would) once the command has locked its folder.
    let deadline = Instant::now() + Duration::from_secs(20);
    let ready = loop {
        let found = std::fs::read_dir(t.state.join("executions"))
            .ok()
            .and_then(|mut d| d.next())
            .map(|e| e.unwrap().path().join("work/src/ready").exists())
            .unwrap_or(false);
        if found || Instant::now() > deadline {
            break found;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    child.kill().unwrap();
    child.wait().unwrap();
    assert!(ready, "the command never got as far as locking its folder");
    let id = only_execution(&t);

    let out = t.agentos(&["recover", &id]);
    assert!(out.contains("NOT APPLIED"), "{out}");
    assert_work_equals_last_checkpoint(&t, &id);
    assert!(!t.exec_dir(&id).join("work/src/d").exists());
    // A second recovery finds nothing to do.
    let again = t.agentos(&["recover", &id]);
    assert!(again.contains("nothing to recover"), "{again}");
}

#[test]
fn recovery_works_when_a_checkpoint_holds_a_read_only_folder() {
    // A folder with mode 555 can be copied, so the call completes and the
    // folder is in the checkpoint. Deleting such a tree needs the write bit
    // back. Call 2 is killed after its effect, so recovery must roll back,
    // which deletes the old workspace; the checkpoint `cp-0002` that call 3
    // half-made (P6: copied but not named in the log) must be deleted too.
    let t = TestEnv::new("hard-recover-readonly").with_policy_text(&policy_with_python());
    let make = r#"os.mkdir("/work/src/ro"); open("/work/src/ro/f.py", "w").write("x = 1\n"); os.chmod("/work/src/ro", 0o555)"#;
    let script = t.write_script("s.toml", &format!("task = \"read-only\"\n{}{}", exec_step(make), exec_step("print(1)")));
    let args = run_args(&t, &script, &demo_repo());
    let st = t.cmd(&args.iter().map(String::as_str).collect::<Vec<_>>()).env("AGENTOS_CRASH_AT", "P6").env("AGENTOS_CRASH_CALL", "2").output().unwrap();
    assert!(!st.status.success(), "the runtime was meant to be killed at P6");
    let id = only_execution(&t);
    assert!(t.exec_dir(&id).join("checkpoints/cp-0002").exists(), "the orphan checkpoint this test is about");

    let out = t.agentos(&["recover", &id]);
    assert!(out.contains("removed checkpoint cp-0002"), "{out}");
    assert_work_equals_last_checkpoint(&t, &id);
    assert_eq!(last_checkpoint(&t, &id).file_name().unwrap(), "cp-0001");
    assert!(t.exec_dir(&id).join("work/src/ro/f.py").exists(), "call 1 completed and stays");
}

// ---------------------------------------------------------------------------
// D-084: fs.write makes missing folders.
// ---------------------------------------------------------------------------

fn write_step(path: &str) -> String {
    format!("[[step]]\ntool = \"fs.write\"\npath = \"{path}\"\ncontent = \"x = 1\\n\"\n")
}

#[test]
fn fs_write_makes_missing_folders_inside_the_write_globs() {
    let t = TestEnv::new("hard-mkdirs");
    let steps = [
        write_step("src/newpkg/sub/mod.py"),      // 1: two folders made
        write_step("tests/unit/test_new.py"),     // 2
        write_step("src/newpkg/sub/second.py"),   // 3: the folders exist now
        write_step("./src//other/./a.py"),        // 4: untidy but harmless spelling
        "[[step]]\ntool = \"fs.read\"\npath = \"src/newpkg/sub/mod.py\"\n".to_string(), // 5
        "[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\"]\n".to_string(),          // 6
    ];
    let script = t.write_script("s.toml", &format!("task = \"new package\"\n{}", steps.concat()));
    let id = t.run_script(script.to_str().unwrap());
    let events = t.events(&id);
    for call in 1..=6 {
        assert!(event(&events, "tool.completed", call).is_some(), "call {call} did not complete: {events:#?}");
    }
    let plan = &event(&events, "tool.intent", 1).unwrap()["plan"];
    assert_eq!(plan["created_dirs"], serde_json::json!(["src/newpkg", "src/newpkg/sub"]), "the intent names the folders before they are made");
    assert_eq!(plan["path"], "src/newpkg/sub/mod.py");
    assert!(plan["pre_hash"].is_null());
    assert!(event(&events, "tool.intent", 3).unwrap()["plan"].get("created_dirs").is_none());
    assert_eq!(event(&events, "tool.intent", 4).unwrap()["plan"]["path"], "src/other/a.py");
    assert!(!events.iter().any(|e| e["type"] == "policy.violation"), "the new folders survive a later exec");
    let work = t.exec_dir(&id).join("work");
    assert_eq!(std::fs::read_to_string(work.join("src/newpkg/sub/mod.py")).unwrap(), "x = 1\n");
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    for f in ["b/src/newpkg/sub/mod.py", "b/tests/unit/test_new.py", "b/src/other/a.py"] {
        assert!(patch.contains(f), "{f} is missing from the patch");
    }
    assert_work_equals_last_checkpoint(&t, &id);
}

#[test]
fn fs_write_makes_no_folder_it_should_not() {
    let t = TestEnv::new("hard-mkdirs-refused");
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(demo_repo()).arg(&repo).status().unwrap().success());
    let outside = t.root.join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink("nowhere", repo.join("src/dangling")).unwrap();
    std::os::unix::fs::symlink(&outside, repo.join("src/out")).unwrap();
    let steps = [
        write_step("docs/guide/a.md"),        // 1: outside the write globs
        write_step("src/.env/x.py"),          // 2: would make a folder with a denied name
        write_step("secrets/new/k.py"),       // 3: denied tree
        write_step("src/out/new/x.py"),       // 4: through a symlink that leaves the workspace
        write_step("src/gone/../x.py"),       // 5: `..` after a folder that does not exist
        write_step("src/stats.py/x.py"),      // 6: through a file
        write_step("src/dangling/x.py"),      // 7: through a symlink that points at nothing
        write_step("../escape/x.py"),         // 8
    ];
    let script = t.write_script("s.toml", &format!("task = \"refused\"\n{}", steps.concat()));
    let id = t.run_script_on(script.to_str().unwrap(), &repo);
    let events = t.events(&id);
    let denied = |call| event(&events, "tool.denied", call).map(|e| e["rule"].as_str().unwrap().to_string());
    let failed = |call| event(&events, "tool.failed", call).map(|e| e["error"].as_str().unwrap().to_string());
    assert_eq!(denied(1).as_deref(), Some("no matching rule"));
    assert!(denied(2).is_some_and(|r| r.starts_with("filesystem.deny") && r.contains(".env")), "{:?}", denied(2));
    assert!(denied(3).is_some_and(|r| r.contains("secrets/**")), "{:?}", denied(3));
    assert!(denied(4).is_some_and(|r| r.contains("leaves the workspace")), "{:?}", denied(4));
    assert!(failed(5).is_some_and(|e| e.contains("does not exist")), "{:?}", failed(5));
    assert!(failed(6).is_some(), "call 6 should fail");
    assert!(failed(7).is_some_and(|e| e.contains("exists and is not a folder")), "{:?}", failed(7));
    assert!(denied(8).is_some(), "call 8 should be denied");
    assert!(!events.iter().any(|e| e["type"] == "tool.completed"), "nothing may complete: {events:#?}");
    // Nothing was made anywhere: not in the workspace, not outside it.
    assert_work_equals_last_checkpoint(&t, &id);
    assert_eq!(last_checkpoint(&t, &id).file_name().unwrap(), "cp-0000");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "a folder was made outside the workspace");
    assert!(std::fs::symlink_metadata(t.exec_dir(&id).join("work/src/dangling")).unwrap().file_type().is_symlink());
}

#[test]
fn a_folder_making_write_is_recovered_at_every_crash_point() {
    // P2: intent logged, nothing done. P3: killed at a random moment while
    // making folders and writing. P4: written, no checkpoint. P6: checkpoint
    // copied, not named. After recovery the workspace must equal the last
    // checkpoint, and the verdict must match what is on disk.
    let mut runs: Vec<(&str, u64)> = vec![("P2", 0), ("P4", 0), ("P6", 0)];
    runs.extend((0..12).map(|i| ("P3", 40 + i * 60)));
    let mut applied = 0;
    for (i, (point, delay_us)) in runs.iter().enumerate() {
        let t = TestEnv::new(&format!("hard-mkdirs-crash-{i}"));
        let script = t.write_script("s.toml", &format!("task = \"crash\"\n{}", write_step("src/a/b/c/mod.py")));
        let args = run_args(&t, &script, &demo_repo());
        let mut cmd = t.cmd(&args.iter().map(String::as_str).collect::<Vec<_>>());
        cmd.env("AGENTOS_CRASH_AT", point).env("AGENTOS_CRASH_CALL", "1").env("AGENTOS_CRASH_DELAY_US", delay_us.max(&1).to_string());
        let st = cmd.output().unwrap();
        let id = only_execution(&t);
        if st.status.success() {
            continue; // P3's timer lost the race: the run finished first
        }
        t.agentos(&["recover", &id]);
        let events = t.events(&id);
        let verdict = events.iter().find(|e| e["type"] == "tool.reconciled").unwrap_or_else(|| panic!("{point}: no verdict: {events:#?}"));
        let file = t.exec_dir(&id).join("work/src/a/b/c/mod.py");
        let folder = t.exec_dir(&id).join("work/src/a");
        if verdict["applied"] == true {
            applied += 1;
            assert_eq!(std::fs::read_to_string(&file).unwrap(), "x = 1\n", "{point}");
        } else {
            assert!(!folder.exists(), "{point} (delay {delay_us}): NOT APPLIED but folders were left behind");
        }
        match *point {
            "P2" => assert_eq!(verdict["applied"], false, "P2"),
            "P4" | "P6" => assert_eq!(verdict["applied"], true, "{point}"),
            _ => {}
        }
        assert_work_equals_last_checkpoint(&t, &id);
        // The run can be finished from here.
        t.agentos(&["run", "--quiet", "--resume", &id]);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "x = 1\n", "{point}: after resume");
        assert_work_equals_last_checkpoint(&t, &id);
    }
    assert!(applied >= 2, "P4 and P6 must both be APPLIED");
}

// ---------------------------------------------------------------------------
// Messages a newcomer meets first (Findings, 2026-11-14).
// ---------------------------------------------------------------------------

#[test]
fn a_policy_mistake_is_reported_once_and_before_anything_is_started() {
    let t = TestEnv::new("hard-policy-typo");
    let text = std::fs::read_to_string(preset_policy()).unwrap().replace("memory_mb", "memory");
    let t = t.with_policy_text(&text);
    let script = repo_root().join("examples/scripts/fix-demo.toml");
    let out = t.output(&run_args(&t, &script, &demo_repo()).iter().map(String::as_str).collect::<Vec<_>>());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert_eq!(err.matches("unknown field `memory`").count(), 1, "the mistake must be named exactly once:\n{err}");
    assert!(err.contains("memory_mb"), "the right name is offered:\n{err}");
    assert!(!t.state.join("executions").exists());
}

#[test]
fn a_wrong_repo_path_is_reported_as_that() {
    let t = TestEnv::new("hard-bad-repo");
    let script = repo_root().join("examples/scripts/fix-demo.toml");
    // An empty PATH would also be a problem (no pytest); the repo comes first.
    let args = run_args(&t, &script, Path::new("/no/such/repo"));
    let out = t.cmd(&args.iter().map(String::as_str).collect::<Vec<_>>()).env("PATH", "").output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && err.contains("--repo /no/such/repo"), "{err}");
}

#[test]
fn without_a_delegated_cgroup_the_message_starts_with_what_to_do() {
    // A plain scope (no Delegate=yes) is what a newcomer's terminal is.
    let t = TestEnv::new("hard-not-delegated");
    let script = repo_root().join("examples/scripts/fix-demo.toml");
    let mut args = vec!["--user".to_string(), "--scope".into(), "-q".into(), agentos_bin().display().to_string(), "--state-dir".into(), t.state.display().to_string()];
    args.extend(run_args(&t, &script, &demo_repo()));
    let out = std::process::Command::new("systemd-run").args(&args).current_dir(repo_root()).output().expect("systemd-run");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "it ran in a scope that was not delegated:\n{err}");
    let fix = err.find("systemd-run --user --scope -p Delegate=yes").unwrap_or_else(|| panic!("the command to use is missing:\n{err}"));
    let found = err.find("what was found").unwrap_or_else(|| panic!("the finding is missing:\n{err}"));
    assert!(fix < found, "the fix must come before the finding:\n{err}");
    assert!(!t.state.join("executions").exists());
}

// ---------------------------------------------------------------------------
// D-090: the patch says what it cannot carry.
// ---------------------------------------------------------------------------

#[test]
fn the_patch_lists_what_it_cannot_carry() {
    let t = TestEnv::new("hard-patch-left-out").with_policy_text(&policy_with_python());
    let code = r##"open("/work/src/run.sh", "w").write("#!/bin/sh\necho hi\n"); os.chmod("/work/src/run.sh", 0o755)
open("/work/src/blob.bin", "wb").write(bytes(range(256)))
os.chmod("/work/src/stats.py", 0o600)
os.mkdir("/work/src/empty")
open("/work/src/pkg_init.py", "w").close()
open("/work/src/plain.py", "w").write("y = 2\n")"##;
    let script = t.write_script("s.toml", &format!("task = \"left out\"\n{}", exec_step(code)));
    let args = run_args(&t, &script, &demo_repo());
    let args: Vec<&str> = args.iter().map(String::as_str).filter(|a| *a != "--quiet").collect();
    let shown = t.agentos(&args);
    assert!(shown.contains("NOT in the patch: 1 binary file, 2 file modes, 1 empty file, 1 empty folder."), "{shown}");
    let id = only_execution(&t);
    let patch_path = t.exec_dir(&id).join("result.patch");
    let patch = std::fs::read_to_string(&patch_path).unwrap();
    let work = t.exec_dir(&id).join("work");
    for line in [
        "# agentos: this patch leaves out 5 thing(s)".to_string(),
        "# agentos: empty new file not in this patch: src/pkg_init.py (create it: touch src/pkg_init.py)".to_string(),
        format!("# agentos: binary file not in this patch: src/blob.bin (copy it from {})", work.join("src/blob.bin").display()),
        "# agentos: new file is executable, but a patch creates plain files: src/run.sh (chmod 755)".to_string(),
        "# agentos: empty folder not in this patch: src/empty".to_string(),
    ] {
        assert!(patch.contains(&line), "missing `{line}` in:\n{patch}");
    }
    assert!(patch.lines().any(|l| l.starts_with("# agentos: mode changed") && l.contains("src/stats.py") && l.ends_with("-> 600)")), "{patch}");
    assert!(!patch.lines().any(|l| l.starts_with("# agentos:") && l.contains("plain.py")), "an ordinary new file needs no note");
    // The comment lines do not get in the way: the text part still applies.
    let target = t.root.join("apply");
    assert!(std::process::Command::new("cp").arg("-a").arg(demo_repo()).arg(&target).status().unwrap().success());
    let st = std::process::Command::new("patch").args(["-p1", "--quiet", "-i"]).arg(&patch_path).current_dir(&target).output().unwrap();
    assert!(st.status.success(), "{}{}", String::from_utf8_lossy(&st.stdout), String::from_utf8_lossy(&st.stderr));
    assert_eq!(std::fs::read_to_string(target.join("src/plain.py")).unwrap(), "y = 2\n");
    assert!(!target.join("src/blob.bin").exists(), "this is the gap the note is about");
    assert!(!target.join("src/pkg_init.py").exists(), "B-012: `patch` does not create an empty new file");
}

#[test]
fn a_run_that_changes_only_text_gets_no_note() {
    let t = TestEnv::new("hard-patch-clean");
    let script = t.write_script("s.toml", &format!("task = \"text only\"\n{}", write_step("src/newpkg/mod.py")));
    let args = run_args(&t, &script, &demo_repo());
    let args: Vec<&str> = args.iter().map(String::as_str).filter(|a| *a != "--quiet").collect();
    let shown = t.agentos(&args);
    assert!(!shown.contains("NOT in the patch"), "{shown}");
    let patch = std::fs::read_to_string(t.exec_dir(&only_execution(&t)).join("result.patch")).unwrap();
    assert!(!patch.contains("# agentos:"), "{patch}");
}

#[test]
fn a_relative_state_dir_works() {
    // B-011: `--state-dir state` made the run fail at its last step.
    let t = TestEnv::new("hard-relative-state");
    let script = t.write_script("s.toml", &format!("task = \"relative\"\n{}[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\"]\n", write_step("src/extra.py")));
    let repo = demo_repo();
    let run = |args: &[&str], env: Option<&str>| {
        let mut c = std::process::Command::new(agentos_bin());
        c.current_dir(&t.root).args(args);
        if let Some(dir) = env {
            c.env("AGENTOS_STATE_DIR", dir);
        }
        let out = c.output().unwrap();
        assert!(out.status.success(), "{args:?}: {}", String::from_utf8_lossy(&out.stderr));
        String::from_utf8(out.stdout).unwrap()
    };
    let tail = ["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()];
    let id = run(&[&["--state-dir", "state"], &tail[..]].concat(), None).trim().to_string();
    assert!(t.root.join("state/executions").join(&id).join("result.patch").exists());
    assert!(run(&["--state-dir", "state", "trace", &id], None).contains("fs.write"));
    let id2 = run(&tail, Some("./by-env")).trim().to_string();
    assert!(t.root.join("by-env/executions").join(&id2).join("result.patch").exists());
}

#[test]
fn a_machine_without_user_namespaces_is_told_so_by_trying_not_by_guessing() {
    // D-093: the startup check really tries to build a user namespace. Here
    // the limit is set to 0 inside a throwaway user namespace, which is how
    // a machine that forbids them looks from the inside.
    let t = TestEnv::new("hard-no-userns");
    let script = repo_root().join("examples/scripts/fix-demo.toml");
    let args = run_args(&t, &script, &demo_repo());
    let inner = format!(
        "echo 0 > /proc/sys/user/max_user_namespaces && exec {} --state-dir {} {}",
        agentos_bin().display(),
        t.state.display(),
        args.iter().map(|a| format!("'{a}'")).collect::<Vec<_>>().join(" ")
    );
    let out = std::process::Command::new("unshare").args(["-Ur", "sh", "-c", &inner]).current_dir(repo_root()).output().expect("unshare");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "{err}");
    assert!(err.contains("user namespaces cannot be used"), "{err}");
    assert!(err.contains("max_user_namespaces is 0"), "{err}");
    assert!(!t.state.join("executions").exists());
}
