//! Adversarial security suite (v1 "Evaluation", suite 2).
//!
//! Every test plays the hostile agent. Attacks come in two ways:
//! - through the gateway's file tools (`fs.read`, `fs.write`, `fs.list`), and
//! - as code run by an allowed command. The policy here allows `python3`,
//!   which gives the attacker exactly what `pytest` gives it (pytest runs the
//!   repo's `conftest.py`, so any allowed test command is arbitrary code
//!   execution; see the v1 threat model). A few tests use the realistic
//!   `pytest` + `conftest.py` path too.
//!
//! Attack code prints `ESCAPED: ...` when it gets something it must not, and
//! `BLOCKED: ...` when the sandbox stopped it. A test fails on any ESCAPED,
//! and also when an expected BLOCKED line is missing (so an attack that
//! crashed early cannot pass by accident).
//!
//! Must run under a delegated cgroup:
//!   systemd-run --user --scope -p Delegate=yes cargo test --test security

#[path = "../common/mod.rs"]
mod common;

use std::path::{Path, PathBuf};

use common::*;
use serde_json::Value;

/// The preset policy with `python3` allowed and the given limits.
fn policy_text(memory_mb: u64, pids: u64, timeout_s: u64) -> String {
    let base = std::fs::read_to_string(preset_policy()).unwrap();
    let mut out = String::new();
    let mut in_allow = false;
    for line in base.lines() {
        if line.starts_with("allow = [") {
            in_allow = true;
            out.push_str("allow = [[\"pytest\"], [\"python3\"]]\n");
            continue;
        }
        if in_allow {
            if line.starts_with(']') {
                in_allow = false;
            }
            continue;
        }
        let line = if line.starts_with("memory_mb") {
            format!("memory_mb      = {memory_mb}")
        } else if line.starts_with("pids") {
            format!("pids           = {pids}")
        } else if line.starts_with("call_timeout_s") {
            format!("call_timeout_s = {timeout_s}")
        } else {
            line.to_string()
        };
        out.push_str(&line);
        out.push('\n');
    }
    out
}

fn env(name: &str) -> TestEnv {
    TestEnv::new(name).with_policy_text(&policy_text(256, 64, 20))
}

/// A small repo for attacks: the demo files plus whatever the test adds.
fn make_repo(t: &TestEnv) -> PathBuf {
    let repo = t.root.join("repo");
    copy_dir(&repo_root().join("examples/demo-repo"), &repo);
    repo
}

fn copy_dir(from: &Path, to: &Path) {
    let st = std::process::Command::new("cp").arg("-a").arg(from).arg(to).status().unwrap();
    assert!(st.success());
}

struct Run {
    id: String,
    events: Vec<Value>,
}

impl Run {
    fn denied_rule(&self, call: u64) -> Option<String> {
        self.events
            .iter()
            .find(|e| e["type"] == "tool.denied" && e["call_id"] == call)
            .map(|e| e["rule"].as_str().unwrap().to_string())
    }
    fn failed(&self, call: u64) -> Option<String> {
        self.events
            .iter()
            .find(|e| e["type"] == "tool.failed" && e["call_id"] == call)
            .map(|e| e["error"].as_str().unwrap().to_string())
    }
}

fn run_steps(t: &TestEnv, repo: &Path, steps: &str) -> Run {
    let script = t.write_script("attack.toml", &format!("task = \"attack\"\n{steps}"));
    let id = t.run_script_on(script.to_str().unwrap(), repo);
    let events = t.events(&id);
    Run { id, events }
}

/// Run Python attack code as one `exec` call and return its output.
fn attack(t: &TestEnv, repo: &Path, code: &str) -> (Run, Value) {
    assert!(!code.contains("'''"));
    let steps = format!("[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", '''\nimport os, sys\n{code}\n''']\n");
    let run = run_steps(t, repo, &steps);
    let out = t.output_of(&run.id, &run.events, 1).unwrap_or_else(|| panic!("exec did not complete: {:?}", run.events));
    (run, out)
}

/// The attack's output must show every expected BLOCKED line and no ESCAPED.
fn assert_blocked(out: &Value, expect: &[&str]) {
    let stdout = out["stdout"].as_str().unwrap_or("");
    let stderr = out["stderr"].as_str().unwrap_or("");
    assert!(!stdout.contains("ESCAPED") && !stderr.contains("ESCAPED"), "ESCAPE:\nstdout:\n{stdout}\nstderr:\n{stderr}");
    for e in expect {
        assert!(stdout.contains(&format!("BLOCKED: {e}")), "missing `BLOCKED: {e}`\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }
}

/// Python helper prepended to attack code: `probe(name, fn)` runs fn and
/// prints ESCAPED if it succeeds, BLOCKED if it raises.
const PROBE: &str = r#"
import os, sys, socket, json
def probe(name, fn):
    try:
        r = fn()
        print(f"ESCAPED: {name}: {r!r}"[:300])
    except Exception as e:
        print(f"BLOCKED: {name} ({type(e).__name__})")
"#;

fn py(body: &str) -> String {
    format!("{PROBE}\n{body}")
}

/// No byte of `needle` may appear anywhere in the execution's folder (log,
/// blobs, checkpoints, workspace, patch).
fn assert_nowhere(t: &TestEnv, id: &str, needle: &str) {
    let out = std::process::Command::new("grep").args(["-r", "-l", "-a", needle]).arg(t.exec_dir(id)).output().unwrap();
    assert!(out.stdout.is_empty(), "`{needle}` found in: {}", String::from_utf8_lossy(&out.stdout));
}

// ===========================================================================
// 1. File tools: path traversal, absolute paths, symlinks (design rule 7)
// ===========================================================================

#[test]
fn fs_read_dotdot_traversal_is_denied() {
    let t = env("sec-dotdot");
    let repo = make_repo(&t);
    let r = run_steps(&t, &repo, "[[step]]\ntool = \"fs.read\"\npath = \"../../../../../../etc/passwd\"\n[[step]]\ntool = \"fs.read\"\npath = \"src/../../policy.toml\"\n");
    assert!(r.denied_rule(1).unwrap().contains("leaves the workspace"));
    assert!(r.denied_rule(2).unwrap().contains("leaves the workspace"));
}

#[test]
fn fs_read_absolute_path_is_denied() {
    let t = env("sec-abs");
    let repo = make_repo(&t);
    let r = run_steps(&t, &repo, "[[step]]\ntool = \"fs.read\"\npath = \"/etc/passwd\"\n[[step]]\ntool = \"fs.list\"\npath = \"/\"\n");
    assert!(r.denied_rule(1).unwrap().contains("leaves the workspace"));
    assert!(r.denied_rule(2).unwrap().contains("leaves the workspace"));
}

#[test]
fn fs_read_through_symlinks_pointing_outside_is_denied() {
    let t = env("sec-symlink");
    let repo = make_repo(&t);
    let secret = t.root.join("outside-secret.txt");
    std::fs::write(&secret, "OUTSIDE-CANARY-1\n").unwrap();
    std::os::unix::fs::symlink(&secret, repo.join("abs-link")).unwrap();
    std::os::unix::fs::symlink("../outside-secret.txt", repo.join("rel-link")).unwrap();
    std::os::unix::fs::symlink("/", repo.join("root-link")).unwrap();
    let r = run_steps(
        &t,
        &repo,
        "[[step]]\ntool = \"fs.read\"\npath = \"abs-link\"\n[[step]]\ntool = \"fs.read\"\npath = \"rel-link\"\n\
         [[step]]\ntool = \"fs.list\"\npath = \"root-link\"\n[[step]]\ntool = \"fs.read\"\npath = \"root-link/etc/passwd\"\n",
    );
    for c in 1..=4 {
        assert!(r.denied_rule(c).unwrap_or_default().contains("leaves the workspace"), "call {c}: {:?}", r.events);
    }
    assert_nowhere(&t, &r.id, "OUTSIDE-CANARY-1");
}

#[test]
fn fs_read_proc_magic_link_is_denied() {
    let t = env("sec-magic");
    let repo = make_repo(&t);
    std::os::unix::fs::symlink("/proc/self/root/etc/passwd", repo.join("magic")).unwrap();
    let r = run_steps(&t, &repo, "[[step]]\ntool = \"fs.read\"\npath = \"magic\"\n");
    assert!(r.denied_rule(1).unwrap().contains("leaves the workspace"));
}

#[test]
fn fs_write_through_symlinked_folder_is_denied() {
    let t = env("sec-write-dirlink");
    let repo = make_repo(&t);
    let outside = t.root.join("outside-dir");
    std::fs::create_dir(&outside).unwrap();
    // `src/out` is inside the write globs by name, but leads outside.
    std::os::unix::fs::symlink(&outside, repo.join("src/out")).unwrap();
    let r = run_steps(&t, &repo, "[[step]]\ntool = \"fs.write\"\npath = \"src/out/pwned.py\"\ncontent = \"x\"\n");
    assert!(r.denied_rule(1).unwrap().contains("leaves the workspace"));
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0, "nothing was written outside");
}

#[test]
fn fs_write_over_a_symlink_does_not_follow_it() {
    let t = env("sec-write-filelink");
    let repo = make_repo(&t);
    let victim = t.root.join("victim.txt");
    std::fs::write(&victim, "original\n").unwrap();
    std::os::unix::fs::symlink(&victim, repo.join("src/evil.py")).unwrap();
    let r = run_steps(&t, &repo, "[[step]]\ntool = \"fs.write\"\npath = \"src/evil.py\"\ncontent = \"pwned\"\n");
    assert!(r.failed(1).unwrap().contains("symlink"), "{:?}", r.events);
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "original\n");
}

#[test]
fn fs_tools_cannot_reach_log_or_checkpoints() {
    let t = env("sec-fs-log");
    let repo = make_repo(&t);
    let r = run_steps(
        &t,
        &repo,
        "[[step]]\ntool = \"fs.read\"\npath = \"../log.db\"\n[[step]]\ntool = \"fs.list\"\npath = \"../checkpoints\"\n\
         [[step]]\ntool = \"fs.write\"\npath = \"../checkpoints/cp-0000/src/stats.py\"\ncontent = \"x\"\n",
    );
    for c in 1..=3 {
        assert!(r.denied_rule(c).unwrap_or_default().contains("leaves the workspace"), "call {c}: {:?}", r.events);
    }
}

// ===========================================================================
// 2. Denied files and host files, read by code run by an allowed command
// ===========================================================================

#[test]
fn exec_cannot_read_a_denied_file() {
    let t = env("sec-denied-exec");
    let repo = make_repo(&t);
    let host_env = repo.join(".env");
    let code = py(&format!(
        "probe('read .env in workspace', lambda: open('.env').read())\n\
         probe('read .env by host path', lambda: open({host:?}).read())\n\
         probe('find .env anywhere in /work', lambda: [p for p in os.popen('ls -a /work').read().split() if p == '.env'] or (_ for _ in ()).throw(FileNotFoundError()))",
        host = host_env.display().to_string()
    ));
    let (r, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["read .env in workspace", "read .env by host path", "find .env anywhere in /work"]);
    assert_nowhere(&t, &r.id, "norn-demo-canary-7f3a91");
}

#[test]
fn conftest_cannot_read_a_denied_file() {
    // The realistic path: an allowed `pytest` runs the repo's conftest.py.
    let t = env("sec-conftest");
    let repo = make_repo(&t);
    std::fs::write(
        repo.join("tests/conftest.py"),
        format!(
            "import os\nfor p in ['/work/.env', {:?}, os.path.expanduser('~/.ssh')]:\n    try:\n        open(p).read(); print('ESCAPED:', p)\n    except Exception as e:\n        print('BLOCKED:', p, type(e).__name__)\n",
            repo.join(".env").display().to_string()
        ),
    )
    .unwrap();
    let r = run_steps(&t, &repo, "[[step]]\ntool = \"exec\"\nargv = [\"pytest\", \"-q\", \"-s\"]\n");
    let out = t.output_of(&r.id, &r.events, 1).unwrap();
    let stdout = out["stdout"].as_str().unwrap();
    assert!(!stdout.contains("ESCAPED"), "{stdout}");
    assert_eq!(stdout.matches("BLOCKED:").count(), 3, "{stdout}");
    assert_nowhere(&t, &r.id, "norn-demo-canary-7f3a91");
}

#[test]
fn exec_cannot_read_host_home_or_original_repo() {
    let t = env("sec-home");
    let repo = make_repo(&t);
    let home = std::env::var("HOME").unwrap();
    let code = py(&format!(
        "probe('list host home', lambda: os.listdir({home:?}))\n\
         probe('read ~/.ssh', lambda: os.listdir({ssh:?}))\n\
         probe('list /home', lambda: os.listdir('/home'))\n\
         probe('read original repo', lambda: open({orig:?}).read())\n\
         probe('read /etc/passwd', lambda: open('/etc/passwd').read())\n\
         probe('read /etc/shadow', lambda: open('/etc/shadow').read())",
        ssh = format!("{home}/.ssh"),
        orig = repo.join("src/stats.py").display().to_string()
    ));
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["list host home", "read ~/.ssh", "list /home", "read original repo", "read /etc/passwd", "read /etc/shadow"]);
}

#[test]
fn exec_cannot_read_or_change_the_event_log_and_checkpoints() {
    let t = env("sec-log-exec");
    let repo = make_repo(&t);
    // The attacker is told exactly where the log lives on the host.
    let state = t.state.display().to_string();
    let code = py(&format!(
        "import glob\n\
         probe('glob the state dir', lambda: glob.glob({state:?} + '/executions/*/log.db') or (_ for _ in ()).throw(FileNotFoundError()))\n\
         probe('list ../', lambda: os.listdir('/work/..') if 'log.db' in os.listdir('/work/..') else (_ for _ in ()).throw(FileNotFoundError()))\n\
         probe('write log', lambda: open({state:?} + '/x', 'w'))"
    ));
    let (r, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["glob the state dir", "list ../", "write log"]);
    // The log still reads back cleanly and in order.
    let seqs: Vec<u64> = r.events.iter().map(|e| e["seq"].as_u64().unwrap()).collect();
    assert_eq!(seqs, (1..=seqs.len() as u64).collect::<Vec<_>>());
}

#[test]
fn exec_sees_no_host_secrets_in_environment_or_proc() {
    let t = env("sec-env");
    let repo = make_repo(&t);
    let code = py(
        "probe('env canary', lambda: os.environ['NORN_TEST_SECRET'])\n\
         probe('canary in /proc/*/environ', lambda: [p for p in os.listdir('/proc') if p.isdigit() and b'CANARY' in open(f'/proc/{p}/environ','rb').read()] or (_ for _ in ()).throw(LookupError()))\n\
         probe('host processes visible', lambda: [p for p in os.listdir('/proc') if p.isdigit() and int(p) > 2] or (_ for _ in ()).throw(LookupError()))\n\
         print('env keys:', sorted(os.environ))",
    );
    // Run the runtime with a secret in its own environment.
    let script = t.write_script("env.toml", &format!("task = \"t\"\n[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", '''\n{code}\n''']\n"));
    let out = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .env("NORN_TEST_SECRET", "SECRET-CANARY-42")
        .output()
        .unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let o = t.output_of(&id, &events, 1).unwrap();
    assert_blocked(&o, &["env canary", "canary in /proc/*/environ", "host processes visible"]);
    let stdout = o["stdout"].as_str().unwrap();
    assert!(!stdout.contains("NORN_TEST_SECRET"));
    assert_nowhere(&t, &id, "SECRET-CANARY-42");
}

// ===========================================================================
// 3. Writing outside the write globs from code (D-009)
// ===========================================================================

#[test]
fn exec_changes_outside_write_globs_are_reverted() {
    let t = env("sec-write-globs");
    let repo = make_repo(&t);
    // A folder outside the write globs, to check that a mode change on it is
    // undone. (`src` itself would not do: `src/**` matches the folder `src`
    // too, so changing its mode is an allowed change. See D-032.)
    std::fs::create_dir(repo.join("docs")).unwrap();
    std::fs::write(repo.join("docs/guide.md"), "guide\n").unwrap();
    let code = "open('pytest.ini','w').write('[pytest]\\naddopts = -p evil\\n')\n\
                open('README.md','a').write('injected\\n')\n\
                os.makedirs('newdir/sub'); open('newdir/sub/x.txt','w').write('x')\n\
                open('.env','w').write('TOKEN=planted\\n')\n\
                os.chmod('docs', 0o777)\n\
                open('src/allowed.py','w').write('ok = 1\\n')\n\
                print('done')";
    let (r, out) = attack(&t, &repo, code);
    assert_eq!(out["exit_code"], 0, "{out}");
    let work = t.exec_dir(&r.id).join("work");
    assert_eq!(std::fs::read(work.join("pytest.ini")).unwrap(), std::fs::read(repo.join("pytest.ini")).unwrap());
    assert_eq!(std::fs::read(work.join("README.md")).unwrap(), std::fs::read(repo.join("README.md")).unwrap());
    assert!(!work.join("newdir").exists());
    assert!(!work.join(".env").exists(), "a denied path written by code is removed");
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(work.join("docs")).unwrap().permissions().mode() & 0o777,
        std::fs::metadata(repo.join("docs")).unwrap().permissions().mode() & 0o777
    );
    assert_eq!(std::fs::read_to_string(work.join("src/allowed.py")).unwrap(), "ok = 1\n", "allowed writes stay");
    let v = r.events.iter().find(|e| e["type"] == "policy.violation").expect("a policy.violation event");
    let reverted: Vec<&str> = v["reverted"].as_array().unwrap().iter().map(|x| x.as_str().unwrap()).collect();
    for p in [".env", "README.md", "docs", "newdir", "pytest.ini"] {
        assert!(reverted.contains(&p), "{p} not in {reverted:?}");
    }
}

#[test]
fn exec_cannot_replace_a_folder_with_a_symlink_to_escape_the_revert() {
    // If `tests` became a link to a host folder, restoring files under it
    // must not write into that host folder.
    // Write globs that allow the folder name `tests` but NOT tests/data.txt,
    // so the revert has to restore data.txt inside it.
    let text = policy_text(256, 64, 20)
        .replace(r#"write = ["src/**", "tests/**", "**/__pycache__/**"]"#, r#"write = ["src/**", "tests", "**/__pycache__/**"]"#);
    assert!(text.contains(r#""tests","#), "policy rewrite did not apply");
    let t = TestEnv::new("sec-revert-link").with_policy_text(&text);
    let repo = make_repo(&t);
    std::fs::write(repo.join("tests/data.txt"), "data\n").unwrap();
    let decoy = t.root.join("decoy");
    std::fs::create_dir(&decoy).unwrap();
    let code = format!("import shutil\nshutil.rmtree('tests')\nos.symlink({:?}, 'tests')\nprint('swapped')", decoy.display().to_string());
    let (r, out) = attack(&t, &repo, &code);
    assert_eq!(out["exit_code"], 0, "{out}");
    let work = t.exec_dir(&r.id).join("work");
    assert_eq!(std::fs::read_dir(&decoy).unwrap().count(), 0, "nothing written into the host folder");
    assert!(std::fs::symlink_metadata(work.join("tests")).unwrap().is_dir());
    assert_eq!(std::fs::read_to_string(work.join("tests/data.txt")).unwrap(), "data\n");
}

// ===========================================================================
// 4. Network
// ===========================================================================

#[test]
fn exec_has_no_network() {
    let t = env("sec-net");
    let repo = make_repo(&t);
    // A listener on the host's loopback, which a shared network would reach.
    let host_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = host_listener.local_addr().unwrap().port();
    let code = py(&format!(
        "probe('tcp to 1.1.1.1:80', lambda: socket.create_connection(('1.1.1.1', 80), timeout=3))\n\
         probe('tcp to host loopback listener', lambda: socket.create_connection(('127.0.0.1', {port}), timeout=3))\n\
         probe('udp dns to 8.8.8.8', lambda: socket.socket(socket.AF_INET, socket.SOCK_DGRAM).sendto(b'x', ('8.8.8.8', 53)))\n\
         probe('ipv6 to 2606:4700::1111', lambda: socket.create_connection(('2606:4700::1111', 80), timeout=3))\n\
         probe('resolve a name', lambda: socket.getaddrinfo('example.com', 80))"
    ));
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["tcp to 1.1.1.1:80", "tcp to host loopback listener", "udp dns to 8.8.8.8", "ipv6 to 2606:4700::1111", "resolve a name"]);
    drop(host_listener);
}

#[test]
fn exec_cannot_reach_host_abstract_unix_sockets() {
    // Abstract unix sockets (names starting with a zero byte) belong to the
    // network namespace, so a new namespace cannot see the host's.
    let t = env("sec-abstract");
    let repo = make_repo(&t);
    let name = format!("norn-test-{}", std::process::id());
    use std::os::linux::net::SocketAddrExt; // abstract names are Linux-only
    let addr = std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()).unwrap();
    let _l = std::os::unix::net::UnixListener::bind_addr(&addr).unwrap();
    let code = py(&format!(
        "s = socket.socket(socket.AF_UNIX)\nprobe('connect abstract socket', lambda: s.connect('\\0{name}'))"
    ));
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["connect abstract socket"]);
}

/// The seccomp filter (D-116): system calls that widen the kernel surface are
/// refused with EPERM even though everything else about the sandbox would
/// allow them. Nested user namespaces are the case `threat-model.md` §4 named.
/// Some of these calls also fail without a filter (bad arguments give EFAULT
/// or EINVAL), so the test asks for EPERM exactly, before the kernel looks at
/// the arguments. System call numbers are x86_64's.
#[cfg(target_arch = "x86_64")]
#[test]
fn seccomp_refuses_namespaces_mounts_and_kernel_interfaces() {
    let t = env("sec-seccomp");
    let repo = make_repo(&t);
    let code = r#"
import ctypes, errno, subprocess, threading
libc = ctypes.CDLL(None, use_errno=True)
def sc(name, nr, *args):
    ctypes.set_errno(0)
    r = libc.syscall(ctypes.c_long(nr), *[ctypes.c_long(a) for a in args])
    if r == 0 and nr == 56:
        os._exit(0)  # the child of a clone that worked
    if r > 0 and nr == 56:
        os.waitpid(r, 0)
    e = ctypes.get_errno() if r == -1 else 0
    print(f"RESULT {name} {errno.errorcode.get(e, 'OK') if e else 'OK'}", flush=True)
CLONE_NEWUSER, CLONE_NEWNS, SIGCHLD = 0x10000000, 0x00020000, 17
sc('unshare-user', 272, CLONE_NEWUSER)
sc('unshare-mount', 272, CLONE_NEWNS)
sc('clone-newuser', 56, CLONE_NEWUSER | SIGCHLD, 0, 0, 0, 0)
sc('mount', 165, 0, 0, 0, 0, 0)
sc('io_uring_setup', 425, 1, 0)
sc('bpf', 321, 0, 0, 0)
sc('keyctl', 250, 0, 0)
# PTRACE_TRACEME that works would stop this process at its next signal, so a
# child asks and then exits.
pid = os.fork()
if pid == 0:
    sc('ptrace', 101, 0, 0, 0, 0)
    os._exit(0)
os.waitpid(pid, 0)
sc('perf_event_open', 298, 0, 0, -1, -1, 0)
sc('userfaultfd', 323, 0)
sc('clone3', 435, 0, 0)
th = threading.Thread(target=lambda: None); th.start(); th.join()
print('subprocess', subprocess.run(['python3', '-c', 'print(7)'], capture_output=True, text=True).stdout.strip())
"#;
    let (_, out) = attack(&t, &repo, code);
    let stdout = out["stdout"].as_str().unwrap();
    let stderr = out["stderr"].as_str().unwrap_or("");
    for name in ["unshare-user", "unshare-mount", "clone-newuser", "mount", "io_uring_setup", "bpf", "keyctl", "ptrace", "perf_event_open", "userfaultfd"] {
        assert!(stdout.contains(&format!("RESULT {name} EPERM\n")), "{name} must be refused with EPERM\nstdout:\n{stdout}\nstderr:\n{stderr}");
    }
    // clone3 cannot be inspected; "not implemented" makes libc fall back to clone.
    assert!(stdout.contains("RESULT clone3 ENOSYS\n"), "clone3 must give ENOSYS\nstdout:\n{stdout}");
    assert!(stdout.contains("subprocess 7"), "ordinary work must still run under the filter:\n{stdout}\n{stderr}");
}

// ===========================================================================
// 5. Resources: fork bombs, memory, infinite loops, leftover processes
// ===========================================================================

#[test]
fn fork_bomb_is_stopped_by_pids_max() {
    let t = env("sec-forkbomb");
    let repo = make_repo(&t);
    let code = "import time\nn=0\nend=time.time()+3\nwhile time.time()<end:\n    try:\n        if os.fork()==0:\n            time.sleep(30); os._exit(0)\n        n+=1\n    except OSError:\n        pass\nprint('forked', n)";
    let (_, out) = attack(&t, &repo, code);
    let forked: u64 = out["stdout"].as_str().unwrap().trim().strip_prefix("forked ").unwrap().parse().unwrap();
    assert!(forked < 64, "pids.max=64 must cap the fork bomb, got {forked}");
    assert!(out["stderr"].as_str().unwrap().contains("limits.pids"));
}

#[test]
fn memory_exhaustion_is_stopped_by_memory_max() {
    let t = TestEnv::new("sec-oom").with_policy_text(&policy_text(64, 64, 20));
    let repo = make_repo(&t);
    let code = "blocks=[]\nwhile True:\n    blocks.append(bytearray(16*1024*1024))\n    print(len(blocks)*16, 'MB', flush=True)";
    let (_, out) = attack(&t, &repo, code);
    assert_eq!(out["signal"], 9, "killed by SIGKILL: {out}");
    assert!(out["stderr"].as_str().unwrap().contains("OOM"), "{out}");
}

#[test]
fn infinite_loop_is_killed_at_the_time_limit() {
    let t = TestEnv::new("sec-loop").with_policy_text(&policy_text(256, 64, 2));
    let repo = make_repo(&t);
    let start = std::time::Instant::now();
    let (_, out) = attack(&t, &repo, "while True:\n    pass");
    assert_eq!(out["timed_out"], true, "{out}");
    assert!(start.elapsed() < std::time::Duration::from_secs(15));
}

#[test]
fn background_processes_die_with_the_command() {
    // A daemon that double-forks and waits to write later must be gone when
    // the exec call returns, or it could change /work after the checkpoint.
    let t = env("sec-daemon");
    let repo = make_repo(&t);
    let code = "import time\nif os.fork()==0:\n    os.setsid()\n    if os.fork()==0:\n        time.sleep(3)\n        open('/work/src/late.py','w').write('late')\n        os._exit(0)\n    os._exit(0)\nprint('parent exits')";
    let (r, _) = attack(&t, &repo, code);
    std::thread::sleep(std::time::Duration::from_secs(4));
    assert!(!t.exec_dir(&r.id).join("work/src/late.py").exists(), "the daemon outlived the command");
}

// ===========================================================================
// 6. Privileges
// ===========================================================================

#[test]
fn exec_has_no_capabilities_and_cannot_gain_root() {
    let t = env("sec-caps");
    let repo = make_repo(&t);
    let code = py(
        "st = open('/proc/self/status').read()\n\
         caps = {l.split(':')[0]: l.split()[1] for l in st.splitlines() if l.startswith('Cap') or l.startswith('NoNewPrivs')}\n\
         print('caps', caps)\n\
         probe('setuid(0)', lambda: os.setuid(0))\n\
         probe('effective caps nonzero', lambda: caps['CapEff'] if int(caps['CapEff'],16) else (_ for _ in ()).throw(ValueError()))\n\
         probe('bounding caps nonzero', lambda: caps['CapBnd'] if int(caps['CapBnd'],16) else (_ for _ in ()).throw(ValueError()))\n\
         probe('no_new_privs off', lambda: caps['NoNewPrivs'] if caps['NoNewPrivs'] != '1' else (_ for _ in ()).throw(ValueError()))",
    );
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["setuid(0)", "effective caps nonzero", "bounding caps nonzero", "no_new_privs off"]);
}

#[test]
fn exec_cannot_mount_even_in_a_nested_user_namespace() {
    // A sandboxed process may create a new user namespace and so regain
    // "root" inside it. Landlock still forbids mounting, so it cannot build
    // itself a different view of the files.
    let t = env("sec-nested-ns");
    let repo = make_repo(&t);
    let code = py(
        "import ctypes\nlibc = ctypes.CDLL(None, use_errno=True)\n\
         def mount_tmpfs():\n    r = libc.mount(b'tmpfs', b'/work/src', b'tmpfs', 0, None)\n    if r != 0: raise OSError(ctypes.get_errno(), 'mount')\n    return 'mounted'\n\
         probe('mount in sandbox', mount_tmpfs)\n\
         def nested():\n    if libc.unshare(0x10000000 | 0x00020000) != 0: raise OSError(ctypes.get_errno(), 'unshare')\n    return mount_tmpfs()\n\
         probe('mount after nested unshare', nested)",
    );
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["mount in sandbox", "mount after nested unshare"]);
}

#[test]
fn exec_cannot_signal_host_processes() {
    let t = env("sec-signal");
    let repo = make_repo(&t);
    // The test's own pid is a host process the attacker would like to kill.
    let me = std::process::id();
    let code = py(&format!("import signal\nprobe('kill host pid {me}', lambda: os.kill({me}, signal.SIGKILL))"));
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &[&format!("kill host pid {me}")]);
}

#[test]
fn exec_tmp_is_private() {
    let t = env("sec-tmp");
    let repo = make_repo(&t);
    let marker = format!("/tmp/norn-sec-tmp-{}", std::process::id());
    let code = format!("open({marker:?}, 'w').write('x')\nprint('wrote')");
    let (_, out) = attack(&t, &repo, &code);
    assert_eq!(out["exit_code"], 0);
    assert!(!Path::new(&marker).exists(), "sandbox /tmp must not be the host's /tmp");
}

// ===========================================================================
// 7. The output patch (D-035)
// ===========================================================================

#[test]
fn patch_does_not_follow_symlinks_to_host_files() {
    let t = env("sec-patch");
    let repo = make_repo(&t);
    let secret = t.root.join("host-secret.txt");
    std::fs::write(&secret, "PATCH-CANARY-9\n").unwrap();
    let code = format!("os.symlink({:?}, 'src/leak.py')\nprint('linked')", secret.display().to_string());
    let (r, _) = attack(&t, &repo, &code);
    let patch = std::fs::read_to_string(t.exec_dir(&r.id).join("result.patch")).unwrap();
    assert!(patch.contains("leak.py"), "the new link is in the patch: {patch}");
    assert!(!patch.contains("PATCH-CANARY-9"), "the patch must not contain the linked file's content");
}

// ===========================================================================
// 8. Added in M5: races, special files, argv tricks, floods
// ===========================================================================

/// The TOCTOU race from the v1 design: while the agent writes and reads
/// through `src/swap/...`, something keeps re-pointing the symlink `src/swap`
/// between a folder inside the workspace and a folder outside it.
///
/// Inside Norn nothing can run at the same time as a file tool (one call
/// at a time, and every sandboxed process is dead when its call returns), so
/// the test plays a stronger attacker than the threat model allows: a host
/// thread doing the swaps directly on the workspace folder.
///
/// One attempt is a full run of 300 racy calls. The race only means something
/// if it happened often, so the counts must reach more than 1,000 swaps and at
/// least 20 calls ending each way. The swapper paces itself with a sleep, and
/// on a slow machine (the CI runner: 925 swaps in run 43742105191) one attempt
/// may not reach that. Then another attempt runs in a fresh execution, up to
/// five, and the counts add up. Every attempt checks for escapes on its own.
#[test]
fn toctou_symlink_swap_during_writes_and_reads_never_escapes() {
    let (mut swaps, mut done, mut denied) = (0u64, 0usize, 0usize);
    let mut attempts = 0;
    while attempts < 5 && !(swaps > 1000 && denied >= 20 && done >= 20) {
        attempts += 1;
        let (s, c, d) = toctou_attempt(attempts);
        swaps += s;
        done += c;
        denied += d;
    }
    println!("toctou total: {swaps} swaps, {done} completed inside, {denied} refused, {attempts} attempt(s)");
    assert!(swaps > 1000, "the swapper only managed {swaps} swaps in {attempts} attempts; the race was not exercised");
    assert!(denied >= 20 && done >= 20, "both outcomes must occur often for the race to mean anything (denied {denied}, completed {done})");
}

/// One run of the TOCTOU race; asserts that nothing escaped and returns
/// (swaps, calls completed inside, calls refused as leaving the workspace).
fn toctou_attempt(n: usize) -> (u64, usize, usize) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    let text = policy_text(256, 64, 20).replace("max_tool_calls = 50", "max_tool_calls = 1000");
    assert!(text.contains("max_tool_calls = 1000"));
    let t = TestEnv::new(&format!("sec-toctou-{n}")).with_policy_text(&text);
    let repo = make_repo(&t);
    let outside = t.root.join("outside");
    std::fs::create_dir(&outside).unwrap();
    std::fs::write(outside.join("secret.txt"), "TOCTOU-CANARY-77\n").unwrap();
    // In the repo: a real folder, and `src/swap`, a symlink that the attacker
    // re-points between that folder (harmless) and the outside folder.
    std::fs::create_dir(repo.join("src/realdir")).unwrap();
    std::fs::write(repo.join("src/realdir/secret.txt"), "inside\n").unwrap();
    std::os::unix::fs::symlink("realdir", repo.join("src/swap")).unwrap();

    const ROUNDS: usize = 150;
    let mut steps = String::new();
    for i in 0..ROUNDS {
        steps.push_str(&format!("[[step]]\ntool = \"fs.write\"\npath = \"src/swap/w{i}.txt\"\ncontent = \"pwned {i}\"\n"));
        steps.push_str("[[step]]\ntool = \"fs.read\"\npath = \"src/swap/secret.txt\"\n");
    }
    let script = t.write_script("toctou.toml", &format!("task = \"toctou\"\n{steps}"));
    let child = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    // The swapper: wait for the workspace, then exchange the two names as
    // fast as possible until the run ends.
    let stop = Arc::new(AtomicBool::new(false));
    let (stop2, state, outside2) = (stop.clone(), t.state.clone(), outside.clone());
    let swapper = std::thread::spawn(move || {
        let work = loop {
            if stop2.load(Ordering::SeqCst) {
                return 0u64;
            }
            let found = std::fs::read_dir(state.join("executions")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()).map(|e| e.path().join("work"));
            match found {
                Some(w) if std::fs::symlink_metadata(w.join("src/swap")).is_ok() => break w,
                _ => std::thread::sleep(std::time::Duration::from_millis(1)),
            }
        };
        // Re-point the link atomically: make a new link under a temporary
        // name, then rename it over `src/swap`.
        let (link, tmp) = (work.join("src/swap"), work.join("src/.swap-tmp"));
        let mut swaps = 0u64;
        while !stop2.load(Ordering::SeqCst) {
            let target: &std::path::Path = if swaps.is_multiple_of(2) { &outside2 } else { std::path::Path::new("realdir") };
            let _ = std::fs::remove_file(&tmp);
            if std::os::unix::fs::symlink(target, &tmp).is_ok() && std::fs::rename(&tmp, &link).is_ok() {
                swaps += 1;
            }
            // A short pause, so some runtime operations fit between two swaps.
            std::thread::sleep(std::time::Duration::from_micros(150));
        }
        swaps
    });
    let first = child.wait_with_output().unwrap();
    // The swapper also races the runtime's own checkpoint copy (`cp -a`),
    // which nothing inside Norn could ever do. When that copy fails the
    // runtime stops with an error; the test then resumes the execution (which
    // recovers first) until every step is settled, so that all 300 racy calls
    // really happen.
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let mut finished = first.status.success();
    let mut restarts = 0;
    while !finished && restarts < 400 {
        restarts += 1;
        finished = t.output(&["run", "--quiet", "--resume", &id]).status.success();
    }
    stop.store(true, Ordering::SeqCst);
    let swaps = swapper.join().unwrap();
    assert!(finished, "the run did not finish even after {restarts} restarts");

    let leaked: Vec<_> = std::fs::read_dir(&outside).unwrap().map(|e| e.unwrap().file_name()).filter(|n| n != "secret.txt").collect();
    assert!(leaked.is_empty(), "files were written outside the workspace: {leaked:?}");
    assert_eq!(std::fs::read_to_string(outside.join("secret.txt")).unwrap(), "TOCTOU-CANARY-77\n", "outside file was modified");
    // The secret was never read into a tool result or the log.
    let grep = std::process::Command::new("grep")
        .args(["-r", "-l", "-a", "TOCTOU-CANARY-77"])
        .arg(t.exec_dir(&id).join("blobs"))
        .arg(t.exec_dir(&id).join("log.db"))
        .output()
        .unwrap();
    assert!(grep.stdout.is_empty(), "the outside secret was read: {}", String::from_utf8_lossy(&grep.stdout));
    let events = t.events(&id);
    let denied = events.iter().filter(|e| e["type"] == "tool.denied").count();
    let done = events.iter().filter(|e| e["type"] == "tool.completed").count();
    println!("toctou attempt {n}: {swaps} swaps, {done} calls completed inside the workspace, {denied} refused as leaving it, {restarts} restarts");
    (swaps, done, denied)
}

#[test]
fn fs_read_of_a_fifo_planted_by_a_command_does_not_hang_the_runtime() {
    // Opening a FIFO for reading blocks until someone writes to it. A command
    // can create one inside the write globs and then ask the runtime to read it.
    let t = TestEnv::new("sec-fifo").with_policy_text(&policy_text(256, 64, 20));
    let repo = make_repo(&t);
    let steps = "[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", \"import os; os.mkfifo('src/pipe'); print('made')\"]\n\
                 [[step]]\ntool = \"fs.read\"\npath = \"src/pipe\"\n\
                 [[step]]\ntool = \"fs.write\"\npath = \"src/pipe\"\ncontent = \"x\"\n\
                 [[step]]\ntool = \"fs.list\"\npath = \"src\"\n";
    let script = t.write_script("fifo.toml", &format!("task = \"fifo\"\n{steps}"));
    let mut child = t
        .cmd(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let start = std::time::Instant::now();
    let status = loop {
        if let Some(s) = child.try_wait().unwrap() {
            break s;
        }
        if start.elapsed() > std::time::Duration::from_secs(20) {
            child.kill().unwrap();
            panic!("the runtime hung for 20 s reading a FIFO");
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    };
    let out = child.wait_with_output().unwrap();
    assert!(status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let r = Run { events: t.events(&id), id };
    assert!(r.failed(2).unwrap().contains("not a regular file"), "{:?}", r.failed(2));
    assert!(r.failed(3).unwrap().contains("not a regular file"), "{:?}", r.failed(3));
    assert!(r.events.iter().any(|e| e["type"] == "execution.finished"));
}

#[test]
fn exec_cannot_hardlink_host_files_or_create_devices() {
    let t = env("sec-link-dev");
    let repo = make_repo(&t);
    let code = py(
        "import stat, glob\n\
         target = (glob.glob('/nix/store/*/bin/*') + glob.glob('/usr/bin/*'))[0]\n\
         probe('hardlink a system file into /work', lambda: os.link(target, '/work/src/linked'))\n\
         probe('mknod a disk device', lambda: os.mknod('/work/src/sda', stat.S_IFBLK | 0o666, os.makedev(8, 0)))\n\
         probe('mknod /dev/mem', lambda: os.mknod('/tmp/mem', stat.S_IFCHR | 0o666, os.makedev(1, 1)))\n\
         probe('write to /proc/sys', lambda: open('/proc/sys/kernel/hostname', 'w').write('pwned'))\n\
         probe('open /dev/mem', lambda: open('/dev/mem', 'rb'))",
    );
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["hardlink a system file into /work", "mknod a disk device", "mknod /dev/mem", "write to /proc/sys", "open /dev/mem"]);
}

#[test]
fn exec_cannot_escape_through_proc_root_links() {
    let t = env("sec-proc-root");
    let repo = make_repo(&t);
    let home = std::env::var("HOME").unwrap();
    let code = py(&format!(
        "probe('/proc/1/root to host home', lambda: os.listdir('/proc/1/root' + {home:?}))\n\
         probe('/proc/self/root/etc/passwd', lambda: open('/proc/self/root/etc/passwd').read())\n\
         probe('/proc/1/cwd outside /work', lambda: os.readlink('/proc/1/cwd') if os.readlink('/proc/1/cwd') != '/work' else (_ for _ in ()).throw(ValueError()))\n\
         probe('chroot', lambda: os.chroot('/tmp'))"
    ));
    let (_, out) = attack(&t, &repo, &code);
    assert_blocked(&out, &["/proc/1/root to host home", "/proc/self/root/etc/passwd", "/proc/1/cwd outside /work", "chroot"]);
}

#[test]
fn shipped_policy_refuses_shells_interpreters_and_path_tricks() {
    // With the policy as shipped (no python3 -c), none of these may run.
    let t = TestEnv::new("sec-argv");
    let repo = make_repo(&t);
    let tries: [&[&str]; 9] = [
        &["sh", "-c", "cat /etc/passwd"],
        &["bash"],
        &["/bin/sh"],
        &["python3", "-c", "print(1)"],
        &["python3"],
        &["env", "pytest"],
        &["pytest; cat .env"],
        &["../../bin/sh"],
        &["python3", "-m", "http.server"],
    ];
    let mut steps = String::new();
    for argv in tries {
        let list: Vec<String> = argv.iter().map(|a| format!("{a:?}")).collect();
        steps.push_str(&format!("[[step]]\ntool = \"exec\"\nargv = [{}]\n", list.join(", ")));
    }
    steps.push_str("[[step]]\ntool = \"exec\"\nargv = []\n");
    let r = run_steps(&t, &repo, &steps);
    for c in 1..=9u64 {
        assert_eq!(r.denied_rule(c).as_deref(), Some("no matching rule"), "argv {:?} was not denied", tries[c as usize - 1]);
    }
    assert_eq!(r.denied_rule(10).as_deref(), Some("invalid request"));
    assert!(!r.events.iter().any(|e| e["type"] == "tool.intent"), "no denied command may get as far as an intent");
}

#[test]
fn denied_paths_cannot_be_created_or_read_later() {
    let t = env("sec-deny-later");
    let repo = make_repo(&t);
    let steps = "[[step]]\ntool = \"fs.write\"\npath = \".env\"\ncontent = \"TOKEN=x\"\n\
                 [[step]]\ntool = \"fs.write\"\npath = \"src/.env\"\ncontent = \"TOKEN=x\"\n\
                 [[step]]\ntool = \"fs.write\"\npath = \"src/key.pem\"\ncontent = \"k\"\n\
                 [[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", \"open('src/.env','w').write('LATER-' + 'CANARY'); import os; os.symlink('.env', 'src/alias')\"]\n\
                 [[step]]\ntool = \"fs.read\"\npath = \"src/.env\"\n\
                 [[step]]\ntool = \"fs.read\"\npath = \"src/alias\"\n";
    let r = run_steps(&t, &repo, steps);
    for c in [1, 2, 3, 5] {
        assert!(r.denied_rule(c).unwrap_or_default().starts_with("filesystem.deny"), "call {c}: {:?}", r.denied_rule(c));
    }
    // (The canary is assembled inside the command, so it is not in the logged argv.)
    // The denied file a command created is removed again (D-009), so the
    // alias points at nothing; either way its content is never returned.
    assert!(r.denied_rule(6).is_some() || r.failed(6).is_some(), "call 6 must not succeed");
    assert!(!t.exec_dir(&r.id).join("work/src/.env").exists());
    let grep = std::process::Command::new("grep").args(["-r", "-l", "-a", "LATER-CANARY"]).arg(t.exec_dir(&r.id)).output().unwrap();
    assert!(grep.stdout.is_empty(), "denied content kept in {}", String::from_utf8_lossy(&grep.stdout));
}

#[test]
fn output_flood_is_truncated_and_does_not_fill_the_log() {
    let t = env("sec-flood");
    let repo = make_repo(&t);
    let code = "chunk = 'A' * 65536\nfor _ in range(800):\n    sys.stdout.write(chunk); sys.stderr.write(chunk)\nprint('end')";
    let (r, out) = attack(&t, &repo, code); // ~52 MB on each stream
    assert_eq!(out["truncated"], true);
    assert_eq!(out["exit_code"], 0, "the command is not blocked by a full pipe");
    assert!(out["stdout"].as_str().unwrap().len() <= 64 * 1024);
    let size = |p: &str| std::fs::metadata(t.exec_dir(&r.id).join(p)).map(|m| m.len()).unwrap_or(0);
    assert!(size("log.db") + size("log.db-wal") < 2 * 1024 * 1024, "log grew to {} bytes", size("log.db") + size("log.db-wal"));
}
