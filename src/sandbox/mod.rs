//! The sandbox: where `exec` commands run.
//!
//! | Layer | Mechanism | File |
//! |---|---|---|
//! | Filesystem view | mount namespace, new root with `/work` + read-only system folders | `ns.rs` |
//! | Path access (fs tools) | `openat2(RESOLVE_BENEATH)` on a workspace handle | `beneath.rs` |
//! | Path access (exec) | Landlock | `landlock.rs` |
//! | Process | user + PID namespace, no capabilities, `no_new_privs` | `ns.rs` |
//! | Network | empty network namespace (loopback only) | `ns.rs` |
//! | Resources | cgroup v2 limits; watchdog kills the cgroup | `cgroup.rs` |
//! | Environment | cleared; only PATH, HOME, LANG | `gateway.rs` |

pub mod beneath;
pub mod cgroup;
pub mod landlock;
pub mod ns;

use std::io::Read;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::Result;

use crate::tools::exec::capture;
use crate::tools::{ExecOutput, ToolError};
use cgroup::ExecCgroup;
pub use ns::SandboxSpec;

/// Run one command in a fresh sandbox inside `cg` and wait for it.
///
/// `helper` is the agentos binary itself (it is started again as
/// `agentos __sandbox-init`).
pub fn run(helper: &Path, spec: &SandboxSpec, cg: &ExecCgroup, timeout: Duration, max_output: usize) -> Result<ExecOutput, ToolError> {
    let start = Instant::now();
    let fail = |m: String| ToolError::new(format!("sandbox: {m}"));
    let json = serde_json::to_string(spec).map_err(|e| fail(e.to_string()))?;
    let (status_r, status_w): (OwnedFd, OwnedFd) =
        nix::unistd::pipe2(nix::fcntl::OFlag::O_CLOEXEC).map_err(|e| fail(format!("pipe: {e}")))?;

    let mut cmd = Command::new(helper);
    cmd.arg("__sandbox-init")
        .arg(&json)
        .env_clear() // the helper itself gets no environment either
        .current_dir("/")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let w = status_w.as_raw_fd();
    // SAFETY: runs in the child between fork and exec, where only
    // async-signal-safe calls are allowed; dup2 and fcntl are. dup2 onto
    // fd 3 clears close-on-exec on the copy, so the helper inherits it.
    unsafe {
        cmd.pre_exec(move || {
            if w == ns::STATUS_FD {
                let flags = libc::fcntl(w, libc::F_GETFD);
                if flags < 0 || libc::fcntl(w, libc::F_SETFD, flags & !libc::FD_CLOEXEC) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            } else if libc::dup2(w, ns::STATUS_FD) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| fail(format!("starting helper {}: {e}", helper.display())))?;
    drop(status_w); // keep only the helper's copy, so EOF arrives when it is gone
    let stdout = capture(child.stdout.take().expect("piped"), max_output);
    let stderr = capture(child.stderr.take().expect("piped"), max_output);
    let status_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::fs::File::from(status_r).read_to_string(&mut s);
        s
    });

    // Watchdog: at the wall-clock limit, kill the whole cgroup in one step.
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let cg_for_watchdog = cg.clone();
    let watchdog = std::thread::spawn(move || {
        if done_rx.recv_timeout(timeout).is_err() {
            let _ = cg_for_watchdog.kill();
            return true;
        }
        false
    });
    let wait = child.wait();
    let _ = done_tx.send(());
    let timed_out = watchdog.join().unwrap_or(false);
    let oom = cg.oom_killed();
    let pids_hit = cg.pids_limit_hit();
    // Anything still in the group (there should be nothing once PID 1 is
    // gone) is killed, and the group is removed.
    let cleanup = cg.destroy();
    let (out, out_cut) = stdout.join().unwrap_or_default();
    let (err, err_cut) = stderr.join().unwrap_or_default();
    let report = status_reader.join().unwrap_or_default();
    wait.map_err(|e| fail(format!("waiting for helper: {e}")))?;
    cleanup.map_err(|e| fail(format!("{e:#}")))?;

    let mut exit_code = None;
    let mut signal = None;
    for line in report.lines() {
        if let Some(m) = line.strip_prefix("error: ") {
            return Err(fail(format!("setup failed: {m}")));
        }
        if let Some(m) = line.strip_prefix("exec-failed: ") {
            return Err(ToolError::new(format!("cannot start command: {m}")));
        }
        if let Some(n) = line.strip_prefix("exit ") {
            exit_code = n.parse().ok();
        }
        if let Some(n) = line.strip_prefix("signal ") {
            signal = n.parse().ok();
        }
    }
    if exit_code.is_none() && signal.is_none() {
        // Killed before PID 1 could report: by the watchdog or the OOM killer.
        signal = Some(libc::SIGKILL);
    }
    let mut stderr_text = err;
    if oom {
        stderr_text.push_str("\n[agentos] the command hit limits.memory_mb and was killed by the OOM killer\n");
    }
    if pids_hit {
        stderr_text.push_str("\n[agentos] the command hit limits.pids; some fork() calls failed\n");
    }
    Ok(ExecOutput {
        exit_code,
        signal,
        timed_out,
        stdout: out,
        stderr: stderr_text,
        truncated: out_cut || err_cut,
        duration_ms: start.elapsed().as_millis() as u64,
        reverted: Vec::new(),
    })
}

/// Check, at startup, that this machine can run the sandbox. Returns every
/// problem found, each with what to do about it (CLAUDE.md, "Portability").
///
/// `helper` is the agentos binary. It is started once as
/// `agentos __probe-userns` to find out whether a user namespace can really
/// be used. Reading the kernel's switches is not enough for that: on Ubuntu
/// the switch `apparmor_restrict_unprivileged_userns` can be on and agentos
/// still be allowed, through its AppArmor profile (D-093).
pub fn check_host(helper: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let read = |p: &str| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string());
    let probe = Command::new(helper).arg(ns::PROBE_ARG).env_clear().stdin(Stdio::null()).output();
    let failure = match probe {
        Ok(out) if out.status.success() => None,
        Ok(out) => Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
        Err(e) => Some(format!("starting {}: {e}", helper.display())),
    };
    if let Some(why) = failure {
        // The probe says that it does not work; the switches say why.
        let mut msg = format!("user namespaces cannot be used ({why})");
        if read("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").as_deref() == Some("1") {
            msg.push_str(
                "\n    This system (Ubuntu 23.10 or newer) only allows them for programs with an AppArmor profile. \
                 Install the one that comes with agentos; it allows them for agentos alone:\n      \
                 sudo install -m 644 packaging/apparmor/agentos /etc/apparmor.d/agentos\n      \
                 sudo apparmor_parser -r /etc/apparmor.d/agentos\n    \
                 (The profile names /usr/local/bin/agentos; change that line if the program is somewhere else.)\n    \
                 Or, for every program on the machine: sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0",
            );
        } else if read("/proc/sys/user/max_user_namespaces").as_deref() == Some("0") {
            msg.push_str("\n    They are switched off: /proc/sys/user/max_user_namespaces is 0");
        } else if read("/proc/sys/kernel/unprivileged_userns_clone").as_deref() == Some("0") {
            msg.push_str("\n    They are switched off for ordinary users; switch them on with: sudo sysctl -w kernel.unprivileged_userns_clone=1");
        }
        problems.push(msg);
    }
    if landlock::kernel_abi() < 1 {
        problems.push("Landlock is not available (kernel 5.13+ with Landlock in the `lsm=` list is needed)".into());
    }
    match beneath::open_root(Path::new("/")) {
        Ok(root) => {
            if let Err(nix::errno::Errno::ENOSYS) = beneath::open_beneath(&root, ".", nix::fcntl::OFlag::O_PATH, nix::fcntl::ResolveFlag::empty()) {
                problems.push("openat2 is not available (kernel 5.6+ is needed)".into());
            }
        }
        Err(e) => problems.push(format!("cannot open /: {e}")),
    }
    if let Err(e) = cgroup::CgroupRoot::init() {
        problems.push(format!("{e:#}"));
    }
    problems
}

/// Name of the cgroup for one call. Fixed by the execution id and call id, so
/// recovery can find and kill a group left behind by a crash.
pub fn cgroup_name(execution_id: &str, call_id: u64) -> String {
    format!("x-{execution_id}-c{call_id}")
}

/// The real path of the folder that holds a program, if it is under one of
/// the read-only mounts (otherwise the sandbox could not see it).
pub fn visible_in_sandbox(program: &Path, ro_mounts: &[PathBuf]) -> bool {
    ro_mounts.iter().any(|m| {
        let real = std::fs::canonicalize(m).unwrap_or_else(|_| m.clone());
        program.starts_with(&real) || program.starts_with(m)
    })
}
