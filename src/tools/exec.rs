//! `exec`: run an allowed command as an argv array, with `execve`, never
//! through a shell (design rule 8). The command itself runs in the sandbox
//! (`crate::sandbox`); this file finds the programs and reads their output.

use std::collections::BTreeSet;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::policy::Policy;

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
            let real = match found {
                Some(p) => std::fs::canonicalize(&p)?,
                None => bail!("allowed program `{prog}` was not found on PATH"),
            };
            // The sandbox sees only sandbox.ro_mounts, so the program must live there.
            let mounts: Vec<PathBuf> = policy.sandbox.ro_mounts.iter().map(PathBuf::from).collect();
            if !crate::sandbox::visible_in_sandbox(&real, &mounts) {
                bail!(
                    "allowed program `{prog}` is at {}, which is not under any sandbox.ro_mounts entry ({}); \
                     use the policy preset for this system (policies/fhs.toml or policies/nix.toml)",
                    real.display(),
                    policy.sandbox.ro_mounts.join(", ")
                );
            }
            resolved.push((prog.clone(), real));
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
