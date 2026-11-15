//! `exec`: run an allowed command as an argv array, with `execve`, never
//! through a shell (design rule 8). The command itself runs in the sandbox
//! (`crate::sandbox`); this file finds the programs and reads their output.

use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{bail, Result};

use crate::policy::Policy;

/// The allowed programs, each found on the host before anything runs.
#[derive(Debug, Clone)]
pub struct Programs {
    /// Program name (argv[0] in the policy) -> real path on the host.
    /// The third field is where on the host `PATH` it was found (0 = first).
    resolved: Vec<(String, PathBuf, usize)>,
    /// Folders of the helper programs (`exec.helpers`), with the same position.
    helper_dirs: Vec<(usize, PathBuf)>,
    /// Things the user should be told before the run starts (D-101).
    pub notes: Vec<String>,
}

impl Programs {
    /// Find each program the policy allows on the host `PATH`. The sandbox
    /// `PATH` is built from where they were found, so no distro layout is
    /// assumed in the code.
    ///
    /// The path used is the one found on `PATH`, with only its *folder*
    /// resolved, as long as it and every symlink step behind it can be seen
    /// in the sandbox. Otherwise it is the program's real location, with all
    /// symlinks resolved. Keeping the found name matters for a Python
    /// virtualenv: `venv/bin/python3` is a symlink to the system's Python,
    /// and Python only uses the virtualenv's packages when it is started
    /// through that link (D-091).
    pub fn resolve(policy: &Policy, host_path: &str) -> Result<Programs> {
        let mut resolved = Vec::new();
        let mut helper_dirs = Vec::new();
        let mut notes = Vec::new();
        let mounts: Vec<PathBuf> = policy.sandbox.ro_mounts.iter().map(PathBuf::from).collect();
        // The programs the agent may ask for, then the helper programs that
        // those start themselves (`exec.helpers`, D-112). Both are found
        // the same way; only the first kind can be looked up by a call.
        let allowed = policy.programs();
        let helpers: Vec<String> = policy.exec.helpers.iter().filter(|h| !allowed.contains(h)).cloned().collect();
        for (prog, helper) in allowed.iter().map(|p| (p, false)).chain(helpers.iter().map(|h| (h, true))) {
            let what = if helper { "helper program" } else { "allowed program" };
            if prog.contains('/') {
                bail!("exec rule program `{prog}` must be a bare name, not a path");
            }
            let found = host_path
                .split(':')
                .filter(|d| !d.is_empty())
                .enumerate()
                .map(|(i, d)| (i, Path::new(d).join(prog)))
                .find(|(_, p)| is_executable(p));
            let Some((position, found)) = found else {
                bail!("{what} `{prog}` was not found on PATH");
            };
            let real = std::fs::canonicalize(&found)?;
            // The sandbox sees only sandbox.ro_mounts, so the program must live there.
            if !crate::sandbox::visible_in_sandbox(&real, &mounts) {
                bail!(
                    "{what} `{prog}` is at {}, which the sandbox cannot see: it is not under any \
                     sandbox.ro_mounts entry ({}).\n  \
                     If this is the wrong kind of system for the policy, use the other preset (policies/fhs.toml or policies/nix.toml).\n  \
                     If the program is in a virtualenv or another folder of your own, add that folder to sandbox.ro_mounts in a copy of the policy.",
                    real.display(),
                    policy.sandbox.ro_mounts.join(", ")
                );
            }
            let as_found = link_chain_visible(&found, &mounts).unwrap_or_else(|| real.clone());
            if helper {
                if let Some(dir) = as_found.parent() {
                    helper_dirs.push((position, dir.to_path_buf()));
                }
                continue;
            }
            // A mounted folder that has this program in its own `bin`, while
            // that `bin` is not on PATH at all: almost always a virtualenv
            // that was mounted but not activated. The run would use another
            // program and fail in ways that do not point here.
            //
            // "Not on PATH at all" is the test, not "a different one was
            // found". On Ubuntu `/usr` is a mount and `/usr/bin` holds a
            // `pytest` too; with a virtualenv active that one is on PATH
            // and merely comes later, which is exactly right (bug B-014).
            for m in &mounts {
                let own_bin = m.join("bin");
                let own = own_bin.join(prog);
                let same = |a: &Path, b: &Path| std::fs::canonicalize(a).ok() == std::fs::canonicalize(b).ok();
                let on_path = host_path.split(':').filter(|d| !d.is_empty()).any(|d| same(Path::new(d), &own_bin));
                if is_executable(&own) && !on_path && !same(&own, &as_found) && !same(&own, &real) {
                    notes.push(format!(
                        "sandbox.ro_mounts has {}, which holds its own `{prog}` ({}), but its bin folder is not on PATH; the `{prog}` in use is {}. \
                         If that folder is a virtualenv, activate it before starting agentos (its bin folder must come first on PATH); \
                         otherwise its packages will not be used.",
                        m.display(),
                        own.display(),
                        as_found.display()
                    ));
                }
            }
            resolved.push((prog.clone(), as_found, position));
        }
        Ok(Programs { resolved, helper_dirs, notes })
    }

    pub fn lookup(&self, name: &str) -> Option<&Path> {
        self.resolved.iter().find(|(n, _, _)| n == name).map(|(_, p, _)| p.as_path())
    }

    /// `PATH` for the command: the folders holding the resolved programs.
    pub fn search_path(&self) -> String {
        // In the order of the host's PATH, so that what came first there
        // comes first here (a virtualenv's `bin` before `/usr/bin`).
        let mut dirs: Vec<(usize, PathBuf)> = self
            .resolved
            .iter()
            .filter_map(|(_, p, position)| p.parent().map(|d| (*position, d.to_path_buf())))
            .chain(self.helper_dirs.iter().cloned())
            .collect();
        dirs.sort();
        let mut out: Vec<String> = Vec::new();
        for (_, dir) in dirs {
            let dir = dir.display().to_string();
            if !out.contains(&dir) {
                out.push(dir);
            }
        }
        out.join(":")
    }
}

/// `found` with its folder resolved, if that path and every symlink step
/// from it to the real file is under a read-only mount. `None` if any step
/// is not (the caller then uses the fully resolved path, which is).
fn link_chain_visible(found: &Path, mounts: &[PathBuf]) -> Option<PathBuf> {
    // Resolve the folder, keep the name: `/bin/x` becomes `/usr/bin/x`, but
    // `venv/bin/python3` stays the link it is.
    let tidy = |p: &Path| Some(std::fs::canonicalize(p.parent()?).ok()?.join(p.file_name()?));
    let start = tidy(found)?;
    let mut at = start.clone();
    for _ in 0..40 {
        if !crate::sandbox::visible_in_sandbox(&at, mounts) {
            return None;
        }
        if !std::fs::symlink_metadata(&at).ok()?.file_type().is_symlink() {
            return Some(start);
        }
        // A relative link target is relative to the link's own folder.
        at = tidy(&at.parent()?.join(std::fs::read_link(&at).ok()?))?;
    }
    None // a loop of links
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
