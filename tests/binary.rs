//! Binary files and the executable bit through the agent's own file tools
//! (D-123): `fs.read` and `fs.write` with `encoding = "base64"`, `fs.write`
//! with `executable`, and `executable` in `fs.list`.
//!
//! What must hold: the bytes that reach the file are exactly the bytes the
//! base64 text stands for, and the other way round; a request that is not
//! valid changes nothing; the mode is set together with the content; a kill
//! around a write that changes only the mode is still classified correctly.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test binary

mod common;

use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};

use norn::blob::content_hash;
use common::*;
use serde_json::Value;

/// Every byte value twice, in two orders: not UTF-8, and it has NUL bytes.
fn sample() -> Vec<u8> {
    (0..=255u8).chain((0..=255u8).rev()).collect()
}

/// What the `base64` program makes of a file (one line).
fn coreutils_base64(path: &Path) -> String {
    let out = std::process::Command::new("base64").arg("-w0").arg(path).output().expect("the base64 program");
    assert!(out.status.success());
    String::from_utf8(out.stdout).unwrap()
}

fn make_repo(t: &TestEnv) -> PathBuf {
    let repo = t.root.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("src/data.bin"), sample()).unwrap();
    std::fs::write(repo.join("src/tool.sh"), "#!/bin/sh\necho tool\n").unwrap();
    std::fs::set_permissions(repo.join("src/tool.sh"), std::fs::Permissions::from_mode(0o755)).unwrap();
    std::fs::write(repo.join("src/plain.txt"), "plain\n").unwrap();
    repo
}

fn run(t: &TestEnv, repo: &Path, steps: &str) -> (String, Vec<Value>) {
    let script = t.write_script("s.toml", &format!("task = \"binary\"\n{steps}"));
    let id = t.run_script_on(script.to_str().unwrap(), repo);
    let events = t.events(&id);
    (id, events)
}

fn mode(p: &Path) -> u32 {
    std::fs::metadata(p).unwrap().permissions().mode() & 0o7777
}

fn refused(events: &[Value], call: u64) -> String {
    let e = events
        .iter()
        .find(|e| (e["type"] == "tool.failed" || e["type"] == "tool.denied") && e["call_id"] == call)
        .unwrap_or_else(|| panic!("call {call} was neither failed nor denied: {events:#?}"));
    format!("{} {}", e["error"].as_str().unwrap_or(""), e["reason"].as_str().unwrap_or(""))
}

#[test]
fn binary_files_are_read_and_written_byte_for_byte() {
    let t = TestEnv::new("bin-roundtrip");
    let repo = make_repo(&t);
    let coded = coreutils_base64(&repo.join("src/data.bin"));
    let new_bytes: Vec<u8> = sample().into_iter().rev().chain([0, 255, 0]).collect();
    std::fs::write(t.root.join("new.bin"), &new_bytes).unwrap();
    let new_coded = coreutils_base64(&t.root.join("new.bin"));
    // Wrapped the way the base64 program wraps by default: must still be read.
    let wrapped: String = new_coded.as_bytes().chunks(76).map(|c| std::str::from_utf8(c).unwrap()).collect::<Vec<_>>().join("\n");
    let steps = format!(
        "[[step]]\ntool = \"fs.read\"\npath = \"src/data.bin\"\nencoding = \"base64\"\n\
         [[step]]\ntool = \"fs.write\"\npath = \"src/new/out.bin\"\nencoding = \"base64\"\ncontent = '''\n{wrapped}\n'''\n\
         [[step]]\ntool = \"fs.read\"\npath = \"src/new/out.bin\"\nencoding = \"base64\"\n\
         [[step]]\ntool = \"fs.write\"\npath = \"src/data.bin\"\nencoding = \"base64\"\ncontent = \"\"\n\
         [[step]]\ntool = \"fs.read\"\npath = \"src/plain.txt\"\nencoding = \"base64\"\n"
    );
    let (id, events) = run(&t, &repo, &steps);
    let out = |call: u64| t.output_of(&id, &events, call).unwrap_or_else(|| panic!("call {call} did not complete: {events:#?}"));
    // Reading: the same text the base64 program gives for the same file.
    assert_eq!(out(1)["content"].as_str().unwrap(), coded);
    assert_eq!(out(1)["encoding"], "base64");
    // Writing: exactly the bytes, in a folder that did not exist.
    let work = t.exec_dir(&id).join("work");
    assert_eq!(std::fs::read(work.join("src/new/out.bin")).unwrap(), new_bytes);
    assert_eq!(out(2)["bytes"], new_bytes.len());
    assert_eq!(out(3)["content"].as_str().unwrap(), new_coded);
    // Empty base64 is an empty file; text can be read as base64 too.
    assert_eq!(std::fs::read(work.join("src/data.bin")).unwrap().len(), 0);
    assert_eq!(out(5)["content"], "cGxhaW4K");
    // The log names the hash and size of the bytes in the file, not of the
    // base64 text, and the checkpoint holds those bytes.
    let requested = events.iter().find(|e| e["type"] == "tool.requested" && e["call_id"] == 2).unwrap();
    assert_eq!(requested["call"]["content_hash"].as_str().unwrap(), content_hash(&new_bytes));
    assert_eq!(requested["call"]["bytes"], new_bytes.len());
    let done = events.iter().find(|e| e["type"] == "tool.completed" && e["call_id"] == 2).unwrap();
    assert_eq!(std::fs::read(t.exec_dir(&id).join("checkpoints").join(done["checkpoint"].as_str().unwrap()).join("src/new/out.bin")).unwrap(), new_bytes);
    // `norn apply` carries the binary file into the repository.
    t.norn(&["apply", &id]);
    assert_eq!(std::fs::read(repo.join("src/new/out.bin")).unwrap(), new_bytes);
}

#[test]
fn requests_that_are_not_valid_change_nothing_and_say_why() {
    let policy = std::fs::read_to_string(preset_policy()).unwrap().replace("max_output_kb  = 64", "max_output_kb  = 1");
    assert!(policy.contains("max_output_kb  = 1\n"));
    let t = TestEnv::new("bin-invalid").with_policy_text(&policy);
    let repo = make_repo(&t);
    std::fs::write(repo.join("src/big.bin"), vec![0xffu8; 900]).unwrap(); // 1200 characters of base64 > 1024
    let w = |extra: &str, content: &str| format!("[[step]]\ntool = \"fs.write\"\npath = \"src/x.bin\"\n{extra}content = \"{content}\"\n");
    let steps = [
        "[[step]]\ntool = \"fs.read\"\npath = \"src/data.bin\"\n".to_string(),                                              // 1: binary as text
        "[[step]]\ntool = \"fs.read\"\npath = \"src/data.bin\"\nencoding = \"base64\"\nstart_line = 1\nmax_lines = 2\n".to_string(), // 2: lines of base64
        "[[step]]\ntool = \"fs.read\"\npath = \"src/data.bin\"\nencoding = \"hex\"\n".to_string(),                            // 3: unknown encoding
        "[[step]]\ntool = \"fs.read\"\npath = \"src/big.bin\"\nencoding = \"base64\"\n".to_string(),                          // 4: over the limit
        w("encoding = \"base64\"\n", "not base64!"),                                                                         // 5
        w("encoding = \"base64\"\n", "Zg="),                                                                                 // 6: wrong length
        w("encoding = \"hex\"\n", "00ff"),                                                                                   // 7
        "[[step]]\ntool = \"fs.edit\"\npath = \"src/data.bin\"\nold_str = \"a\"\nnew_str = \"b\"\n".to_string(),              // 8: edit of a binary file
        "[[step]]\ntool = \"fs.list\"\npath = \"src\"\n".to_string(),                                                        // 9
    ]
    .concat();
    let (id, events) = run(&t, &repo, &steps);
    assert!(refused(&events, 1).contains("encoding = \"base64\""), "the error names the way out: {}", refused(&events, 1));
    assert!(refused(&events, 2).contains("cannot be used with encoding"));
    assert!(refused(&events, 3).contains("unknown encoding `hex`"));
    assert!(refused(&events, 4).contains("1200 characters of base64"), "{}", refused(&events, 4));
    assert!(refused(&events, 5).contains("not valid base64"));
    assert!(refused(&events, 6).contains("multiple of 4"));
    assert!(refused(&events, 7).contains("unknown encoding `hex`"));
    assert!(refused(&events, 8).contains("not UTF-8"));
    let listed = t.output_of(&id, &events, 9).unwrap();
    let names: Vec<&str> = listed["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["big.bin", "data.bin", "plain.txt", "tool.sh"], "no x.bin and no temporary file");
    let dir = t.exec_dir(&id);
    assert_eq!(std::fs::read(dir.join("work/src/data.bin")).unwrap(), sample());
    assert_eq!(std::fs::read_dir(dir.join("checkpoints")).unwrap().count(), 1, "nothing was changed, so only cp-0000");
}

#[test]
fn the_executable_bit_is_set_cleared_kept_and_shown() {
    let t = TestEnv::new("bin-mode");
    let repo = make_repo(&t);
    let w = |path: &str, extra: &str| format!("[[step]]\ntool = \"fs.write\"\npath = \"{path}\"\n{extra}content = \"#!/bin/sh\\necho hi\\n\"\n");
    let steps = [
        w("src/new.sh", "executable = true\n"),   // 1: a new script
        w("src/tool.sh", ""),                      // 2: an executable file rewritten: stays executable
        w("src/plain.txt", ""),                    // 3: a plain file rewritten: stays plain
        w("src/other.sh", ""),                     // 4: a new file without the flag: not executable
        w("src/plain.txt", "executable = true\n"), // 5: the same content again, only the mode changes
        "[[step]]\ntool = \"fs.edit\"\npath = \"src/new.sh\"\nold_str = \"echo hi\"\nnew_str = \"echo ho\"\n".to_string(), // 6: an edit keeps the mode
        "[[step]]\ntool = \"fs.list\"\npath = \"src\"\n".to_string(), // 7
        w("src/tool.sh", "executable = false\n"),  // 8: cleared
    ]
    .concat();
    let (id, events) = run(&t, &repo, &steps);
    let dir = t.exec_dir(&id);
    let work = dir.join("work");
    assert_eq!(mode(&work.join("src/new.sh")), 0o755);
    assert_eq!(mode(&work.join("src/plain.txt")), 0o755);
    assert_eq!(mode(&work.join("src/other.sh")), 0o644);
    assert_eq!(mode(&work.join("src/tool.sh")), 0o644, "cleared by call 8");
    // What fs.list said before call 8.
    let listed = t.output_of(&id, &events, 7).unwrap();
    let flags: Vec<(String, bool)> =
        listed["entries"].as_array().unwrap().iter().map(|e| (e["name"].as_str().unwrap().to_string(), e["executable"] == true)).collect();
    let want = [("data.bin", false), ("new.sh", true), ("other.sh", false), ("plain.txt", true), ("tool.sh", true)];
    assert_eq!(flags, want.map(|(n, x)| (n.to_string(), x)));
    // The mode is in the checkpoint taken right after the write.
    let cp_of = |call: u64| dir.join("checkpoints").join(events.iter().find(|e| e["type"] == "tool.completed" && e["call_id"] == call).unwrap()["checkpoint"].as_str().unwrap());
    assert_eq!(mode(&cp_of(1).join("src/new.sh")), 0o755);
    assert_eq!(mode(&cp_of(5).join("src/plain.txt")), 0o755);
    assert_eq!(mode(&cp_of(4).join("src/plain.txt")), 0o644, "before call 5 it was still plain");
    // The intent says which mode a write sets, and only when it sets one.
    let plan = |call: u64| events.iter().find(|e| e["type"] == "tool.intent" && e["call_id"] == call).unwrap()["plan"].clone();
    assert_eq!(plan(1)["mode"], 0o755);
    assert!(plan(2).get("mode").is_none());
    // `norn apply` carries the modes.
    let shown = t.norn(&["apply", &id]);
    assert!(shown.contains("src/new.sh (executable, mode 755)"), "{shown}");
    assert_eq!(mode(&repo.join("src/new.sh")), 0o755);
    assert_eq!(mode(&repo.join("src/tool.sh")), 0o644);
}

/// Kill the run at `point` during call 1, then recover. Returns (the
/// recovery's verdict for call 1, the file's mode after recovery, the id).
fn kill_and_recover(name: &str, point: &str, step: &str) -> (Option<bool>, u32, TestEnv, String) {
    let t = TestEnv::new(name);
    let repo = make_repo(&t);
    let script = t.write_script("s.toml", &format!("task = \"mode under a kill\"\n{step}[[step]]\ntool = \"fs.list\"\npath = \"src\"\n"));
    let killed = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--agent", "scripted", "--script", script.to_str().unwrap()])
        .env("NORN_CRASH_AT", point)
        .env("NORN_CRASH_CALL", "1")
        .output()
        .unwrap();
    assert_eq!(killed.status.signal(), Some(9), "{point}: {}", String::from_utf8_lossy(&killed.stderr));
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    t.norn(&["recover", "--quiet", &id]);
    let verdict = t.events(&id).iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == 1).map(|e| e["applied"] == true);
    let m = mode(&t.exec_dir(&id).join("work/src/plain.txt"));
    (verdict, m, t, id)
}

#[test]
fn a_kill_around_a_write_that_only_changes_the_mode_is_classified_by_the_mode() {
    // The content is what the file already holds, so its hash is the same
    // before and after: only the mode shows whether the write happened.
    let step = "[[step]]\ntool = \"fs.write\"\npath = \"src/plain.txt\"\nexecutable = true\ncontent = \"plain\\n\"\n";
    // Killed after the intent, before the effect: not applied, mode as it was.
    let (verdict, m, t, id) = kill_and_recover("bin-kill-p2", "P2", step);
    assert_eq!((verdict, m), (Some(false), 0o644));
    // The resumed run does it.
    t.norn(&["run", "--quiet", "--resume", &id]);
    assert_eq!(mode(&t.exec_dir(&id).join("work/src/plain.txt")), 0o755);
    assert!(t.events(&id).iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"));
    // Killed after the effect, before the checkpoint: applied, mode set,
    // and the checkpoint recovery takes holds it.
    let (verdict, m, t, id) = kill_and_recover("bin-kill-p4", "P4", step);
    assert_eq!((verdict, m), (Some(true), 0o755));
    let events = t.events(&id);
    let cp = events.iter().find(|e| e["type"] == "tool.reconciled" && e["call_id"] == 1).unwrap()["checkpoint"].as_str().unwrap().to_string();
    assert_eq!(mode(&t.exec_dir(&id).join("checkpoints").join(cp).join("src/plain.txt")), 0o755);
}
