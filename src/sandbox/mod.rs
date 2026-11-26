//! The sandbox: where `exec` commands run.
//!
//! | Layer | Mechanism | File |
//! |---|---|---|
//! | Filesystem view | mount namespace, new root with `/work` + read-only system folders | `ns.rs` |
//! | Path access (fs tools) | `openat2(RESOLVE_BENEATH)` on a workspace handle | `beneath.rs` |
//! | Path access (exec) | Landlock | `landlock.rs` |
//! | Process | user + PID namespace, no capabilities, `no_new_privs` | `ns.rs` |
//! | System calls | seccomp deny list: namespaces, mounts, `bpf`, `ptrace`, `io_uring`, ... fail with `EPERM` | `seccomp.rs` |
//! | Network | empty network namespace (loopback only); with `[network] allow`, one way out: a proxy in the runtime that checks each host (D-155) | `ns.rs`, `proxy.rs` |
//! | Resources | cgroup v2 limits; watchdog kills the cgroup at the time limit or when the workspace is over `workspace_mb` | `cgroup.rs`, here |
//! | Environment | cleared; only PATH, HOME, LANG | `gateway.rs` |

pub mod beneath;
pub mod cgroup;
pub mod landlock;
pub mod ns;
pub mod proxy;
pub mod seccomp;

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

/// The workspace size limit for one command (D-117).
#[derive(Debug, Clone)]
pub struct DiskLimit {
    /// The workspace on the host.
    pub dir: PathBuf,
    pub max_bytes: u64,
}

/// What ended a command early, if the runtime did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stopped {
    Timeout,
    /// The workspace measured this many bytes, over the limit.
    Disk(u64),
}

/// The shortest time between two measurements of the workspace.
const DISK_INTERVAL_MIN: Duration = Duration::from_millis(50);

/// Bytes on disk below `dir`: allocated blocks, so a sparse file counts as
/// what it really uses. Symlinks are not followed. Entries that vanish or
/// cannot be read while the command runs are skipped; the walk is a
/// measurement, not a record. About 2 µs per entry (experiment
/// `m11-disk-limit`).
pub fn disk_usage(dir: &Path) -> u64 {
    use std::os::unix::fs::MetadataExt;
    let mut bytes = 0u64;
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else { continue };
        for e in entries.flatten() {
            let Ok(m) = e.metadata() else { continue };
            bytes = bytes.saturating_add(m.blocks() * 512);
            if m.is_dir() {
                stack.push(e.path());
            }
        }
    }
    bytes
}

/// Run one command in a fresh sandbox inside `cg` and wait for it.
///
/// `helper` is the norn binary itself (it is started again as
/// `norn __sandbox-init`). The second value says whether the runtime
/// stopped the command (timeout or disk limit).
pub fn run(
    helper: &Path,
    spec: &SandboxSpec,
    cg: &ExecCgroup,
    timeout: Duration,
    max_output: usize,
    disk: &DiskLimit,
    hosts: &[(String, crate::policy::HostRule)],
) -> Result<(ExecOutput, Option<Stopped>), ToolError> {
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
    // With hosts allowed, a socket pair: the helper gets one end as fd 4,
    // the proxy keeps the other (D-155). Both are close-on-exec here.
    let net = if spec.proxy {
        let (ours, theirs) = std::os::unix::net::UnixStream::pair().map_err(|e| fail(format!("socketpair: {e}")))?;
        Some((OwnedFd::from(ours), OwnedFd::from(theirs)))
    } else {
        None
    };
    let w = status_w.as_raw_fd();
    let n = net.as_ref().map(|(_, theirs)| theirs.as_raw_fd()).unwrap_or(-1);
    // SAFETY: runs in the child between fork and exec, where only
    // async-signal-safe calls are allowed; fcntl and dup2 are. Each
    // descriptor is first copied above 10, so placing one on 3 can never
    // close the other before it is copied; dup2 then clears close-on-exec
    // on the copies at 3 and 4, so the helper inherits exactly those.
    unsafe {
        cmd.pre_exec(move || {
            let high = |fd: i32| {
                let h = libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 10);
                if h < 0 { Err(std::io::Error::last_os_error()) } else { Ok(h) }
            };
            let w_high = high(w)?;
            let n_high = if n >= 0 { Some(high(n)?) } else { None };
            if libc::dup2(w_high, ns::STATUS_FD) < 0 {
                return Err(std::io::Error::last_os_error());
            }
            if let Some(nh) = n_high {
                if libc::dup2(nh, ns::NET_FD) < 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = cmd.spawn().map_err(|e| fail(format!("starting helper {}: {e}", helper.display())))?;
    drop(status_w); // keep only the helper's copy, so EOF arrives when it is gone
    // The same for the socket pair; then wait for the sandbox's socket.
    let proxy = net.map(|(ours, theirs)| {
        drop(theirs);
        proxy::Proxy::start(ours, hosts.to_vec())
    });
    let stdout = capture(child.stdout.take().expect("piped"), max_output);
    let stderr = capture(child.stderr.take().expect("piped"), max_output);
    let status_reader = std::thread::spawn(move || {
        let mut s = String::new();
        let _ = std::fs::File::from(status_r).read_to_string(&mut s);
        s
    });

    // Watchdog: at the wall-clock limit, or when the workspace has grown past
    // its limit, kill the whole cgroup in one step. The workspace is measured
    // every 50 ms, or ten times as long as one measurement takes, so a large
    // workspace does not spend much of a core on being measured (D-117).
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let cg_for_watchdog = cg.clone();
    let disk = disk.clone();
    let watchdog = std::thread::spawn(move || {
        let deadline = start + timeout;
        let mut interval = DISK_INTERVAL_MIN;
        loop {
            let now = Instant::now();
            if now >= deadline {
                let _ = cg_for_watchdog.kill();
                return Some(Stopped::Timeout);
            }
            match done_rx.recv_timeout(interval.min(deadline - now)) {
                Err(mpsc::RecvTimeoutError::Timeout) => {}
                _ => return None,
            }
            let t = Instant::now();
            let used = disk_usage(&disk.dir);
            if used > disk.max_bytes {
                let _ = cg_for_watchdog.kill();
                return Some(Stopped::Disk(used));
            }
            interval = DISK_INTERVAL_MIN.max(t.elapsed() * 10);
        }
    });
    let wait = child.wait();
    let _ = done_tx.send(());
    let stopped = watchdog.join().unwrap_or(None);
    let timed_out = stopped == Some(Stopped::Timeout);
    let oom = cg.oom_killed();
    let pids_hit = cg.pids_limit_hit();
    // Anything still in the group (there should be nothing once PID 1 is
    // gone) is killed, and the group is removed.
    let cleanup = cg.destroy();
    let (out, out_cut) = stdout.join().unwrap_or_default();
    let (err, err_cut) = stderr.join().unwrap_or_default();
    let report = status_reader.join().unwrap_or_default();
    // Every process of the command is gone; close what it left open.
    let network = proxy.map(proxy::Proxy::finish);
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
        stderr_text.push_str("\n[norn] the command hit limits.memory_mb and was killed by the OOM killer\n");
    }
    if pids_hit {
        stderr_text.push_str("\n[norn] the command hit limits.pids; some fork() calls failed\n");
    }
    if let Some(net) = &network {
        stderr_text.push_str(&network_notes(net, hosts));
    }
    let out = ExecOutput {
        exit_code,
        signal,
        timed_out,
        stdout: out,
        stderr: stderr_text,
        truncated: out_cut || err_cut,
        duration_ms: start.elapsed().as_millis() as u64,
        reverted: Vec::new(),
        network,
    };
    Ok((out, stopped))
}

/// Lines for the command's stderr about connections the proxy refused or
/// could not make, so that the agent (which reads stderr) learns why a
/// download failed and what is allowed (as D-146 does for refused calls).
fn network_notes(net: &proxy::Report, hosts: &[(String, crate::policy::HostRule)]) -> String {
    let mut seen: Vec<(String, u16)> = Vec::new();
    let mut out = String::new();
    for c in &net.connections {
        if (c.allowed && c.error.is_none()) || seen.contains(&(c.host.clone(), c.port)) || seen.len() >= 5 {
            continue;
        }
        seen.push((c.host.clone(), c.port));
        let why = match (&c.error, c.allowed) {
            (Some(e), _) => e.clone(),
            (None, _) => c.rule.clone(),
        };
        out.push_str(&format!("\n[norn] network: {}:{} was not reached: {why}", c.host, c.port));
    }
    if !out.is_empty() {
        let allowed: Vec<&str> = hosts.iter().map(|(t, _)| t.split_once(' ').map(|x| x.1).unwrap_or(t).trim_matches('"')).collect();
        out.push_str(&format!("\n[norn] network.allow lists: {}\n", allowed.join(", ")));
    }
    if net.dropped > 0 {
        out.push_str(&format!("[norn] network: {} more connections were not recorded\n", net.dropped));
    }
    out
}

/// Check, at startup, that this machine can run the sandbox. Returns every
/// problem found, each with what to do about it (CLAUDE.md, "Portability").
///
/// Whether a user namespace can really be used is found out by trying, in
/// a forked child (`ns::probe_in_child`). Reading the kernel's switches is
/// not enough: on Ubuntu the switch `apparmor_restrict_unprivileged_userns`
/// can be on and norn still be allowed, through its AppArmor profile
/// (D-093). `helper` is only the program named in the `Command` that does
/// the fork; it is never started.
pub fn check_host(helper: &Path) -> Vec<String> {
    let mut problems = Vec::new();
    let read = |p: &str| std::fs::read_to_string(p).ok().map(|s| s.trim().to_string());
    let uid_map = format!("{0} {0} 1", nix::unistd::getuid().as_raw()).into_bytes();
    let gid_map = format!("{0} {0} 1", nix::unistd::getgid().as_raw()).into_bytes();
    let mut probe = Command::new(helper);
    probe.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::piped());
    // SAFETY: the closure runs in the forked child and never returns; it
    // makes raw system calls only and touches nothing but the two byte
    // strings prepared above (see `probe_in_child` for what it relies on).
    unsafe {
        probe.pre_exec(move || ns::probe_in_child(&uid_map, &gid_map));
    }
    let failure = match probe.output() {
        Ok(out) if out.status.code() == Some(0) => None,
        Ok(out) => {
            let step = out.status.code().and_then(|c| ns::PROBE_STEPS.get((c as usize).wrapping_sub(1))).copied().unwrap_or("the probe");
            let errno = String::from_utf8_lossy(&out.stderr).trim().parse::<i32>().ok();
            Some(match errno {
                Some(e) => format!("{step}: {}", std::io::Error::from_raw_os_error(e)),
                None => format!("{step} failed"),
            })
        }
        Err(e) => Some(format!("starting the probe: {e}")),
    };
    if let Some(why) = failure {
        // The probe says that it does not work; the switches say why.
        let mut msg = format!("user namespaces cannot be used ({why})");
        if read("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").as_deref() == Some("1") {
            msg.push_str(
                "\n    This system (Ubuntu 23.10 or newer) only allows them for programs with an AppArmor profile. \
                 Install the one that comes with norn; it allows them for norn alone:\n      \
                 sudo install -m 644 packaging/apparmor/norn /etc/apparmor.d/norn\n      \
                 sudo apparmor_parser -r /etc/apparmor.d/norn\n    \
                 (The profile names /usr/local/bin/norn; change that line if the program is somewhere else.)\n    \
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
