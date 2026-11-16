//! `filesystem.ro_dirs` (D-122): a folder of the repository that is not
//! copied. Commands see the original, read-only, at its place in the
//! workspace; `fs.read` and `fs.list` read it from the repository.
//!
//! What must hold: commands can use it and cannot change it; the original
//! repository is never modified (design rule 10); no checkpoint holds it; a
//! denied file in it stops the run before it starts (design rule 6); the
//! agent's file tools cannot write it or leave it; it is still there after
//! a kill and a resume; `agentos apply` leaves it alone.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test rodirs

mod common;

use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};

use common::*;
use serde_json::Value;

/// The preset policy with `python3` allowed, write access to `src/**` only
/// by default, and `vendor` as a read-only folder.
fn policy(extra_filesystem: &str) -> String {
    let base = std::fs::read_to_string(preset_policy()).unwrap();
    let mut out = String::new();
    let mut in_allow = false;
    for line in base.lines() {
        if line.starts_with("allow = [") {
            in_allow = true;
            out.push_str("allow = [[\"python3\"]]\n");
        } else if in_allow {
            in_allow = !line.starts_with(']');
        } else if line.starts_with("ro_dirs") {
            // The presets ship an empty list; this is the line under test.
            out.push_str("ro_dirs = [\"vendor\"]\n");
            out.push_str(extra_filesystem);
        } else {
            out.push_str(line);
            out.push('\n');
        }
    }
    assert!(out.contains("ro_dirs = [\"vendor\"]"), "the preset has no ro_dirs line to replace");
    out
}

/// A project whose code imports a package from `vendor/`, the way a Node
/// project needs `node_modules` at its place.
fn make_repo(t: &TestEnv) -> PathBuf {
    let repo = t.root.join("repo");
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::create_dir_all(repo.join("vendor/lib/deep")).unwrap();
    std::fs::write(repo.join("src/app.py"), "import sys\nsys.path.insert(0, 'vendor')\nimport lib\nprint('answer', lib.ANSWER)\n").unwrap();
    std::fs::write(repo.join("vendor/lib/__init__.py"), "ANSWER = 42\n").unwrap();
    std::fs::write(repo.join("vendor/lib/deep/data.txt"), "vendored data\n").unwrap();
    std::fs::write(repo.join("vendor/big.bin"), vec![7u8; 300_000]).unwrap();
    repo
}

fn exec(code: &str) -> String {
    assert!(!code.contains("'''"));
    format!("[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-B\", \"-c\", '''\nimport os, sys\n{code}\n''']\n")
}

fn run(t: &TestEnv, repo: &Path, steps: &str) -> (String, Vec<Value>) {
    let script = t.write_script("s.toml", &format!("task = \"ro dirs\"\n{steps}"));
    let id = t.run_script_on(script.to_str().unwrap(), repo);
    let events = t.events(&id);
    (id, events)
}

fn rule(events: &[Value], call: u64) -> String {
    events
        .iter()
        .find(|e| e["type"] == "tool.denied" && e["call_id"] == call)
        .unwrap_or_else(|| panic!("call {call} was not denied: {events:#?}"))["rule"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn commands_use_the_folder_and_cannot_change_it() {
    let t = TestEnv::new("ro-use").with_policy_text(&policy(""));
    let repo = make_repo(&t);
    let before = tree_digest(&repo);
    let attack = r#"
def attempt(name, fn):
    try:
        fn(); print("CHANGED:", name)
    except OSError as e:
        print("refused:", name, e.strerror)
attempt("overwrite", lambda: open("vendor/lib/__init__.py", "w").write("ANSWER = 0\n"))
attempt("append", lambda: open("vendor/lib/deep/data.txt", "a").write("x"))
attempt("create", lambda: open("vendor/new.py", "w").write("x"))
attempt("delete", lambda: os.remove("vendor/big.bin"))
attempt("chmod", lambda: os.chmod("vendor/lib/__init__.py", 0o777))
attempt("mkdir", lambda: os.mkdir("vendor/newdir"))
attempt("rename inside", lambda: os.rename("vendor/lib", "vendor/lib2"))
attempt("rename the folder", lambda: os.rename("vendor", "vendor2"))
attempt("rmdir the folder", lambda: os.rmdir("vendor"))
import ctypes
libc = ctypes.CDLL(None, use_errno=True)
def umount():
    if libc.umount2(b"/work/vendor", 2) != 0:
        raise OSError(ctypes.get_errno(), os.strerror(ctypes.get_errno()))
attempt("umount", umount)
print("size seen", os.path.getsize("vendor/big.bin"))
"#;
    let steps = [
        exec("exec(open('src/app.py').read())"), // 1: the project runs with its vendored package
        exec(attack),                            // 2
        "[[step]]\ntool = \"fs.write\"\npath = \"src/new.py\"\ncontent = \"x = 1\\n\"\n".to_string(), // 3: ordinary work goes on
        exec("exec(open('src/app.py').read())"), // 4: still there after other calls
    ]
    .concat();
    let (id, events) = run(&t, &repo, &steps);
    let out = |call: u64| t.output_of(&id, &events, call).unwrap_or_else(|| panic!("call {call} did not complete: {events:#?}"));
    assert_eq!(out(1)["stdout"].as_str().unwrap().trim(), "answer 42");
    let attack_out = out(2)["stdout"].as_str().unwrap().to_string();
    assert!(!attack_out.contains("CHANGED"), "{attack_out}\n{}", out(2)["stderr"]);
    assert_eq!(attack_out.matches("refused:").count(), 10, "{attack_out}");
    assert!(attack_out.contains("size seen 300000"));
    assert_eq!(out(4)["stdout"].as_str().unwrap().trim(), "answer 42");
    assert!(!events.iter().any(|e| e["type"] == "policy.violation"), "{events:#?}");

    // Design rule 10: the original repository is exactly as it was.
    assert_eq!(tree_digest(&repo), before);
    // The workspace and every checkpoint hold an empty folder there, not
    // the 300 KB the folder has: it is never copied.
    let dir = t.exec_dir(&id);
    assert_eq!(std::fs::read_dir(dir.join("work/vendor")).unwrap().count(), 0);
    let mut checkpoints = 0;
    for cp in std::fs::read_dir(dir.join("checkpoints")).unwrap() {
        let cp = cp.unwrap().path();
        assert_eq!(std::fs::read_dir(cp.join("vendor")).unwrap().count(), 0, "{} holds the read-only folder's files", cp.display());
        checkpoints += 1;
    }
    assert_eq!(checkpoints, 5, "cp-0000 and one per mutating call");
    // The patch holds the agent's own change and nothing about the folder.
    let patch = std::fs::read_to_string(dir.join("result.patch")).unwrap();
    assert!(patch.contains("src/new.py") && !patch.contains("vendor"), "{patch}");
    // `agentos apply` copies that change back and leaves the folder alone.
    let shown = t.agentos(&["apply", &id]);
    assert!(shown.contains("A src/new.py") && !shown.contains("vendor"), "{shown}");
    assert!(repo.join("src/new.py").exists());
    assert_eq!(std::fs::read_to_string(repo.join("vendor/lib/__init__.py")).unwrap(), "ANSWER = 42\n");
    assert_eq!(std::fs::read(repo.join("vendor/big.bin")).unwrap().len(), 300_000);
}

#[test]
fn the_file_tools_read_it_from_the_repository_and_never_write_it() {
    // Write globs that cover the folder, to show ro_dirs wins over them.
    let text = policy("").replace("write = [\"src/**\"", "write = [\"vendor/**\", \"src/**\"");
    assert!(text.contains("\"vendor/**\""));
    let t = TestEnv::new("ro-tools").with_policy_text(&text);
    let repo = make_repo(&t);
    std::fs::write(repo.join("src/secret.txt"), "REPO-ONLY\n").unwrap();
    std::os::unix::fs::symlink("../src/secret.txt", repo.join("vendor/out")).unwrap();
    std::os::unix::fs::symlink("/etc/hostname", repo.join("vendor/abs")).unwrap();
    let before = tree_digest(&repo);
    let steps = [
        "[[step]]\ntool = \"fs.read\"\npath = \"vendor/lib/deep/data.txt\"\n",                              // 1
        "[[step]]\ntool = \"fs.list\"\npath = \"vendor/lib\"\n",                                            // 2
        "[[step]]\ntool = \"fs.write\"\npath = \"vendor/lib/__init__.py\"\ncontent = \"ANSWER = 0\\n\"\n",    // 3: denied
        "[[step]]\ntool = \"fs.edit\"\npath = \"vendor/lib/__init__.py\"\nold_str = \"42\"\nnew_str = \"0\"\n", // 4: denied
        "[[step]]\ntool = \"fs.write\"\npath = \"vendor/new.py\"\ncontent = \"x\"\n",                         // 5: denied
        "[[step]]\ntool = \"fs.read\"\npath = \"vendor/out\"\n",                                             // 6: a link out of the folder
        "[[step]]\ntool = \"fs.read\"\npath = \"vendor/abs\"\n",                                             // 7: a link out of the repository
        "[[step]]\ntool = \"fs.read\"\npath = \"vendor/../src/app.py\"\n",                                   // 8: not in the folder: the workspace's file
        "[[step]]\ntool = \"fs.list\"\npath = \".\"\n",                                                      // 9
    ]
    .concat();
    let (id, events) = run(&t, &repo, &steps);
    assert_eq!(t.output_of(&id, &events, 1).unwrap()["content"], "vendored data\n");
    let names: Vec<String> = t.output_of(&id, &events, 2).unwrap()["entries"].as_array().unwrap().iter().map(|e| e["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, ["__init__.py", "deep"]);
    for call in [3, 4, 5] {
        assert!(rule(&events, call).contains("filesystem.ro_dirs `vendor` (read-only)"), "call {call}: {}", rule(&events, call));
    }
    assert!(rule(&events, 6).contains("leaves the read-only folder"), "{}", rule(&events, 6));
    assert!(rule(&events, 7).contains("leaves the workspace"), "{}", rule(&events, 7));
    assert!(t.output_of(&id, &events, 8).unwrap()["content"].as_str().unwrap().contains("import lib"));
    assert!(t.output_of(&id, &events, 9).unwrap()["entries"].as_array().unwrap().iter().any(|e| e["name"] == "vendor" && e["kind"] == "dir"));
    assert_eq!(tree_digest(&repo), before, "the original repository is untouched");
    assert_eq!(std::fs::read_dir(t.exec_dir(&id).join("work/vendor")).unwrap().count(), 0, "nothing was written under the mount point");
}

#[test]
fn a_denied_file_in_the_folder_stops_the_run_before_it_starts() {
    let t = TestEnv::new("ro-deny").with_policy_text(&policy(""));
    let repo = make_repo(&t);
    std::fs::write(repo.join("vendor/lib/.env"), "TOKEN=in-vendor\n").unwrap();
    let script = t.write_script("s.toml", &format!("task = \"x\"\n{}", exec("print(open('vendor/lib/.env').read())")));
    let args = ["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--agent", "scripted", "--script", script.to_str().unwrap()];
    let out = t.output(&args);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success() && err.contains("vendor/lib/.env") && err.contains("deny rule"), "{err}");
    let check = t.output(&["check", "--policy", t.policy.to_str().unwrap(), "--repo", repo.to_str().unwrap()]);
    let shown = String::from_utf8_lossy(&check.stdout).to_string() + &String::from_utf8_lossy(&check.stderr);
    assert!(!check.status.success() && shown.contains("vendor/lib/.env"), "{shown}");
    // Without the denied file, check says what will happen.
    std::fs::remove_file(repo.join("vendor/lib/.env")).unwrap();
    let check = t.output(&["check", "--policy", t.policy.to_str().unwrap(), "--repo", repo.to_str().unwrap()]);
    let shown = String::from_utf8_lossy(&check.stdout).to_string();
    assert!(check.status.success() && shown.contains("shown to commands read-only from the repository: vendor"), "{shown}\n{}", String::from_utf8_lossy(&check.stderr));
}

#[test]
fn a_folder_that_is_missing_or_a_symlink_is_refused() {
    for (name, prepare) in [
        ("ro-missing", (|repo: &Path| std::fs::remove_dir_all(repo.join("vendor")).unwrap()) as fn(&Path)),
        ("ro-symlink", |repo: &Path| {
            std::fs::rename(repo.join("vendor"), repo.join("elsewhere")).unwrap();
            std::os::unix::fs::symlink("elsewhere", repo.join("vendor")).unwrap();
        }),
    ] {
        let t = TestEnv::new(name).with_policy_text(&policy(""));
        let repo = make_repo(&t);
        prepare(&repo);
        let script = t.write_script("s.toml", "task = \"x\"\n[[step]]\ntool = \"fs.list\"\npath = \".\"\n");
        let out = t.output(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--agent", "scripted", "--script", script.to_str().unwrap()]);
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(!out.status.success() && err.contains("filesystem.ro_dirs entry `vendor`"), "{name}: {err}");
    }
}

#[test]
fn a_command_that_swaps_the_way_to_the_mount_point_is_reverted() {
    // Even with write globs that cover the folder's parent, a read-only
    // folder is never a writable path, so the revert after the command puts
    // the mount point back and reports it. The next command finds the folder.
    let text = policy("").replace("write = [\"src/**\"", "write = [\"sub/**\", \"src/**\"").replace("ro_dirs = [\"vendor\"]", "ro_dirs = [\"sub/vendor\"]");
    let t = TestEnv::new("ro-swap").with_policy_text(&text);
    let repo = t.root.join("repo");
    std::fs::create_dir_all(repo.join("sub/vendor")).unwrap();
    std::fs::create_dir_all(repo.join("src")).unwrap();
    std::fs::write(repo.join("sub/vendor/data.txt"), "vendored\n").unwrap();
    let show = "print(open('sub/vendor/data.txt').read().strip())";
    let steps = [exec(show), exec("os.rename('sub', 'sub-old'); os.mkdir('sub'); os.symlink('/tmp', 'sub/vendor')"), exec(show)].concat();
    let (id, events) = run(&t, &repo, &steps);
    assert_eq!(t.output_of(&id, &events, 1).unwrap()["stdout"].as_str().unwrap().trim(), "vendored");
    let violation = events.iter().find(|e| e["type"] == "policy.violation" && e["call_id"] == 2).unwrap_or_else(|| panic!("the swap must be reported: {events:#?}"));
    assert!(violation["reverted"].as_array().unwrap().iter().any(|p| p == "sub/vendor"), "{violation}");
    assert_eq!(t.output_of(&id, &events, 3).unwrap()["stdout"].as_str().unwrap().trim(), "vendored");
    let mount_point = std::fs::symlink_metadata(t.exec_dir(&id).join("work/sub/vendor")).unwrap();
    assert!(mount_point.is_dir(), "the mount point is a plain folder again");
}

#[test]
fn the_sandbox_refuses_a_mount_point_that_is_a_symlink() {
    // The second line of defence, which the gateway's revert normally keeps
    // from being reached: the sandbox itself checks every folder on the way
    // to a mount point. Driven through the library, with a workspace that
    // has a symlink where the mount point should be.
    use agentos::policy::Policy;
    use agentos::sandbox::{self, cgroup::CgroupRoot, DiskLimit, SandboxSpec};
    use agentos::tools::exec::Programs;
    let t = TestEnv::new("ro-ns");
    let policy = Policy::parse(&policy("")).unwrap();
    let programs = Programs::resolve(&policy, &std::env::var("PATH").unwrap_or_default()).unwrap();
    let cgroups = CgroupRoot::init().unwrap();
    let repo = make_repo(&t);
    let outside = t.root.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let attempt = |name: &str, prepare: &dyn Fn(&Path)| {
        let work = t.root.join(name);
        std::fs::create_dir_all(&work).unwrap();
        prepare(&work);
        let work = std::fs::canonicalize(&work).unwrap();
        let spec = SandboxSpec {
            workspace: work.clone(),
            ro_mounts: policy.sandbox.ro_mounts.iter().map(PathBuf::from).collect(),
            program: programs.lookup("python3").unwrap().to_path_buf(),
            argv: ["python3", "-B", "-c", "import os; print(sorted(os.listdir('vendor')))"].iter().map(|s| s.to_string()).collect(),
            env: vec![("PATH".into(), programs.search_path()), ("HOME".into(), "/work".into())],
            cgroup: cgroups.path_of(&format!("ro-ns-{}-{name}", std::process::id())),
            parent_pid: std::process::id() as i32,
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
            max_file_bytes: policy.limits.workspace_bytes(),
            ro_dirs: vec![(repo.join("vendor"), "vendor".into())],
        };
        let cg = cgroups.create(&format!("ro-ns-{}-{name}", std::process::id()), &policy.limits).unwrap();
        let disk = DiskLimit { dir: work, max_bytes: policy.limits.workspace_bytes() };
        sandbox::run(&agentos_bin(), &spec, &cg, std::time::Duration::from_secs(20), 4096, &disk)
    };
    // The control: a plain folder as the mount point works.
    let (out, _) = attempt("good", &|w| std::fs::create_dir(w.join("vendor")).unwrap()).unwrap();
    assert_eq!(out.stdout.trim(), "['big.bin', 'lib']", "{}", out.stderr);
    // A symlink there, or no folder at all: the command is not started.
    let outside2 = outside.clone();
    let err = attempt("link", &move |w| std::os::unix::fs::symlink(&outside2, w.join("vendor")).unwrap()).unwrap_err();
    assert!(err.0.contains("mount point for read-only folder `vendor` is not a plain folder"), "{}", err.0);
    let err = attempt("none", &|_| ()).unwrap_err();
    assert!(err.0.contains("mount point for read-only folder `vendor` is missing"), "{}", err.0);
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "nothing was mounted or written through the link");
}

#[test]
fn the_folder_is_still_there_after_a_kill_and_a_resume() {
    let t = TestEnv::new("ro-resume").with_policy_text(&policy(""));
    let repo = make_repo(&t);
    let steps = [exec("exec(open('src/app.py').read())"), exec("exec(open('src/app.py').read())"), "[[step]]\ntool = \"fs.read\"\npath = \"vendor/lib/deep/data.txt\"\n".to_string()].concat();
    let script = t.write_script("s.toml", &format!("task = \"resume\"\n{steps}"));
    let killed = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--agent", "scripted", "--script", script.to_str().unwrap()])
        .env("AGENTOS_CRASH_AT", "P2")
        .env("AGENTOS_CRASH_CALL", "2")
        .output()
        .unwrap();
    assert_eq!(killed.status.signal(), Some(9), "{}", String::from_utf8_lossy(&killed.stderr));
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let out = t.output(&["run", "--quiet", "--resume", &id]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let events = t.events(&id);
    assert!(events.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"));
    let last_exec = events.iter().filter(|e| e["type"] == "tool.completed").filter_map(|e| t.output_of(&id, &events, e["call_id"].as_u64().unwrap())).rfind(|o| o["kind"] == "exec").unwrap();
    assert_eq!(last_exec["stdout"].as_str().unwrap().trim(), "answer 42", "the command after the resume sees the folder");
    let read = events.iter().filter(|e| e["type"] == "tool.completed").filter_map(|e| t.output_of(&id, &events, e["call_id"].as_u64().unwrap())).find(|o| o["kind"] == "read").unwrap();
    assert_eq!(read["content"], "vendored data\n");
}
