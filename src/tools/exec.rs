//! `exec`: run an allowed command as an argv array, with `execve`, never
//! through a shell (design rule 8).
//!
//! M1 runs the command directly on the host, in the workspace folder, with a
//! cleared environment. M2 moves it into the sandbox.

use std::collections::BTreeSet;
use std::io::Read;
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{bail, Result};

use crate::policy::Policy;
use crate::tools::{ExecOutput, ToolError};

/// The allowed programs, each found on the host before anything runs.
#[derive(Debug, Clone)]
pub struct Programs {
    /// Program name (argv[0] in the policy) -> real path on the host.
    resolved: Vec<(String, PathBuf)>,
}

impl Programs {
    /// Find each program the policy allows on the host `PATH` and resolve
    /// symlinks to its real location. The sandbox `PATH` is built from these
    /// real locations, so no distro layout is assumed in the code.
    pub fn resolve(policy: &Policy, host_path: &str) -> Result<Programs> {
        let mut resolved = Vec::new();
        for prog in policy.programs() {
            if prog.contains('/') {
                bail!("exec rule program `{prog}` must be a bare name, not a path");
            }
            let found = host_path
                .split(':')
                .filter(|d| !d.is_empty())
                .map(|d| Path::new(d).join(&prog))
                .find(|p| is_executable(p));
            match found {
                Some(p) => resolved.push((prog.clone(), std::fs::canonicalize(&p)?)),
                None => bail!("allowed program `{prog}` was not found on PATH"),
            }
        }
        Ok(Programs { resolved })
    }

    pub fn lookup(&self, name: &str) -> Option<&Path> {
        self.resolved.iter().find(|(n, _)| n == name).map(|(_, p)| p.as_path())
    }

    /// `PATH` for the command: the folders holding the resolved programs.
    pub fn search_path(&self) -> String {
        let dirs: BTreeSet<String> = self
            .resolved
            .iter()
            .filter_map(|(_, p)| p.parent().map(|d| d.display().to_string()))
            .collect();
        dirs.into_iter().collect::<Vec<_>>().join(":")
    }
}

fn is_executable(p: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(p).map(|m| m.is_file() && m.permissions().mode() & 0o111 != 0).unwrap_or(false)
}

/// Everything needed to run one command.
pub struct ExecRequest<'a> {
    pub program: &'a Path,
    pub argv: &'a [String],
    pub cwd: &'a Path,
    pub env: Vec<(String, String)>,
    pub timeout: Duration,
    pub max_output: usize,
}

/// Run a command directly (no sandbox) with a watchdog thread.
pub fn run_direct(req: &ExecRequest) -> Result<ExecOutput, ToolError> {
    let start = Instant::now();
    let mut cmd = Command::new(req.program);
    cmd.arg0(&req.argv[0])
        .args(&req.argv[1..])
        .env_clear()
        .envs(req.env.iter().map(|(k, v)| (k, v)))
        .current_dir(req.cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = cmd.spawn().map_err(|e| ToolError::new(format!("cannot start `{}`: {e}", req.argv[0])))?;
    let pid = child.id();
    let stdout = capture(child.stdout.take().expect("piped"), req.max_output);
    let stderr = capture(child.stderr.take().expect("piped"), req.max_output);

    // Watchdog: waits for either "done" or the timeout. On timeout it kills
    // the command. (M1 kills by pid, which has a tiny race with pid reuse;
    // M2 kills the whole cgroup instead.)
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let timeout = req.timeout;
    let watchdog = std::thread::spawn(move || {
        if done_rx.recv_timeout(timeout).is_err() {
            let _ = nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid as i32), nix::sys::signal::Signal::SIGKILL);
            return true;
        }
        false
    });
    let status = child.wait().map_err(|e| ToolError::new(format!("waiting for command: {e}")))?;
    let _ = done_tx.send(());
    let timed_out = watchdog.join().unwrap_or(false);
    let (out, out_cut) = stdout.join().unwrap_or_default();
    let (err, err_cut) = stderr.join().unwrap_or_default();
    Ok(ExecOutput {
        exit_code: status.code(),
        signal: status.signal(),
        timed_out,
        stdout: out,
        stderr: err,
        truncated: out_cut || err_cut,
        duration_ms: start.elapsed().as_millis() as u64,
        reverted: Vec::new(),
    })
}

/// Read a pipe on its own thread, keeping at most `max` bytes. The rest is
/// read and thrown away, so a chatty command never blocks on a full pipe.
pub fn capture<R: Read + Send + 'static>(mut r: R, max: usize) -> std::thread::JoinHandle<(String, bool)> {
    std::thread::spawn(move || {
        let mut kept = Vec::new();
        let mut buf = [0u8; 8192];
        let mut cut = false;
        loop {
            match r.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let room = max.saturating_sub(kept.len());
                    if n > room {
                        cut = true;
                    }
                    kept.extend_from_slice(&buf[..n.min(room)]);
                }
            }
        }
        (String::from_utf8_lossy(&kept).into_owned(), cut)
    })
}
