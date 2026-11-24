//! `norn apply` (D-108): copy the result of a finished execution into a
//! repository, including what a patch cannot carry.
//!
//! This is the only command that writes outside the state folder, so the
//! tests are mostly about when it must *not* write: a repository that
//! changed since the run copied it, a symlink in the way, an execution that
//! is not finished, files the run never saw.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test apply

mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use common::*;

/// The preset policy with `python3` allowed, so a step can run chosen code.
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

/// Everything a patch cannot carry, and the ordinary things too.
const CHANGES: &str = r##"
import os
open("/work/src/stats.py", "a").write("\n# a changed text file\n")
os.makedirs("/work/src/newpkg/sub")
open("/work/src/newpkg/__init__.py", "w").close()                       # an empty file
open("/work/src/newpkg/sub/mod.py", "w").write("x = 1\n")
open("/work/src/blob.bin", "wb").write(bytes(range(256)) * 4)           # a binary file
open("/work/src/run.sh", "w").write("#!/bin/sh\necho hi\n"); os.chmod("/work/src/run.sh", 0o755)
os.chmod("/work/tests/test_stats.py", 0o600)                            # only the mode
os.remove("/work/src/old.py")                                           # a deleted file
os.mkdir("/work/src/empty")                                             # an empty folder
os.symlink("stats.py", "/work/src/link.py")                             # a symlink
"##;

struct Setup {
    t: TestEnv,
    /// The repository the run was started on (a copy of the demo repo).
    repo: PathBuf,
    id: String,
}

/// A repository, and a finished run on it that made `CHANGES`.
fn finished_run(name: &str) -> Setup {
    let t = TestEnv::new(name).with_policy_text(&policy_with_python());
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    std::fs::write(repo.join("src/old.py"), "old = True\n").unwrap();
    assert!(repo.join(".env").exists(), "the demo repo has a denied file; these tests rely on it");
    let script = t.write_script("s.toml", &format!("task = \"changes\"\n[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", '''{CHANGES}''']\n"));
    let id = t.run_script_on(script.to_str().unwrap(), &repo);
    let events = t.events(&id);
    assert!(events.iter().any(|e| e["type"] == "tool.completed" && e["call_id"] == 1), "the command did not complete: {events:#?}");
    Setup { t, repo, id }
}

/// A tree's digest without the paths the run never saw.
fn digest_without(dir: &Path, unseen: &[&str]) -> Vec<String> {
    tree_digest(dir).into_iter().filter(|line| !unseen.iter().any(|u| line.split(' ').nth(1) == Some(u))).collect()
}

#[test]
fn apply_copies_everything_the_run_changed_and_nothing_else() {
    let s = finished_run("apply-all");
    let work = s.t.exec_dir(&s.id).join("work");
    let env_before = std::fs::read(s.repo.join(".env")).unwrap();

    // A dry run lists the changes and makes none.
    let before = tree_digest(&s.repo);
    let shown = s.t.norn(&["apply", &s.id, "--dry-run"]);
    for line in [
        "M src/stats.py",
        "A src/newpkg/",
        "A src/newpkg/__init__.py",
        "A src/newpkg/sub/mod.py",
        "A src/blob.bin",
        "A src/run.sh (executable, mode 755)",
        "mode tests/test_stats.py 644 -> 600",
        "D src/old.py",
        "A src/empty/",
        "L src/link.py -> stats.py",
        "would be made",
    ] {
        assert!(shown.contains(line), "`{line}` is missing from:\n{shown}");
    }
    assert_eq!(tree_digest(&s.repo), before, "a dry run changed the repository");

    // The real thing.
    let shown = s.t.norn(&["apply", &s.id]);
    assert!(shown.contains("11 change(s) made"), "{shown}");
    assert_eq!(digest_without(&s.repo, &[".env"]), tree_digest(&work), "the repository does not equal the run's result");
    assert_eq!(std::fs::read(s.repo.join("src/blob.bin")).unwrap(), (0..=255u8).collect::<Vec<_>>().repeat(4));
    assert_eq!(std::fs::metadata(s.repo.join("src/run.sh")).unwrap().permissions().mode() & 0o777, 0o755);
    assert_eq!(std::fs::metadata(s.repo.join("src/newpkg/__init__.py")).unwrap().len(), 0);
    assert!(!s.repo.join("src/old.py").exists());
    // The denied file was never in the workspace, so apply left it alone.
    assert_eq!(std::fs::read(s.repo.join(".env")).unwrap(), env_before);
    // No temp files left behind.
    assert!(!tree_digest(&s.repo).iter().any(|l| l.contains("norn-apply")));

    // The log knows, and so does the trace.
    let events = s.t.events(&s.id);
    let applied = events.iter().find(|e| e["type"] == "result.applied").expect("a result.applied event");
    assert_eq!(applied["target"], s.repo.canonicalize().unwrap().display().to_string());
    assert_eq!(applied["changes"].as_array().unwrap().len(), 11);
    assert!(s.t.norn(&["trace", &s.id]).contains("applied: 11 change(s) to"));

    // A second apply finds the work done and is not an error.
    let again = s.t.norn(&["apply", &s.id]);
    assert!(again.contains("already holds the result"), "{again}");
    assert_eq!(digest_without(&s.repo, &[".env"]), tree_digest(&work));
    assert_eq!(s.t.events(&s.id).iter().filter(|e| e["type"] == "result.applied").count(), 1, "nothing was applied the second time");
}

#[test]
fn apply_refuses_a_repository_that_changed_since_the_copy() {
    let s = finished_run("apply-conflict");
    // The user kept working: one file the run also changed, one it deleted.
    std::fs::write(s.repo.join("src/stats.py"), "# my own edit\n").unwrap();
    std::fs::write(s.repo.join("src/old.py"), "old = False  # I still need this\n").unwrap();
    // And one unrelated file, which must not matter.
    std::fs::write(s.repo.join("README.md"), "my notes\n").unwrap();
    let before = tree_digest(&s.repo);

    let out = s.t.output(&["apply", &s.id]);
    let (shown, err) = (String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(!out.status.success());
    assert!(shown.contains("Nothing was changed"), "{shown}");
    assert!(shown.contains("src/stats.py:") && shown.contains("src/old.py:"), "both conflicts are named:\n{shown}");
    assert!(!shown.contains("README.md"), "a file the run did not touch is not a conflict:\n{shown}");
    assert!(err.contains("2 path(s) stand in the way") && err.contains("result.patch"), "{err}");
    assert_eq!(tree_digest(&s.repo), before, "a refused apply changed something");
    assert!(!s.t.events(&s.id).iter().any(|e| e["type"] == "result.applied"));

    // With the two files put back, it goes through, and the unrelated edit stays.
    std::fs::copy(repo_root().join("examples/demo-repo/src/stats.py"), s.repo.join("src/stats.py")).unwrap();
    std::fs::write(s.repo.join("src/old.py"), "old = True\n").unwrap();
    s.t.norn(&["apply", &s.id]);
    assert_eq!(std::fs::read_to_string(s.repo.join("README.md")).unwrap(), "my notes\n");
    assert!(std::fs::read_to_string(s.repo.join("src/stats.py")).unwrap().ends_with("# a changed text file\n"));
}

#[test]
fn apply_can_target_another_copy_and_leaves_the_original_alone() {
    let s = finished_run("apply-to");
    let other = s.t.root.join("other");
    assert!(std::process::Command::new("cp").arg("-a").arg(&s.repo).arg(&other).status().unwrap().success());
    let original = tree_digest(&s.repo);
    s.t.norn(&["apply", &s.id, "--to", other.to_str().unwrap()]);
    assert_eq!(tree_digest(&s.repo), original, "the original repository was touched");
    assert_eq!(digest_without(&other, &[".env"]), tree_digest(&s.t.exec_dir(&s.id).join("work")));
    // Not a folder, not there: refused with the path.
    for bad in ["/no/such/folder", s.repo.join("README.md").to_str().unwrap()] {
        let out = s.t.output(&["apply", &s.id, "--to", bad]);
        assert!(!out.status.success(), "{bad}");
    }
}

#[test]
fn apply_does_not_write_through_a_symlink_in_the_repository() {
    let s = finished_run("apply-symlink");
    // After the run, `src` in the repository is swapped for a link to a
    // folder elsewhere that holds the same files.
    let outside = s.t.root.join("outside");
    std::fs::rename(s.repo.join("src"), &outside).unwrap();
    std::os::unix::fs::symlink(&outside, s.repo.join("src")).unwrap();
    let before = tree_digest(&outside);
    let out = s.t.output(&["apply", &s.id]);
    let shown = String::from_utf8_lossy(&out.stdout);
    assert!(!out.status.success(), "{shown}");
    assert!(shown.contains("is a symlink in the repository"), "{shown}");
    assert_eq!(tree_digest(&outside), before, "apply wrote through the symlink");
    assert_eq!(std::fs::metadata(s.repo.join("tests/test_stats.py")).unwrap().permissions().mode() & 0o777, 0o644, "nothing at all was applied");
}

#[test]
fn apply_needs_a_finished_execution() {
    let t = TestEnv::new("apply-unfinished");
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    let script = repo_root().join("examples/scripts/fix-demo.toml");
    let args = ["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()];
    // Killed after the write of call 6, before its checkpoint.
    let st = t.cmd(&args).env("NORN_CRASH_AT", "P4").env("NORN_CRASH_CALL", "6").output().unwrap();
    assert!(!st.status.success());
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let before = tree_digest(&repo);

    let out = t.output(&["apply", &id]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && err.contains("has not finished") && err.contains("--resume"), "{err}");
    assert_eq!(tree_digest(&repo), before);
    assert!(!t.output(&["apply", "no-such-id"]).status.success());

    // Resumed to the end, it applies, and the repository's tests pass.
    t.norn(&["run", "--quiet", "--resume", &id]);
    let shown = t.norn(&["apply", &id]);
    assert!(shown.contains("M src/stats.py") && shown.contains("1 change(s) made"), "{shown}");
    let pytest = std::process::Command::new("pytest").args(["-q", "-p", "no:cacheprovider"]).current_dir(&repo).env("PYTHONDONTWRITEBYTECODE", "1").output().unwrap();
    assert!(pytest.status.success(), "{}", String::from_utf8_lossy(&pytest.stdout));
}

#[test]
fn a_run_that_changed_nothing_has_nothing_to_apply_and_caches_are_not_applied() {
    let t = TestEnv::new("apply-nothing");
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    // pytest's cache on: the run leaves `.pytest_cache` in the workspace (a scratch path).
    std::fs::write(repo.join("pytest.ini"), "[pytest]\npythonpath = src\ntestpaths = tests\n").unwrap();
    let script = t.write_script("s.toml", "task = \"look\"\n[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\"]\n[[step]]\ntool = \"fs.read\"\npath = \"src/stats.py\"\n");
    let id = t.run_script_on(script.to_str().unwrap(), &repo);
    assert!(t.exec_dir(&id).join("work/.pytest_cache").is_dir());
    let before = tree_digest(&repo);
    let shown = t.norn(&["apply", &id]);
    assert!(shown.contains("nothing to apply"), "{shown}");
    assert_eq!(tree_digest(&repo), before);
    assert!(!repo.join(".pytest_cache").exists(), "a cache was applied");
    assert!(!t.events(&id).iter().any(|e| e["type"] == "result.applied"));
}
