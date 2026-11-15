//! Packages from outside the repository (D-091, D-092).
//!
//! A real project's tests import packages that are not in the repository.
//! The sandbox shows a command only the copy of the repo and the read-only
//! folders the policy names, so such packages have to be named there:
//!
//! - a virtualenv: add its folder to `sandbox.ro_mounts` and start agentos
//!   with it activated (its `bin` first on `PATH`);
//! - any other folder of packages: add it to `sandbox.ro_mounts` and point
//!   at it with a variable in `[sandbox.env]`.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test deps

mod common;

use std::path::{Path, PathBuf};

use common::*;
use serde_json::Value;

/// The preset policy with one more read-only mount and extra variables.
fn policy_with(mount: Option<&Path>, env: &[(&str, &str)]) -> String {
    let mut out = String::new();
    for line in std::fs::read_to_string(preset_policy()).unwrap().lines() {
        match (line.strip_prefix("ro_mounts = ["), mount) {
            (Some(rest), Some(m)) => out.push_str(&format!("ro_mounts = [\"{}\", {rest}\n", m.display())),
            _ => out.push_str(&format!("{line}\n")),
        }
        if line == "[sandbox.env]" {
            for (k, v) in env {
                out.push_str(&format!("{k} = \"{v}\"\n"));
            }
        }
    }
    assert!(mount.is_none() || out.contains(&mount.unwrap().display().to_string()), "the preset has no ro_mounts line");
    assert!(env.is_empty() || out.contains(env[0].0), "the preset has no [sandbox.env] table");
    out
}

/// The demo repo plus one test that needs the package `fakedep` and prints
/// where Python and the package came from.
fn repo_needing_fakedep(t: &TestEnv) -> PathBuf {
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    std::fs::write(
        repo.join("tests/test_dep.py"),
        "import os, sys\nimport fakedep\n\n\ndef test_dep():\n    print('PREFIX', sys.prefix)\n    print('FAKEDEP', fakedep.__file__)\n    print('GREETING', os.environ.get('GREETING'))\n    assert fakedep.V == 1\n",
    )
    .unwrap();
    repo
}

const STEPS: &str = "task = \"deps\"\n\
    [[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\", \"-s\", \"tests/test_dep.py\"]\n\
    [[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-m\", \"pytest\", \"-q\", \"-s\", \"tests/test_dep.py\"]\n";

fn run(t: &TestEnv, repo: &Path, script: &Path, path_first: Option<&Path>) -> std::process::Output {
    let mut c = t.cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()]);
    if let Some(dir) = path_first {
        c.env("PATH", format!("{}:{}", dir.display(), std::env::var("PATH").unwrap()));
    }
    c.output().unwrap()
}

fn stdout_of(t: &TestEnv, id: &str, events: &[Value], call: u64) -> (Option<i64>, String) {
    let out = t.output_of(id, events, call).unwrap_or_else(|| panic!("call {call} did not complete: {events:#?}"));
    (out["exit_code"].as_i64(), format!("{}{}", out["stdout"].as_str().unwrap(), out["stderr"].as_str().unwrap()))
}

/// A virtualenv as `python3 -m venv` makes it, with one package and a
/// `pytest` launcher of the kind `pip` writes into `bin/`.
fn make_venv(t: &TestEnv) -> PathBuf {
    let venv = t.root.join("venv");
    let out = std::process::Command::new("python3").args(["-m", "venv", "--without-pip", "--system-site-packages"]).arg(&venv).output().unwrap();
    assert!(out.status.success(), "python3 -m venv failed: {}", String::from_utf8_lossy(&out.stderr));
    let lib = std::fs::read_dir(venv.join("lib")).unwrap().next().expect("venv/lib/pythonX.Y").unwrap().path();
    std::fs::write(lib.join("site-packages/fakedep.py"), "V = 1\n").unwrap();
    let launcher = venv.join("bin/pytest");
    std::fs::write(&launcher, format!("#!{}/bin/python3\nimport sys, pytest\nsys.exit(pytest.main())\n", venv.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();
    venv
}

#[test]
fn a_virtualenv_outside_the_repo_is_used_when_it_is_mounted_and_active() {
    let t = TestEnv::new("deps-venv");
    let venv = make_venv(&t);
    let repo = repo_needing_fakedep(&t);
    let t = t.with_policy_text(&policy_with(Some(&venv), &[]));
    let script = t.write_script("s.toml", STEPS);
    let out = run(&t, &repo, &script, Some(&venv.join("bin")));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    for call in [1, 2] {
        let (exit, text) = stdout_of(&t, &id, &events, call);
        assert_eq!(exit, Some(0), "call {call}:\n{text}");
        assert!(text.contains(&format!("PREFIX {}", venv.display())), "call {call} did not run inside the virtualenv:\n{text}");
        assert!(text.contains(&format!("FAKEDEP {}", venv.display())), "call {call}:\n{text}");
    }
    // `python3` is started through the virtualenv's link, not through the
    // system Python the link points at: only then does Python use the venv.
    let program = |call: u64| events.iter().find(|e| e["type"] == "tool.intent" && e["call_id"] == call).unwrap()["plan"]["program"].as_str().unwrap().to_string();
    assert_eq!(program(1), venv.join("bin/pytest").display().to_string());
    assert_eq!(program(2), venv.join("bin/python3").display().to_string());
}

#[test]
fn a_virtualenv_that_is_not_mounted_is_refused_at_startup_with_the_way_out() {
    let t = TestEnv::new("deps-venv-unmounted");
    let venv = make_venv(&t);
    let repo = repo_needing_fakedep(&t);
    let script = t.write_script("s.toml", STEPS);
    let out = run(&t, &repo, &script, Some(&venv.join("bin"))); // shipped policy: no extra mount
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    for needle in ["sandbox cannot see", "sandbox.ro_mounts", "virtualenv", venv.to_str().unwrap()] {
        assert!(err.contains(needle), "`{needle}` is missing from:\n{err}");
    }
    assert!(!t.state.join("executions").exists(), "nothing may be started");
}

#[test]
fn a_package_folder_is_reached_through_a_mount_and_a_policy_variable() {
    let t = TestEnv::new("deps-folder");
    let deps = t.root.join("deps");
    std::fs::create_dir(&deps).unwrap();
    std::fs::write(deps.join("fakedep.py"), "V = 1\n").unwrap();
    let repo = repo_needing_fakedep(&t);
    let t = t.with_policy_text(&policy_with(Some(&deps), &[("PYTHONPATH", deps.to_str().unwrap()), ("GREETING", "hello there")]));
    // A third step tries to change the package from inside the sandbox.
    let attack = format!(
        "[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\", \"-s\", \"tests/test_w.py\"]\n[[step]]\ntool = \"fs.write\"\npath = \"tests/test_w.py\"\ncontent = '''\ndef test_w():\n    try:\n        open(\"{0}/fakedep.py\", \"w\").write(\"V = 2\\n\")\n        print(\"WROTE\")\n    except OSError as e:\n        print(\"BLOCKED\", e.errno)\n    try:\n        open(\"{0}/new.py\", \"w\").write(\"x\")\n        print(\"WROTE\")\n    except OSError as e:\n        print(\"BLOCKED\", e.errno)\n'''\n[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\", \"-s\", \"tests/test_w.py\"]\n",
        deps.display()
    );
    let script = t.write_script("s.toml", &format!("{STEPS}{attack}"));
    let out = run(&t, &repo, &script, None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    for call in [1, 2] {
        let (exit, text) = stdout_of(&t, &id, &events, call);
        assert_eq!(exit, Some(0), "call {call}:\n{text}");
        assert!(text.contains(&format!("FAKEDEP {}", deps.display())), "call {call}:\n{text}");
        assert!(text.contains("GREETING hello there"), "call {call}:\n{text}");
    }
    let (_, text) = stdout_of(&t, &id, &events, 5);
    assert_eq!(text.matches("BLOCKED").count(), 2, "the mounted folder must be read-only:\n{text}");
    assert!(!text.contains("WROTE"), "{text}");
    assert_eq!(std::fs::read_to_string(deps.join("fakedep.py")).unwrap(), "V = 1\n");
    assert!(!deps.join("new.py").exists());
}

#[test]
fn without_the_mount_the_package_is_simply_not_there() {
    // The shipped policy: the test cannot import the package, and says so.
    let t = TestEnv::new("deps-absent");
    let repo = repo_needing_fakedep(&t);
    let script = t.write_script("s.toml", STEPS);
    let out = run(&t, &repo, &script, None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let (exit, text) = stdout_of(&t, &id, &t.events(&id), 1);
    assert_ne!(exit, Some(0));
    assert!(text.contains("No module named 'fakedep'"), "{text}");
}

#[test]
fn the_shipped_policies_leave_no_compiled_python_files_behind() {
    // `PYTHONDONTWRITEBYTECODE` in `[sandbox.env]`: without it every pytest
    // run adds .pyc files, which are binary and so cannot be in the patch.
    let t = TestEnv::new("deps-no-pyc");
    let id = t.run_script(repo_root().join("examples/scripts/fix-demo.toml").to_str().unwrap());
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    assert!(!patch.contains("Binary files") && !patch.contains("# agentos:"), "{patch}");
    assert!(patch.contains("b/src/stats.py"));
    let digest = tree_digest(&t.exec_dir(&id).join("work")).join("\n");
    assert!(!digest.contains("__pycache__"), "{digest}");
}

// ---------------------------------------------------------------------------
// M9: deny exceptions, skip, scratch, and the virtualenv note (D-099 to D-101).
// ---------------------------------------------------------------------------

#[test]
fn a_mounted_virtualenv_that_is_not_active_gets_a_note() {
    let t = TestEnv::new("deps-venv-inactive");
    let venv = make_venv(&t);
    let repo = repo_needing_fakedep(&t);
    let t = t.with_policy_text(&policy_with(Some(&venv), &[]));
    let script = t.write_script("s.toml", STEPS);
    let out = run(&t, &repo, &script, None); // mounted, but its bin is not on PATH
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "the note must not stop the run: {err}");
    for needle in ["note:", "activate it", venv.join("bin/pytest").to_str().unwrap()] {
        assert!(err.contains(needle), "`{needle}` is missing from:\n{err}");
    }
    // And with the virtualenv active there is nothing to say.
    let t2 = TestEnv::new("deps-venv-active-quiet");
    let venv2 = make_venv(&t2);
    let repo2 = repo_needing_fakedep(&t2);
    let t2 = t2.with_policy_text(&policy_with(Some(&venv2), &[]));
    let script2 = t2.write_script("s.toml", STEPS);
    let out2 = run(&t2, &repo2, &script2, Some(&venv2.join("bin")));
    assert!(!String::from_utf8_lossy(&out2.stderr).contains("note:"), "{}", String::from_utf8_lossy(&out2.stderr));
}

#[test]
fn a_second_copy_of_a_program_later_on_path_is_not_worth_a_note() {
    // B-014. Two mounted folders each hold a `pytest`; both `bin` folders
    // are on PATH and the first one wins. That is every Ubuntu machine with
    // an active virtualenv: `/usr` is a mount and `/usr/bin/pytest` exists.
    // On NixOS no mount has a `bin`, so only this test shows the case there.
    let t = TestEnv::new("deps-two-on-path");
    let active = make_venv(&t);
    let other = t.root.join("other-tools");
    assert!(std::process::Command::new("cp").arg("-a").arg(&active).arg(&other).status().unwrap().success());
    let repo = repo_needing_fakedep(&t);
    let mounted_twice = policy_with(Some(&active), &[]).replace("ro_mounts = [", &format!("ro_mounts = [\"{}\", ", other.display()));
    assert!(mounted_twice.contains(other.to_str().unwrap()));
    let t = t.with_policy_text(&mounted_twice);
    let script = t.write_script("s.toml", STEPS);
    let mut c = t.cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()]);
    c.env("PATH", format!("{}:{}:{}", active.join("bin").display(), other.join("bin").display(), std::env::var("PATH").unwrap()));
    let out = c.output().unwrap();
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{err}");
    assert!(!err.contains("note:"), "a program that is on PATH, only later, got a note:\n{err}");
    // The same two mounts with the second one *not* on PATH: now it is worth saying.
    let t3 = TestEnv::new("deps-two-one-off-path");
    let active3 = make_venv(&t3);
    let other3 = t3.root.join("other-tools");
    assert!(std::process::Command::new("cp").arg("-a").arg(&active3).arg(&other3).status().unwrap().success());
    let repo3 = repo_needing_fakedep(&t3);
    let t3 = t3.with_policy_text(&policy_with(Some(&active3), &[]).replace("ro_mounts = [", &format!("ro_mounts = [\"{}\", ", other3.display())));
    let script3 = t3.write_script("s.toml", STEPS);
    let out3 = run(&t3, &repo3, &script3, Some(&active3.join("bin")));
    let err3 = String::from_utf8_lossy(&out3.stderr);
    assert!(err3.contains("note:") && err3.contains(other3.to_str().unwrap()) && !err3.contains(&format!("has {},", active3.display())), "{err3}");
}

#[test]
fn a_virtualenv_inside_the_repo_is_skipped_in_the_copy_and_still_used() {
    let t = TestEnv::new("deps-skip");
    let repo = repo_needing_fakedep(&t);
    // The usual place: <repo>/.venv
    let made = make_venv(&t);
    let venv = repo.join(".venv");
    std::fs::remove_dir_all(&made).unwrap();
    let out = std::process::Command::new("python3").args(["-m", "venv", "--without-pip", "--system-site-packages"]).arg(&venv).output().unwrap();
    assert!(out.status.success());
    let lib = std::fs::read_dir(venv.join("lib")).unwrap().next().unwrap().unwrap().path();
    std::fs::write(lib.join("site-packages/fakedep.py"), "V = 1\n").unwrap();
    let launcher = venv.join("bin/pytest");
    std::fs::write(&launcher, format!("#!{}/bin/python3\nimport sys, pytest\nsys.exit(pytest.main())\n", venv.display())).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o755)).unwrap();

    let policy = policy_with(Some(&venv), &[]).replace("skip = []", "skip = [\".venv/**\"]");
    assert!(policy.contains("skip = [\".venv/**\"]"), "the preset has no skip line");
    let t = t.with_policy_text(&policy);
    let script = t.write_script("s.toml", STEPS);
    let out = run(&t, &repo, &script, Some(&venv.join("bin")));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let created = events.iter().find(|e| e["type"] == "workspace.created").unwrap();
    assert_eq!(created["skipped"], serde_json::json!([".venv"]));
    assert!(!t.exec_dir(&id).join("work/.venv").exists(), "the virtualenv was copied");
    assert!(!t.exec_dir(&id).join("checkpoints/cp-0000/.venv").exists());
    for call in [1, 2] {
        let (exit, text) = stdout_of(&t, &id, &events, call);
        assert_eq!(exit, Some(0), "call {call}:\n{text}");
        assert!(text.contains(&format!("FAKEDEP {}", venv.display())), "call {call}:\n{text}");
    }
    assert!(t.agentos(&["trace", &id]).contains("left out by skip rules: .venv"));
}

/// The demo repo with pytest's cache switched back on.
fn repo_with_pytest_cache(t: &TestEnv) -> PathBuf {
    let repo = t.root.join("repo");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    std::fs::write(repo.join("pytest.ini"), "[pytest]\npythonpath = src\ntestpaths = tests\n").unwrap();
    std::fs::write(repo.join(".env.example"), "TOKEN=put-yours-here\n").unwrap();
    repo
}

const CACHE_STEPS: &str = "task = \"cache\"\n\
    [[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\"]\n\
    [[step]]\ntool = \"fs.write\"\npath = \"src/extra.py\"\ncontent = \"x = 1\\n\"\n\
    [[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\"]\n\
    [[step]]\ntool = \"fs.read\"\npath = \".env.example\"\n\
    [[step]]\ntool = \"fs.read\"\npath = \".env\"\n\
    [[step]]\ntool = \"fs.write\"\npath = \".pytest_cache/mine\"\ncontent = \"x\"\n";

#[test]
fn a_cache_is_scratch_and_an_example_env_file_is_not_a_secret() {
    let t = TestEnv::new("deps-scratch");
    let repo = repo_with_pytest_cache(&t);
    let script = t.write_script("s.toml", CACHE_STEPS);
    let out = run(&t, &repo, &script, None); // the shipped policy
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let work = t.exec_dir(&id).join("work");
    assert!(work.join(".pytest_cache").is_dir(), "pytest wrote its cache, which is what this test is about");
    assert!(!events.iter().any(|e| e["type"] == "policy.violation"), "a cache is not a violation: {events:#?}");
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    assert!(patch.contains("b/src/extra.py"));
    assert!(!patch.contains("pytest_cache") && !patch.contains("# agentos:"), "the cache leaked into the patch:\n{patch}");
    // The template is readable; the real file is still denied and absent.
    let read = t.output_of(&id, &events, 4).expect(".env.example can be read");
    assert_eq!(read["content"], "TOKEN=put-yours-here\n");
    let denied = |call: u64| events.iter().find(|e| e["type"] == "tool.denied" && e["call_id"] == call).map(|e| e["rule"].to_string());
    assert!(denied(5).is_some_and(|r| r.contains("filesystem.deny")), "{:?}", denied(5));
    assert!(!work.join(".env").exists());
    // Scratch is for commands. The agent's own fs.write still needs a write glob.
    assert_eq!(denied(6).as_deref(), Some("\"no matching rule\""));
    // A cache change alone is not a change: the workspace still matches its checkpoint.
    let last = t.exec_dir(&id).join("checkpoints/cp-0003");
    assert_eq!(tree_digest(&work), tree_digest(&last));
}

#[test]
fn without_a_scratch_rule_the_cache_is_reverted_and_reported() {
    // The control for the test above: the same run with `scratch = []`.
    let t = TestEnv::new("deps-no-scratch");
    let repo = repo_with_pytest_cache(&t);
    let policy = std::fs::read_to_string(preset_policy()).unwrap().replace("scratch = [\".pytest_cache/**\"]", "scratch = []");
    assert!(policy.contains("scratch = []"));
    let t = t.with_policy_text(&policy);
    let script = t.write_script("s.toml", CACHE_STEPS);
    let out = run(&t, &repo, &script, None);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let violation = events.iter().find(|e| e["type"] == "policy.violation").expect("the cache is reverted without the rule");
    assert!(violation["reverted"].to_string().contains(".pytest_cache"), "{violation}");
    assert!(!t.exec_dir(&id).join("work/.pytest_cache").exists());
}

// ---------------------------------------------------------------------------
// D-112: helper programs (`exec.helpers`).
// ---------------------------------------------------------------------------

#[test]
fn a_helper_program_is_found_by_commands_and_cannot_be_called_by_the_agent() {
    // An allowed command (`python3 -m pytest`) starts another program by
    // name, the way `cargo test` starts `cc` and `npm test` starts `sh`.
    let make = |name: &str| {
        let t = TestEnv::new(name);
        let tools = t.root.join("tools");
        std::fs::create_dir_all(tools.join("bin")).unwrap();
        let greet = tools.join("bin/greet");
        // A script whose interpreter is named by its full path, so that it
        // needs nothing but itself to be found.
        let python = std::process::Command::new("python3").args(["-c", "import sys; print(sys.executable)"]).output().unwrap();
        let python = String::from_utf8(python.stdout).unwrap().trim().to_string();
        std::fs::write(&greet, format!("#!{python}\nprint('hello from the helper')\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&greet, std::fs::Permissions::from_mode(0o755)).unwrap();
        let repo = t.root.join("repo");
        assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
        std::fs::write(
            repo.join("tests/test_helper.py"),
            "import shutil, subprocess\n\n\ndef test_helper():\n    print('WHICH', shutil.which('greet'))\n    out = subprocess.run(['greet'], capture_output=True, text=True)\n    print('SAID', out.stdout.strip())\n    assert out.returncode == 0\n",
        )
        .unwrap();
        (t, tools, repo)
    };
    let steps = "task = \"helper\"\n\
        [[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-m\", \"pytest\", \"-q\", \"-s\", \"tests/test_helper.py\"]\n\
        [[step]]\ntool = \"exec\"\nargv = [\"greet\"]\n";

    // Named as a helper: the command finds it.
    let (t, tools, repo) = make("deps-helper");
    let policy = policy_with(Some(&tools), &[]).replace("helpers = []", "helpers = [\"greet\"]");
    assert!(policy.contains("helpers = [\"greet\"]"), "the preset has no helpers line");
    let t = t.with_policy_text(&policy);
    let script = t.write_script("s.toml", steps);
    let out = run(&t, &repo, &script, Some(&tools.join("bin")));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let (exit, text) = stdout_of(&t, &id, &events, 1);
    assert_eq!(exit, Some(0), "{text}");
    assert!(text.contains(&format!("WHICH {}", tools.join("bin/greet").display())) && text.contains("SAID hello from the helper"), "{text}");
    // The agent itself still may not run it: a helper is not a rule.
    let denied = events.iter().find(|e| e["type"] == "tool.denied" && e["call_id"] == 2).expect("call 2 is denied");
    assert_eq!(denied["rule"], "no matching rule");

    // Mounted and on the host's PATH, but not named: the command does not find it.
    let (t, tools, repo) = make("deps-helper-unnamed");
    let t = t.with_policy_text(&policy_with(Some(&tools), &[]));
    let script = t.write_script("s.toml", steps);
    let out = run(&t, &repo, &script, Some(&tools.join("bin")));
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let (exit, text) = stdout_of(&t, &id, &t.events(&id), 1);
    assert_ne!(exit, Some(0));
    assert!(text.contains("WHICH None"), "{text}");

    // A helper that is not on the host's PATH is a startup error that names it.
    let (t, tools, repo) = make("deps-helper-missing");
    let t = t.with_policy_text(&policy_with(Some(&tools), &[]).replace("helpers = []", "helpers = [\"greet\"]"));
    let script = t.write_script("s.toml", steps);
    let out = run(&t, &repo, &script, None);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && err.contains("helper program `greet` was not found on PATH"), "{err}");
    assert!(!t.state.join("executions").exists());
}
