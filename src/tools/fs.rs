//! `fs.read`, `fs.list` and `fs.write`, run by the trusted runtime on the
//! workspace folder.
//!
//! Every path is opened with `openat2(RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS)`
//! from a handle on the workspace (`sandbox::beneath`), so the kernel refuses
//! anything that leaves it (design rule 7). The opened handle is then used for
//! the actual read or write, so the file checked is the file used: a symlink
//! swapped in between cannot redirect the operation (no TOCTOU gap).
//!
//! After the open, the policy is checked again on the path the kernel
//! actually resolved (v1 design, "Deny paths"): a symlink named `notes` that
//! points at a denied file is refused by the rule for that file.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::{AsFd, OwnedFd};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use nix::errno::Errno;
use nix::fcntl::{openat, renameat, OFlag, ResolveFlag};
use nix::sys::stat::{fstat, fstatat, Mode, SFlag};
use nix::unistd::{unlinkat, UnlinkatFlags};

use crate::blob::content_hash;
use crate::sandbox::beneath;
use crate::tools::{ListEntry, ToolError};

/// The result of opening a path the agent named.
pub enum Opened {
    /// It is inside the workspace. `rel` is where the kernel says it is.
    Inside { fd: OwnedFd, rel: String },
    /// The kernel refused because the path leads outside.
    Outside(Errno),
}

/// The workspace's real path (symlinks resolved), used to turn the kernel's
/// answer for an fd back into a workspace-relative path.
pub fn work_real(work: &Path) -> Result<PathBuf, ToolError> {
    std::fs::canonicalize(work).map_err(|e| ToolError::new(format!("workspace: {e}")))
}

/// Open `path` beneath the workspace for reading (`dir`: as a folder).
pub fn open_for_read(work: &Path, path: &str, dir: bool) -> Result<Opened, ToolError> {
    let root = beneath::open_root(work).map_err(|e| ToolError::new(format!("workspace: {e}")))?;
    // O_NONBLOCK: opening a FIFO for reading normally blocks until a writer
    // appears, and a command can plant a FIFO in the workspace. With this
    // flag the open returns at once; `read` then refuses anything that is not
    // a regular file. For regular files and folders the flag changes nothing.
    // (Bug B-005 in docs/bugs.md.)
    let flags = if dir { OFlag::O_RDONLY | OFlag::O_DIRECTORY } else { OFlag::O_RDONLY | OFlag::O_NONBLOCK };
    match beneath::open_beneath(&root, path, flags, ResolveFlag::empty()) {
        Ok(fd) => Ok(Opened::Inside { rel: resolved_rel(work, &fd)?, fd }),
        Err(e) if beneath::is_escape(e) => Ok(Opened::Outside(e)),
        Err(e) => Err(errno(path, e)),
    }
}

fn resolved_rel(work: &Path, fd: impl AsFd) -> Result<String, ToolError> {
    let real = work_real(work)?;
    let at = beneath::fd_path(fd).map_err(|e| ToolError::new(format!("reading fd path: {e}")))?;
    beneath::relative_to(&real, &at)
        .ok_or_else(|| ToolError::new(format!("kernel resolved to {} outside the workspace", at.display())))
}

pub fn read(fd: OwnedFd, rel: &str, max_bytes: u64) -> Result<String, ToolError> {
    let st = fstat(&fd).map_err(|e| errno(rel, e))?;
    let kind = SFlag::from_bits_truncate(st.st_mode) & SFlag::S_IFMT;
    if kind == SFlag::S_IFDIR {
        return Err(ToolError::new(format!("`{}` is a directory; use fs.list", show(rel))));
    }
    if kind != SFlag::S_IFREG {
        // FIFOs, sockets, devices: reading them can block or never end.
        return Err(ToolError::new(format!("`{}` is not a regular file", show(rel))));
    }
    if st.st_size as u64 > max_bytes {
        return Err(ToolError::new(format!(
            "`{}` is {} bytes, more than the {max_bytes}-byte limit (limits.max_output_kb)",
            show(rel),
            st.st_size
        )));
    }
    let mut bytes = Vec::new();
    File::from(fd).take(max_bytes + 1).read_to_end(&mut bytes).map_err(|e| io(rel, e))?;
    String::from_utf8(bytes).map_err(|_| ToolError::new(format!("`{}` is not UTF-8 text", show(rel))))
}

pub fn list(fd: OwnedFd, rel: &str) -> Result<Vec<ListEntry>, ToolError> {
    let mut dir = nix::dir::Dir::from_fd(fd).map_err(|e| errno(rel, e))?;
    let mut names = Vec::new();
    for entry in dir.iter() {
        let entry = entry.map_err(|e| errno(rel, e))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name != "." && name != ".." {
            names.push(name);
        }
    }
    let mut out = Vec::new();
    for name in names {
        // fstatat with AT_SYMLINK_NOFOLLOW describes the entry itself.
        let st = fstatat(&dir, name.as_str(), nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW).map_err(|e| errno(rel, e))?;
        let kind = match SFlag::from_bits_truncate(st.st_mode) & SFlag::S_IFMT {
            SFlag::S_IFDIR => "dir",
            SFlag::S_IFREG => "file",
            SFlag::S_IFLNK => "symlink",
            _ => "other",
        };
        out.push(ListEntry { name, kind: kind.into(), size: st.st_size as u64 });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// The folder an `fs.write` goes into, opened beneath the workspace, plus
/// the file name inside it.
pub struct WriteTarget {
    /// The file's folder, or, if that does not exist yet, the deepest folder
    /// on the way to it that does.
    pub parent: OwnedFd,
    /// Folder names between `parent` and the file that do not exist yet.
    /// `write` makes them; nothing is made before the intent is logged (D-084).
    pub missing: Vec<String>,
    pub name: String,
    /// Workspace-relative path of the target, as resolved by the kernel.
    pub rel: String,
}

impl WriteTarget {
    /// Workspace-relative paths of the folders `write` will make, outermost first.
    pub fn new_dirs(&self) -> Vec<String> {
        let parts: Vec<&str> = self.rel.split('/').collect();
        let dirs = parts.len() - 1; // the last part is the file name
        (dirs - self.missing.len()..dirs).map(|i| parts[..=i].join("/")).collect()
    }
}

pub enum OpenedParent {
    Inside(WriteTarget),
    Outside(Errno),
}

/// Split `path` into folder + name, and open the folder beneath the workspace.
/// The name must be a plain name: not empty, `.` or `..`.
pub fn open_for_write(work: &Path, path: &str) -> Result<OpenedParent, ToolError> {
    let trimmed = path.trim_end_matches('/');
    let (dir, name) = match trimmed.rsplit_once('/') {
        Some((d, n)) => (if d.is_empty() { "/" } else { d }, n),
        None => (".", trimmed),
    };
    if name.is_empty() || name == "." || name == ".." {
        return Err(ToolError::new(format!("`{path}` does not name a file")));
    }
    let root = beneath::open_root(work).map_err(|e| ToolError::new(format!("workspace: {e}")))?;
    let target = |parent: OwnedFd, missing: Vec<String>| -> Result<OpenedParent, ToolError> {
        let mut parts: Vec<String> = Vec::new();
        let dir_rel = resolved_rel(work, &parent)?;
        if !dir_rel.is_empty() {
            parts.push(dir_rel);
        }
        parts.extend(missing.iter().cloned());
        parts.push(name.to_string());
        Ok(OpenedParent::Inside(WriteTarget { parent, missing, name: name.to_string(), rel: parts.join("/") }))
    };
    let open = |p: &str| beneath::open_beneath(&root, p, OFlag::O_PATH | OFlag::O_DIRECTORY, ResolveFlag::empty());
    match open(dir) {
        Ok(parent) => return target(parent, Vec::new()),
        Err(e) if beneath::is_escape(e) => return Ok(OpenedParent::Outside(e)),
        Err(Errno::ENOENT) => {}
        Err(Errno::ENOTDIR) => return Err(ToolError::new(format!("the folder for `{path}` is not a folder"))),
        Err(e) => return Err(errno(path, e)),
    }
    // The folder does not exist. Find the deepest folder on the way that
    // does, still asking the kernel (so a symlink or `..` in the existing
    // part is confined as always). What is left over are the folders to
    // make. They must be plain names: `..` in a part that does not exist
    // cannot be resolved by anyone.
    let comps: Vec<&str> = dir.split('/').filter(|c| !c.is_empty() && *c != ".").collect();
    for k in (0..comps.len()).rev() {
        match open(&comps[..k].join("/")) {
            Ok(parent) => {
                if comps[k..].contains(&"..") {
                    break;
                }
                return target(parent, comps[k..].iter().map(|c| c.to_string()).collect());
            }
            Err(e) if beneath::is_escape(e) => return Ok(OpenedParent::Outside(e)),
            Err(Errno::ENOENT) => continue,
            Err(Errno::ENOTDIR) => return Err(ToolError::new(format!("`{}` is not a folder", comps[..k].join("/")))),
            Err(e) => return Err(errno(path, e)),
        }
    }
    Err(ToolError::new(format!("the folder for `{path}` does not exist")))
}

/// Hash of the file the write would replace, or `None` if there is none.
/// This is the "before" fingerprint in the `fs.write` intent (D-016).
/// A symlink or any other non-regular file there is refused: replacing it
/// would make reconciliation ambiguous, and following it is never wanted.
pub fn current_hash(t: &WriteTarget) -> Result<Option<String>, ToolError> {
    if !t.missing.is_empty() {
        return Ok(None); // its folder does not exist yet, so neither does the file
    }
    match openat(&t.parent, t.name.as_str(), OFlag::O_RDONLY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC | OFlag::O_NONBLOCK, Mode::empty()) {
        Err(Errno::ENOENT) => Ok(None),
        Err(Errno::ELOOP) => Err(ToolError::new(format!("`{}` is a symlink; fs.write only replaces regular files", t.rel))),
        Err(e) => Err(errno(&t.rel, e)),
        Ok(fd) => {
            let st = fstat(&fd).map_err(|e| errno(&t.rel, e))?;
            if SFlag::from_bits_truncate(st.st_mode) & SFlag::S_IFMT != SFlag::S_IFREG {
                return Err(ToolError::new(format!("`{}` exists and is not a regular file", t.rel)));
            }
            let mut bytes = Vec::new();
            File::from(fd).read_to_end(&mut bytes).map_err(|e| io(&t.rel, e))?;
            Ok(Some(content_hash(&bytes)))
        }
    }
}

/// Temporary file name used while writing `call_id`'s new content.
pub fn temp_name(call_id: u64) -> String {
    format!(".agentos-tmp-{call_id}")
}

/// Atomic replace, entirely through the parent folder's handle: create a
/// temp file (`O_EXCL | O_NOFOLLOW`, so it cannot be a planted link), write,
/// fsync, then `renameat` over the target. `renameat` replaces the name
/// itself and never follows a symlink there.
///
/// If folders are missing they are made first, one `mkdirat` each through
/// the handle of the folder above. `mkdirat` fails if the name exists in any
/// form (also as a symlink), and the new folder is then opened with
/// `O_NOFOLLOW`, so this never walks through a link. If the write fails
/// afterwards, the folders are removed again: FAILED means nothing changed.
pub fn write(t: &WriteTarget, content: &str, call_id: u64) -> Result<u64, ToolError> {
    if t.missing.is_empty() {
        return write_in(&t.parent, &t.name, &t.rel, content, call_id);
    }
    // (folder above, name) of each folder made, to undo in reverse order.
    let mut made: Vec<(OwnedFd, &str)> = Vec::new();
    let result = (|| {
        let mut cur = t.parent.try_clone().map_err(|e| io(&t.rel, e))?;
        for comp in &t.missing {
            match nix::sys::stat::mkdirat(&cur, comp.as_str(), Mode::from_bits_truncate(0o755)) {
                Ok(()) => {}
                Err(Errno::EEXIST) => return Err(ToolError::new(format!("`{comp}` on the way to `{}` exists and is not a folder", t.rel))),
                Err(e) => return Err(errno(&t.rel, e)),
            }
            let next = openat(&cur, comp.as_str(), OFlag::O_PATH | OFlag::O_DIRECTORY | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC, Mode::empty());
            made.push((cur, comp.as_str()));
            cur = next.map_err(|e| errno(&t.rel, e))?;
        }
        write_in(&cur, &t.name, &t.rel, content, call_id)
    })();
    if result.is_err() {
        for (above, name) in made.iter().rev() {
            let _ = unlinkat(above, *name, UnlinkatFlags::RemoveDir);
        }
    }
    result
}

/// The atomic replace itself, in a folder that exists.
fn write_in(parent: &OwnedFd, name: &str, rel: &str, content: &str, call_id: u64) -> Result<u64, ToolError> {
    struct T<'a> {
        parent: &'a OwnedFd,
        name: &'a str,
        rel: &'a str,
    }
    let t = T { parent, name, rel };
    let tmp = temp_name(call_id);
    // Keep the mode of the file being replaced; new files get 0644 (minus umask).
    let mode = match fstatat(t.parent, t.name, nix::fcntl::AtFlags::AT_SYMLINK_NOFOLLOW) {
        Ok(st) => Mode::from_bits_truncate(st.st_mode & 0o7777),
        Err(_) => Mode::from_bits_truncate(0o644),
    };
    let result = (|| -> Result<(), ToolError> {
        let fd = openat(
            t.parent,
            tmp.as_str(),
            OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC,
            mode,
        )
        .map_err(|e| errno(t.rel, e))?;
        let mut f = File::from(fd);
        f.write_all(content.as_bytes()).map_err(|e| io(t.rel, e))?;
        f.set_permissions(std::fs::Permissions::from_mode(mode.bits())).map_err(|e| io(t.rel, e))?;
        f.sync_all().map_err(|e| io(t.rel, e))?;
        renameat(t.parent, tmp.as_str(), t.parent, t.name).map_err(|e| errno(t.rel, e))
    })();
    if result.is_err() {
        let _ = unlinkat(t.parent, tmp.as_str(), UnlinkatFlags::NoRemoveDir);
    }
    result.map(|()| content.len() as u64)
}

fn show(rel: &str) -> &str {
    if rel.is_empty() {
        "."
    } else {
        rel
    }
}

fn errno(rel: &str, e: Errno) -> ToolError {
    ToolError::new(format!("`{}`: {}", show(rel), e.desc()))
}

fn io(rel: &str, e: std::io::Error) -> ToolError {
    ToolError::new(format!("`{}`: {e}", show(rel)))
}
