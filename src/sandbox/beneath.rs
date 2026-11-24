//! Opening paths inside the workspace with the kernel doing the confinement.
//!
//! Every open starts from a handle (file descriptor) on the workspace folder
//! and uses `openat2` with `RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS`. The
//! kernel walks the path one step at a time and refuses (with `EXDEV`) any
//! step that would leave the workspace: `..`, an absolute path, a symlink
//! pointing outside, or a `/proc/.../root` "magic link". Because the check and
//! the open are one system call, a symlink swapped in between cannot fool it
//! (design rule 7; `experiments/m2-openat2`).

use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::path::{Path, PathBuf};

use nix::errno::Errno;
use nix::fcntl::{open, openat2, OFlag, OpenHow, ResolveFlag};
use nix::sys::stat::Mode;

/// A handle on the workspace folder. `O_PATH` means "a handle on the place,
/// not opened for reading"; it is only used as the starting point for opens.
pub fn open_root(work: &Path) -> nix::Result<OwnedFd> {
    open(work, OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_CLOEXEC, Mode::empty())
}

/// `openat2(root, path, flags, RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS)`.
/// `extra` adds resolve flags, e.g. `RESOLVE_NO_SYMLINKS` for the revert code.
pub fn open_beneath(root: impl AsFd, path: &str, flags: OFlag, extra: ResolveFlag) -> nix::Result<OwnedFd> {
    // openat2 is stricter than open: with O_PATH it accepts only O_CLOEXEC,
    // O_DIRECTORY and O_NOFOLLOW, and fails with EINVAL otherwise (found when
    // fs.write first ran; see the decision log's Findings).
    let flags = if flags.contains(OFlag::O_PATH) { flags | OFlag::O_CLOEXEC } else { flags | OFlag::O_CLOEXEC | OFlag::O_NOCTTY };
    let how = OpenHow::new()
        .flags(flags)
        .mode(Mode::empty())
        .resolve(ResolveFlag::RESOLVE_BENEATH | ResolveFlag::RESOLVE_NO_MAGICLINKS | extra);
    let path = if path.is_empty() { "." } else { path };
    openat2(root, path, how)
}

/// Where an open file actually is, as the kernel reports it. This reads the
/// name only; the confinement was already done by `openat2`.
pub fn fd_path(fd: impl AsFd) -> std::io::Result<PathBuf> {
    std::fs::read_link(format!("/proc/self/fd/{}", fd.as_fd().as_raw_fd()))
}

/// Workspace-relative form (`a/b`, `""` for the root) of a host path that the
/// kernel resolved beneath `work_real` (the workspace's real path).
pub fn relative_to(work_real: &Path, resolved: &Path) -> Option<String> {
    let rest = resolved.strip_prefix(work_real).ok()?;
    let s = rest.to_str()?;
    Some(s.to_string())
}

/// Did `openat2` refuse because the path leaves the workspace?
pub fn is_escape(e: Errno) -> bool {
    // EXDEV: a step left the starting folder. ELOOP: a magic link was refused
    // (or, with RESOLVE_NO_SYMLINKS, any symlink).
    matches!(e, Errno::EXDEV | Errno::ELOOP)
}

/// Clean a path as text for *policy matching only*: drop `.` and empty parts,
/// apply `..`. Returns `None` for an absolute path or one that climbs above
/// the root. This decides nothing about where the path really leads: that is
/// `openat2`'s job. It only lets deny rules be checked on the name the agent
/// asked for, before the file is opened.
pub fn lexical_rel(path: &str) -> Option<String> {
    if path.starts_with('/') {
        return None;
    }
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                parts.pop()?;
            }
            c => parts.push(c),
        }
    }
    Some(parts.join("/"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn escapes_are_refused_by_the_kernel() {
        let base = std::env::temp_dir().join(format!("norn-beneath-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let ws = base.join("ws");
        std::fs::create_dir_all(ws.join("src")).unwrap();
        std::fs::write(ws.join("src/a.txt"), "in").unwrap();
        std::fs::write(base.join("secret"), "out").unwrap();
        symlink("/etc/passwd", ws.join("abs")).unwrap();
        symlink("../secret", ws.join("rel")).unwrap();
        symlink("src/a.txt", ws.join("good")).unwrap();
        let root = open_root(&ws).unwrap();
        let ok = |p: &str| open_beneath(&root, p, OFlag::O_RDONLY, ResolveFlag::empty());
        assert!(ok("src/a.txt").is_ok());
        assert!(ok("good").is_ok(), "a link that stays inside still works");
        for bad in ["../secret", "/etc/passwd", "abs", "rel", "/proc/self/root/etc/passwd", "src/../../secret"] {
            let e = ok(bad).unwrap_err();
            assert!(is_escape(e), "{bad}: expected EXDEV/ELOOP, got {e:?}");
        }
        let real = std::fs::canonicalize(&ws).unwrap();
        let fd = ok("good").unwrap();
        assert_eq!(relative_to(&real, &fd_path(&fd).unwrap()).as_deref(), Some("src/a.txt"));
        std::fs::remove_dir_all(&base).unwrap();
    }

    #[test]
    fn lexical_cleanup() {
        assert_eq!(lexical_rel(".").as_deref(), Some(""));
        assert_eq!(lexical_rel("./src//a.py").as_deref(), Some("src/a.py"));
        assert_eq!(lexical_rel("src/../tests/t.py").as_deref(), Some("tests/t.py"));
        assert_eq!(lexical_rel("../x"), None);
        assert_eq!(lexical_rel("/etc/passwd"), None);
    }
}
