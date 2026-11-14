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
