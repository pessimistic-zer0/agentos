//! The tools added in v2 (M17, D-147): `fs.delete`, `fs.search`, and
//! `replace_all` for `fs.edit`, driven by the scripted agent. Their kill
//! cells are `a_delete_is_recovered_like_a_write` in `tests/recovery/main.rs`.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test agent_tools

mod common;

use common::*;
use serde_json::Value;

fn demo_copy(t: &TestEnv) -> std::path::PathBuf {
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    repo
}

fn run(t: &TestEnv, repo: &std::path::Path, steps: &str) -> (String, Vec<Value>) {
    let script = t.write_script("steps.toml", &format!("task = \"tools\"\n{steps}"));
    let id = t.run_script_on(script.to_str().unwrap(), repo);
    let events = t.events(&id);
    (id, events)
}

fn ending(events: &[Value], call: u64) -> &Value {
    events
        .iter()
        .find(|e| e["call_id"] == call && ["tool.completed", "tool.denied", "tool.failed"].contains(&e["type"].as_str().unwrap()))
        .unwrap_or_else(|| panic!("call {call} has no ending: {events:#?}"))
}

#[test]
fn fs_delete_removes_a_file_and_the_patch_says_so() {
    let t = TestEnv::new("tools-delete");
    let repo = demo_copy(&t);
    std::fs::write(repo.join("src/old.py"), "x = 1\n").unwrap();
    let (id, events) = run(
        &t,
        &repo,
        "[[step]]\ntool = \"fs.delete\"\npath = \"src/old.py\"\n\
         [[step]]\ntool = \"fs.delete\"\npath = \"src/old.py\"\n\
         [[step]]\ntool = \"fs.delete\"\npath = \"README.md\"\n\
         [[step]]\ntool = \"fs.delete\"\npath = \"src\"\n\
         [[step]]\ntool = \"fs.delete\"\npath = \"../outside\"\n",
    );
    let done = ending(&events, 1);
    assert_eq!(done["type"], "tool.completed", "{done}");
    assert!(done["checkpoint"].is_string(), "a delete is a change: it takes a checkpoint");
    assert_eq!(done["summary"], "deleted (6 bytes)");
    assert!(!t.exec_dir(&id).join("work/src/old.py").exists());
    assert!(ending(&events, 2)["error"].as_str().unwrap().contains("does not exist"), "a second delete fails");
    assert_eq!(ending(&events, 3)["type"], "tool.denied", "README.md is outside the write globs");
    assert_eq!(ending(&events, 4)["type"], "tool.failed", "a folder is not a file");
    assert_eq!(ending(&events, 5)["type"], "tool.denied", "outside the workspace");
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    assert!(patch.contains("--- a/src/old.py") && patch.contains("+++ /dev/null") || patch.contains("+++ b/src/old.py"), "{patch}");
    // `apply` carries the deletion into the repository.
    t.norn(&["apply", &id]);
    assert!(!repo.join("src/old.py").exists());
    assert!(repo.join("README.md").exists());
}

#[test]
fn fs_delete_never_follows_a_symlink() {
    let t = TestEnv::new("tools-delete-link");
    let repo = demo_copy(&t);
    std::os::unix::fs::symlink("stats.py", repo.join("src/link.py")).unwrap();
    let (id, events) = run(&t, &repo, "[[step]]\ntool = \"fs.delete\"\npath = \"src/link.py\"\n");
    assert!(ending(&events, 1)["error"].as_str().unwrap().contains("symlink"), "{events:#?}");
    assert!(t.exec_dir(&id).join("work/src/stats.py").exists(), "the target is untouched");
}

#[test]
fn fs_search_finds_lines_in_readable_files_only_and_skips_caches() {
    let policy = std::fs::read_to_string(preset_policy()).unwrap().replace("read  = [\"**\"]", "read  = [\"src/**\", \"tests/**\", \".pytest_cache/**\"]");
    let t = TestEnv::new("tools-search").with_policy_text(&policy);
    let repo = demo_copy(&t);
    std::fs::write(repo.join("README.md"), "mean is mentioned here too\n").unwrap();
    std::fs::create_dir_all(repo.join("src/deep/er")).unwrap();
    std::fs::write(repo.join("src/deep/er/notes.txt"), "a\nthe mean of it\n").unwrap();
    std::fs::write(repo.join("src/blob.bin"), [0xff, 0xfe, b'm', b'e', b'a', b'n']).unwrap();
    // A cache (a scratch path in the preset): readable here, never searched.
    std::fs::create_dir_all(repo.join(".pytest_cache")).unwrap();
    std::fs::write(repo.join(".pytest_cache/README.md"), "pytest cache directory\n").unwrap();
    let (id, events) = run(
        &t,
        &repo,
        "[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\"]\n\
         [[step]]\ntool = \"fs.search\"\npattern = \"mean\"\n\
         [[step]]\ntool = \"fs.search\"\npattern = \"mean\"\npath = \"src/deep\"\n\
         [[step]]\ntool = \"fs.search\"\npattern = \"cache directory\"\n\
         [[step]]\ntool = \"fs.search\"\npattern = \"x\"\npath = \"README.md\"\n\
         [[step]]\ntool = \"fs.search\"\npattern = \"\"\n",
    );
    let out = t.output_of(&id, &events, 2).unwrap();
    let found: Vec<String> = out["matches"].as_array().unwrap().iter().map(|m| format!("{}:{}", m["path"].as_str().unwrap(), m["line"])).collect();
    assert!(found.contains(&"src/deep/er/notes.txt:2".to_string()), "{found:?}");
    assert!(found.iter().any(|f| f.starts_with("src/stats.py:")) && found.iter().any(|f| f.starts_with("tests/test_stats.py:")), "{found:?}");
    assert!(!found.iter().any(|f| f.starts_with("README.md")), "README.md is not readable under this policy: {found:?}");
    assert!(!found.iter().any(|f| f.starts_with("src/blob.bin")), "a file that is not text is skipped");
    let sub = t.output_of(&id, &events, 3).unwrap();
    assert_eq!(sub["matches"].as_array().unwrap().len(), 1, "only under src/deep: {sub}");
    assert!(t.exec_dir(&id).join("work/.pytest_cache/README.md").exists(), "the cache is in the workspace");
    assert_eq!(t.output_of(&id, &events, 4).unwrap()["matches"].as_array().unwrap().len(), 0);
    assert_eq!(ending(&events, 5)["type"], "tool.denied", "searching a file that may not be read");
    assert!(ending(&events, 6)["error"].as_str().unwrap().contains("pattern is empty"));
    // A search changes nothing: no checkpoint.
    assert!(ending(&events, 2)["checkpoint"].is_null());
}

#[test]
fn fs_edit_with_replace_all_changes_every_occurrence() {
    let t = TestEnv::new("tools-replace-all");
    let repo = demo_copy(&t);
    let (id, events) = run(
        &t,
        &repo,
        "[[step]]\ntool = \"fs.edit\"\npath = \"src/stats.py\"\nold_str = \"values\"\nnew_str = \"numbers\"\n\
         [[step]]\ntool = \"fs.edit\"\npath = \"src/stats.py\"\nold_str = \"values\"\nnew_str = \"numbers\"\nreplace_all = true\n",
    );
    let first = ending(&events, 1);
    assert!(first["error"].as_str().unwrap().contains("more than once") && first["error"].as_str().unwrap().contains("replace_all"), "{first}");
    let second = ending(&events, 2);
    assert_eq!(second["type"], "tool.completed");
    let n = std::fs::read_to_string(repo.join("src/stats.py")).unwrap().matches("values").count();
    assert!(n > 1);
    assert!(second["summary"].as_str().unwrap().starts_with(&format!("replaced {n} occurrences")), "{second}");
    let text = std::fs::read_to_string(t.exec_dir(&id).join("work/src/stats.py")).unwrap();
    assert!(!text.contains("values") && text.contains("numbers"));
    let requested = events.iter().find(|e| e["type"] == "tool.requested" && e["call_id"] == 2).unwrap();
    assert_eq!(requested["call"]["replace_all"], true, "the log records it");
    assert!(t.norn(&["trace", &id]).contains("bytes, all)"), "the trace shows it");
}
