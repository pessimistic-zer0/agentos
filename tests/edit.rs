//! `fs.edit` (D-120): replace one piece of text in an existing file.
//!
//! What must hold: exactly the named text changes and nothing else; an edit
//! that does not fit one place changes nothing and says why; policy and path
//! confinement are those of `fs.write`, plus read access; and the result is
//! a checkpoint and a patch like any write. Kills are in `tests/recovery/`.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test edit

mod common;

use common::*;
use serde_json::Value;

const ORIGINAL: &str = include_str!("../examples/demo-repo/src/stats.py");

fn edit_step(path: &str, old: &str, new: &str) -> String {
    assert!(!old.contains("'''") && !new.contains("'''"));
    // TOML literal strings: a first newline after ''' is dropped, so add one.
    format!("[[step]]\ntool = \"fs.edit\"\npath = \"{path}\"\nold_str = '''\n{old}'''\nnew_str = '''\n{new}'''\n")
}

fn run(t: &TestEnv, steps: &str) -> (String, Vec<Value>) {
    let script = t.write_script("s.toml", &format!("task = \"edit\"\n{steps}"));
    let id = t.run_script(script.to_str().unwrap());
    let events = t.events(&id);
    (id, events)
}

fn failed(events: &[Value], call: u64) -> String {
    events
        .iter()
        .find(|e| e["type"] == "tool.failed" && e["call_id"] == call)
        .unwrap_or_else(|| panic!("call {call} did not fail: {events:#?}"))["error"]
        .as_str()
        .unwrap()
        .to_string()
}

fn denied(events: &[Value], call: u64) -> String {
    events
        .iter()
        .find(|e| e["type"] == "tool.denied" && e["call_id"] == call)
        .unwrap_or_else(|| panic!("call {call} was not denied: {events:#?}"))["rule"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn an_edit_changes_one_place_and_nothing_else() {
    let t = TestEnv::new("edit-ok");
    let steps = format!(
        "{}[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\"]\n",
        edit_step("src/stats.py", "    return sum(values) / (len(values) - 1)\n", "    return sum(values) / len(values)\n")
    );
    let (id, events) = run(&t, &steps);
    let work = t.exec_dir(&id).join("work");
    let now = std::fs::read_to_string(work.join("src/stats.py")).unwrap();
    assert_eq!(now, ORIGINAL.replace("(len(values) - 1)", "len(values)"), "only the named text changed");
    assert_eq!(exec_exit_codes(&t, &events), vec![Some(0)], "the one-line fix makes the demo's tests pass");

    // The result says where; the log holds hashes, not the texts.
    let out = t.output_of(&id, &events, 1).expect("the edit completed");
    assert_eq!(out["kind"], "edit");
    assert_eq!(out["line"], 8);
    assert_eq!(out["bytes"], now.len());
    let requested = events.iter().find(|e| e["type"] == "tool.requested" && e["call_id"] == 1).unwrap();
    assert_eq!(requested["call"]["tool"], "fs.edit");
    assert!(requested["call"]["old_hash"].is_string() && requested["call"].get("old_str").is_none(), "{requested}");
    // An edit is a mutating call: it has an intent like a write's and a checkpoint that holds it.
    let intent = events.iter().find(|e| e["type"] == "tool.intent" && e["call_id"] == 1).unwrap();
    assert!(intent["plan"]["pre_hash"].is_string() && intent["plan"]["new_hash"].is_string() && intent["plan"]["temp"].is_string(), "{intent}");
    let done = events.iter().find(|e| e["type"] == "tool.completed" && e["call_id"] == 1).unwrap();
    let cp = t.exec_dir(&id).join("checkpoints").join(done["checkpoint"].as_str().unwrap());
    assert_eq!(std::fs::read_to_string(cp.join("src/stats.py")).unwrap(), now);
    // The patch is the one changed line.
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    assert!(patch.contains("-    return sum(values) / (len(values) - 1)") && patch.contains("+    return sum(values) / len(values)"), "{patch}");
    assert_eq!(patch.lines().filter(|l| l.starts_with('-') && !l.starts_with("---")).count(), 1, "{patch}");
    // `agentos trace` shows it.
    let trace = t.agentos(&["trace", &id]);
    assert!(trace.contains("fs.edit") && trace.contains("replaced at line 8"), "{trace}");
}

#[test]
fn an_edit_that_does_not_fit_one_place_changes_nothing_and_says_why() {
    let t = TestEnv::new("edit-fail");
    let steps = [
        edit_step("src/stats.py", "    return sum(values) / len(values)  # not in the file\n", "x\n"), // 1: not found
        edit_step("src/stats.py", "    if not values:\n", "    if values is None:\n"),                     // 2: twice (lines 6 and 13)
        edit_step("src/nothing.py", "a\n", "b\n"),                                                        // 3: no such file
        "[[step]]\ntool = \"fs.edit\"\npath = \"src/stats.py\"\nold_str = \"\"\nnew_str = \"x\"\n".to_string(), // 4: empty
        edit_step("src/stats.py", "def mean(values):\n", "def mean(values):\n"),                         // 5: no change
        "[[step]]\ntool = \"fs.edit\"\npath = \"src\"\nold_str = \"a\"\nnew_str = \"b\"\n".to_string(),     // 6: a folder
        "[[step]]\ntool = \"fs.list\"\npath = \"src\"\n".to_string(),                                      // 7: the run goes on
    ]
    .concat();
    let (id, events) = run(&t, &steps);
    assert!(failed(&events, 1).contains("was not found") && failed(&events, 1).contains("fs.read"));
    let twice = failed(&events, 2);
    assert!(twice.contains("more than once") && twice.contains("line 6") && twice.contains("line 13"), "{twice}");
    assert!(failed(&events, 3).contains("does not exist") && failed(&events, 3).contains("fs.write"));
    assert!(failed(&events, 4).contains("old_str is empty"));
    assert!(failed(&events, 5).contains("the same"));
    assert!(events.iter().any(|e| (e["type"] == "tool.failed" || e["type"] == "tool.denied") && e["call_id"] == 6), "editing a folder must not work");
    assert!(t.output_of(&id, &events, 7).is_some());
    // Nothing changed, no checkpoint was taken, the patch is empty.
    let dir = t.exec_dir(&id);
    assert_eq!(std::fs::read_to_string(dir.join("work/src/stats.py")).unwrap(), ORIGINAL);
    assert_eq!(std::fs::read_dir(dir.join("checkpoints")).unwrap().count(), 1, "only cp-0000");
    assert_eq!(std::fs::read_to_string(dir.join("result.patch")).unwrap().trim(), "");
    assert!(std::fs::read_dir(dir.join("work/src")).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().starts_with(".agentos-tmp")));
}

#[test]
fn an_edit_needs_write_and_read_access_and_stays_in_the_workspace() {
    // A repo with a secret, a file outside the write paths, a symlink to a
    // file outside the workspace, and a file that is not text.
    let policy = std::fs::read_to_string(preset_policy()).unwrap();
    let narrow = policy.replace("read  = [\"**\"]", "read  = [\"src/**\", \"README.md\"]");
    assert_ne!(policy, narrow);
    let t = TestEnv::new("edit-policy").with_policy_text(&narrow);
    let repo = t.root.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("tests")).unwrap();
    std::fs::write(repo.join("src/a.py"), "x = 1\n").unwrap();
    std::fs::write(repo.join("tests/t.py"), "y = 1\n").unwrap();
    std::fs::write(repo.join("README.md"), "hello\n").unwrap();
    std::fs::write(repo.join(".env"), "TOKEN=secret\n").unwrap();
    std::fs::write(repo.join("src/blob.bin"), [0xff, 0xfe, 0x00, 0x41]).unwrap();
    let outside = t.root.join("outside.txt");
    std::fs::write(&outside, "x = 1\n").unwrap();
    std::os::unix::fs::symlink(&outside, repo.join("src/link.py")).unwrap();
    let e = |path: &str| format!("[[step]]\ntool = \"fs.edit\"\npath = \"{path}\"\nold_str = \"1\"\nnew_str = \"2\"\n");
    let steps = [
        e("README.md"),                       // 1: readable, not writable
        e("tests/t.py"),                      // 2: writable, not readable: its answers would describe the file
        "[[step]]\ntool = \"fs.edit\"\npath = \".env\"\nold_str = \"secret\"\nnew_str = \"x\"\n".to_string(), // 3: denied
        e("../outside.txt"),                  // 4: outside the workspace
        e(outside.to_str().unwrap()),         // 5: absolute path
        e("src/link.py"),                     // 6: a symlink to outside
        e("src/blob.bin"),                    // 7: not text
        e("src/a.py"),                        // 8: allowed
    ]
    .concat();
    let script = t.write_script("s.toml", &format!("task = \"edit policy\"\n{steps}"));
    let id = t.run_script_on(script.to_str().unwrap(), &repo);
    let events = t.events(&id);
    assert_eq!(denied(&events, 1), "no matching rule");
    assert_eq!(denied(&events, 2), "no matching rule", "an edit needs read access too");
    assert!(denied(&events, 3).starts_with("filesystem.deny"));
    for call in [4, 5] {
        assert!(events.iter().any(|e| e["type"] == "tool.denied" && e["call_id"] == call), "call {call} must be denied: {events:#?}");
    }
    assert!(failed(&events, 6).contains("symlink"), "{}", failed(&events, 6));
    assert!(failed(&events, 7).contains("not UTF-8"));
    assert!(t.output_of(&id, &events, 8).is_some());
    let work = t.exec_dir(&id).join("work");
    assert_eq!(std::fs::read_to_string(work.join("src/a.py")).unwrap(), "x = 2\n");
    assert_eq!(std::fs::read_to_string(work.join("tests/t.py")).unwrap(), "y = 1\n");
    assert_eq!(std::fs::read_to_string(work.join("README.md")).unwrap(), "hello\n");
    assert_eq!(std::fs::read_to_string(&outside).unwrap(), "x = 1\n", "the file outside the workspace is untouched");
    assert_eq!(std::fs::read_to_string(repo.join("src/a.py")).unwrap(), "x = 1\n", "the original repo is never modified");
    // The secret's text is nowhere in the execution: not in the log, not in a blob.
    let grep = std::process::Command::new("grep").args(["-r", "-l", "-a", "TOKEN=secret"]).arg(t.exec_dir(&id)).output().unwrap();
    assert!(grep.stdout.is_empty(), "{}", String::from_utf8_lossy(&grep.stdout));
}

#[test]
fn an_edit_keeps_the_files_mode_and_works_on_text_that_is_not_ascii() {
    let t = TestEnv::new("edit-mode");
    let repo = t.root.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    let text = "#!/bin/sh\n# größe → 3 ✓\necho größe\r\necho end";
    std::fs::write(repo.join("src/run.sh"), text).unwrap();
    std::fs::set_permissions(repo.join("src/run.sh"), std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    let script = t.write_script("s.toml", "task = \"mode\"\n[[step]]\ntool = \"fs.edit\"\npath = \"src/run.sh\"\nold_str = \"echo größe\\r\\n\"\nnew_str = \"echo Größe ✓\\r\\n\"\n");
    let id = t.run_script_on(script.to_str().unwrap(), &repo);
    let file = t.exec_dir(&id).join("work/src/run.sh");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "#!/bin/sh\n# größe → 3 ✓\necho Größe ✓\r\necho end", "line endings and the missing last newline are kept");
    let out = t.output_of(&id, &t.events(&id), 1).unwrap();
    assert_eq!(out["line"], 3);
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(std::fs::metadata(&file).unwrap().permissions().mode() & 0o777, 0o755, "the file stays executable");
}
