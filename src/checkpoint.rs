//! Checkpoints: full copies of the workspace folder (D-007).
//!
//! A checkpoint is made with `cp -a --reflink=auto`, which shares disk blocks
//! with the original on btrfs and XFS and falls back to a real copy elsewhere.
//! It is written under a temporary name and renamed into place, so a
//! half-made copy never carries a real checkpoint name.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Name of the n-th checkpoint: `cp-0000`, `cp-0001`, ...
pub fn name(n: u64) -> String {
    format!("cp-{n:04}")
}

/// Copy `work` to `<checkpoints>/<name>` and return the new folder's path.
pub fn create(work: &Path, checkpoints: &Path, name: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(checkpoints)?;
    let final_path = checkpoints.join(name);
    if final_path.exists() {
        bail!("checkpoint {name} already exists");
    }
    let tmp = checkpoints.join(format!(".tmp-{name}"));
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?; // left over from an earlier crash
    }
    run_cp(work, &tmp)?;
    std::fs::rename(&tmp, &final_path).with_context(|| format!("renaming {} into place", tmp.display()))?;
    Ok(final_path)
}

/// Number in a checkpoint name (`cp-0007` -> 7).
pub fn number(name: &str) -> Option<u64> {
    name.strip_prefix("cp-")?.parse().ok()
}

/// Folder used while restoring; a sibling of `work/`.
fn restore_tmp(work: &Path) -> PathBuf {
    work.with_file_name("work.restore-tmp")
}

/// Make `work` equal to checkpoint `cp` (roll back).
///
/// The checkpoint is copied to a temporary folder next to `work`, then the
/// two folders are swapped with one `renameat2(RENAME_EXCHANGE)` call, which
/// the kernel does atomically. A crash before the swap leaves `work`
/// untouched; a crash after it leaves `work` fully restored. Either way,
/// running the restore again gives the same result, which is what makes
/// recovery safe to interrupt (crash point P8).
pub fn restore(work: &Path, cp: &Path) -> Result<()> {
    let tmp = restore_tmp(work);
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?; // left over from an interrupted restore
    }
    run_cp(cp, &tmp)?;
    crate::faults::point("P8", None);
    nix::fcntl::renameat2(
        nix::fcntl::AT_FDCWD,
        &tmp,
        nix::fcntl::AT_FDCWD,
        work,
        nix::fcntl::RenameFlags::RENAME_EXCHANGE,
    )
    .with_context(|| format!("swapping {} into place", tmp.display()))?;
    crate::faults::point("P8", None);
    // `tmp` now holds the old workspace contents.
    std::fs::remove_dir_all(&tmp)?;
    Ok(())
}

/// Remove half-made things a crash can leave: `.tmp-*` checkpoint copies and
/// the restore folder. Returns nothing; missing things are fine.
pub fn remove_leftovers(work: &Path, checkpoints: &Path) -> Result<()> {
    let tmp = restore_tmp(work);
    if tmp.exists() {
        std::fs::remove_dir_all(&tmp)?;
    }
    if checkpoints.exists() {
        for e in std::fs::read_dir(checkpoints)? {
            let e = e?;
            if e.file_name().to_string_lossy().starts_with(".tmp-") {
                std::fs::remove_dir_all(e.path())?;
            }
        }
    }
    Ok(())
}

/// Names of the checkpoint folders on disk, sorted.
pub fn on_disk(checkpoints: &Path) -> Result<Vec<String>> {
    let mut out = Vec::new();
    if checkpoints.exists() {
        for e in std::fs::read_dir(checkpoints)? {
            let name = e?.file_name().to_string_lossy().into_owned();
            if number(&name).is_some() {
                out.push(name);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// `cp -a --reflink=auto SRC DST`, where DST does not exist yet.
pub fn run_cp(src: &Path, dst: &Path) -> Result<()> {
    let out = Command::new("cp")
        .arg("-a")
        .arg("--reflink=auto")
        .arg("--")
        .arg(src)
        .arg(dst)
        .output()
        .context("running cp")?;
    if !out.status.success() {
        bail!("cp -a {} {} failed: {}", src.display(), dst.display(), String::from_utf8_lossy(&out.stderr).trim());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Reverting changes outside the write globs (D-009).
//
// Code run by `exec` may write anywhere in /work. Afterwards, every path that
// the write globs do NOT allow is made equal to the checkpoint again.
//
// These operations run in the trusted runtime on a folder the agent shaped,
// so they must never follow a symlink the agent planted (for example `docs`
// replaced by a link to the home folder; restoring `docs/README` would then
// write into the home folder). Each change is therefore made through a handle
// on its parent folder, opened with openat2(RESOLVE_BENEATH |
// RESOLVE_NO_SYMLINKS): if any folder on the way is a symlink, the open fails
// instead of following it. No sandboxed process is alive at this point (the
// exec's cgroup has been destroyed), so nothing can swap paths meanwhile.
// ---------------------------------------------------------------------------

use std::collections::BTreeMap;
use std::os::fd::{AsRawFd, OwnedFd};
use std::os::unix::fs::{MetadataExt, PermissionsExt};

#[derive(Debug, Clone, PartialEq)]
enum Kind {
    File,
    Dir,
    Symlink(PathBuf),
    Other,
}

#[derive(Debug, Clone)]
struct Entry {
    kind: Kind,
    mode: u32,
    size: u64,
}

/// Every path below `root` (not following symlinks), keyed by relative path.
fn scan(root: &Path) -> Result<BTreeMap<String, Entry>> {
    let mut out = BTreeMap::new();
    fn walk(dir: &Path, rel: &str, out: &mut BTreeMap<String, Entry>) -> Result<()> {
        for e in std::fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let e = e?;
            let name = e.file_name();
            let name = name.to_str().with_context(|| format!("non-UTF-8 name in {}", dir.display()))?;
            let child_rel = if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
            let m = std::fs::symlink_metadata(e.path())?;
            let kind = if m.file_type().is_symlink() {
                Kind::Symlink(std::fs::read_link(e.path())?)
            } else if m.is_dir() {
                Kind::Dir
            } else if m.is_file() {
                Kind::File
            } else {
                Kind::Other
            };
            let is_dir = kind == Kind::Dir;
            out.insert(child_rel.clone(), Entry { kind, mode: m.mode() & 0o7777, size: m.size() });
            if is_dir {
                walk(&e.path(), &child_rel, out)?;
            }
        }
        Ok(())
    }
    let m = std::fs::symlink_metadata(root)?;
    out.insert(String::new(), Entry { kind: Kind::Dir, mode: m.mode() & 0o7777, size: 0 });
    walk(root, "", &mut out)?;
    Ok(out)
}

fn same(base: &Path, work: &Path, rel: &str, a: &Entry, b: &Entry) -> Result<bool> {
    if a.kind != b.kind || a.mode != b.mode {
        return Ok(false);
    }
    if a.kind == Kind::File {
        // Compare contents, never timestamps: the agent can set mtime to anything.
        if a.size != b.size {
            return Ok(false);
        }
        return Ok(std::fs::read(base.join(rel))? == std::fs::read(work.join(rel))?);
    }
    Ok(true)
}

/// Handle on the parent folder of `rel` inside `work`, refusing symlinks on
/// the way. Returns the handle and the final name.
fn parent_of(work_root: &OwnedFd, rel: &str) -> Result<(OwnedFd, String)> {
    let (dir, name) = match rel.rsplit_once('/') {
        Some((d, n)) => (d, n),
        None => ("", rel),
    };
    let fd = crate::sandbox::beneath::open_beneath(
        work_root,
        dir,
        nix::fcntl::OFlag::O_PATH | nix::fcntl::OFlag::O_DIRECTORY,
        nix::fcntl::ResolveFlag::RESOLVE_NO_SYMLINKS,
    )
    .with_context(|| format!("opening the folder of `{rel}` without following symlinks"))?;
    Ok((fd, name.to_string()))
}

/// `/proc/self/fd/<N>/<name>`: a path that reaches the entry through the
/// already-opened parent handle, so tools like `cp` and `remove_dir_all` act
/// on that exact folder and not on whatever a path string resolves to now.
fn via_fd(parent: &OwnedFd, name: &str) -> PathBuf {
    PathBuf::from(format!("/proc/self/fd/{}/{name}", parent.as_raw_fd()))
}

fn remove_entry(work_root: &OwnedFd, rel: &str, entry: &Entry, only_if_empty: bool) -> Result<bool> {
    let (parent, name) = parent_of(work_root, rel)?;
    let p = via_fd(&parent, &name);
    if entry.kind == Kind::Dir {
        if only_if_empty {
            return Ok(std::fs::remove_dir(&p).is_ok());
        }
        // std's remove_dir_all does not follow symlinks inside the tree.
        std::fs::remove_dir_all(&p).with_context(|| format!("removing `{rel}`"))?;
    } else {
        std::fs::remove_file(&p).with_context(|| format!("removing `{rel}`"))?;
    }
    Ok(true)
}

/// Make every path that `writable` refuses equal to `base` again. Returns the
/// paths that were changed back, sorted.
pub fn revert_outside(work: &Path, base: &Path, writable: impl Fn(&str) -> bool) -> Result<Vec<String>> {
    let a = scan(base)?;
    let b = scan(work)?;
    let root = crate::sandbox::beneath::open_root(work).context("opening workspace")?;
    let mut reverted = Vec::new();

    // 1. Remove what is new (or of a different kind) outside the globs,
    //    deepest first. A new folder is removed only if it ends up empty: it
    //    may hold files that the globs do allow.
    let mut extra: Vec<(&String, &Entry)> = b
        .iter()
        .filter(|(rel, e)| !rel.is_empty() && !writable(rel) && a.get(*rel).map(|x| x.kind != e.kind).unwrap_or(true))
        .collect();
    extra.sort_by_key(|(rel, _)| std::cmp::Reverse(rel.matches('/').count()));
    for (rel, e) in extra {
        let kind_changed = a.contains_key(rel);
        if remove_entry(&root, rel, e, e.kind == Kind::Dir && !kind_changed)? {
            reverted.push(rel.clone());
        }
    }

    // 2. Put back what is missing or different, shallowest first, so a
    //    folder exists before its contents are restored.
    let mut missing: Vec<(&String, &Entry)> = Vec::new();
    for (rel, e) in &a {
        if writable(rel) {
            continue;
        }
        let now = scan_one(work, rel);
        let differs = match &now {
            None => true,
            Some(cur) => !same(base, work, rel, e, cur)?,
        };
        if differs {
            missing.push((rel, e));
        }
    }
    missing.sort_by_key(|(rel, _)| rel.matches('/').count());
    for (rel, e) in missing {
        restore_entry(&root, base, rel, e, scan_one(work, rel).as_ref())?;
        if !reverted.contains(rel) {
            reverted.push(if rel.is_empty() { ".".into() } else { rel.clone() });
        }
    }
    reverted.sort();
    reverted.dedup();
    Ok(reverted)
}

fn scan_one(work: &Path, rel: &str) -> Option<Entry> {
    let p = if rel.is_empty() { work.to_path_buf() } else { work.join(rel) };
    let m = std::fs::symlink_metadata(&p).ok()?;
    let kind = if m.file_type().is_symlink() {
        Kind::Symlink(std::fs::read_link(&p).ok()?)
    } else if m.is_dir() {
        Kind::Dir
    } else if m.is_file() {
        Kind::File
    } else {
        Kind::Other
    };
    Some(Entry { kind, mode: m.mode() & 0o7777, size: m.size() })
}

fn restore_entry(work_root: &OwnedFd, base: &Path, rel: &str, want: &Entry, now: Option<&Entry>) -> Result<()> {
    if rel.is_empty() {
        // The workspace root itself: only its mode can differ.
        std::fs::set_permissions(via_fd(work_root, "."), std::fs::Permissions::from_mode(want.mode))?;
        return Ok(());
    }
    // Make sure every folder on the way is a real folder (an allowed path
    // might have been replaced by a symlink or a file).
    let parts: Vec<&str> = rel.split('/').collect();
    for i in 1..parts.len() {
        let anc = parts[..i].join("/");
        let (parent, name) = parent_of(work_root, &anc)?;
        let p = via_fd(&parent, &name);
        match std::fs::symlink_metadata(&p) {
            Ok(m) if m.is_dir() => {}
            Ok(m) => {
                let k = if m.file_type().is_symlink() { Kind::Symlink(PathBuf::new()) } else { Kind::File };
                remove_entry(work_root, &anc, &Entry { kind: k, mode: 0, size: 0 }, false)?;
                std::fs::create_dir(&p)?;
            }
            Err(_) => std::fs::create_dir(&p)?,
        }
        let mode = std::fs::symlink_metadata(base.join(&anc)).map(|m| m.mode() & 0o7777).unwrap_or(0o755);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(mode))?;
    }
    let (parent, name) = parent_of(work_root, rel)?;
    let p = via_fd(&parent, &name);
    if let Some(cur) = now {
        let keep_dir = want.kind == Kind::Dir && cur.kind == Kind::Dir;
        if !keep_dir {
            remove_entry(work_root, rel, cur, false)?;
        }
    }
    match &want.kind {
        Kind::Dir => {
            if std::fs::symlink_metadata(&p).is_err() {
                std::fs::create_dir(&p)?;
            }
            std::fs::set_permissions(&p, std::fs::Permissions::from_mode(want.mode))?;
        }
        Kind::Symlink(target) => std::os::unix::fs::symlink(target, &p)?,
        Kind::File => {
            // Copied here rather than with `cp`: `/proc/self/fd/N` means the
            // runtime's fd only inside the runtime's own process.
            let fd = nix::fcntl::openat(
                &parent,
                name.as_str(),
                nix::fcntl::OFlag::O_WRONLY | nix::fcntl::OFlag::O_CREAT | nix::fcntl::OFlag::O_EXCL | nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_CLOEXEC,
                nix::sys::stat::Mode::from_bits_truncate(want.mode),
            )
            .with_context(|| format!("recreating `{rel}`"))?;
            let mut dst = std::fs::File::from(fd);
            let mut src = std::fs::File::open(base.join(rel))?;
            std::io::copy(&mut src, &mut dst)?;
            dst.set_permissions(std::fs::Permissions::from_mode(want.mode))?;
        }
        // FIFOs and sockets: nothing a source tree needs; leave the path absent.
        Kind::Other => {}
    }
    Ok(())
}

/// Write the difference between `base` and `work` as a unified diff that
/// applies with `patch -p1` (or `git apply`) in the original repo.
///
/// `--no-dereference` matters: without it `diff` follows symlinks, and a link
/// in the workspace pointing at, say, `~/.ssh/id_ed25519` would copy that
/// file's content into the patch (see D-034).
pub fn write_patch(base: &Path, work: &Path, out: &Path) -> Result<bool> {
    let parent = base.parent().context("checkpoint has no parent folder")?;
    let output = Command::new("diff")
        .args(["-ruN", "--no-dereference"])
        .arg(base)
        .arg(work)
        .current_dir(parent)
        .output()
        .context("running diff")?;
    // Symlinks that exist on one side only. GNU diff cannot show these when it
    // is told not to follow links: it prints "No such file or directory" for
    // each and exits 2 (see Findings, 2026-11-11). They are found here and
    // listed in the patch as comment lines instead.
    let (a, b) = (scan(base)?, scan(work)?);
    let mut notes = Vec::new();
    for (rel, e) in &b {
        if let (Kind::Symlink(target), None) = (&e.kind, a.get(rel)) {
            notes.push(format!("# agentos: symlink added: {rel} -> {}\n", target.display()));
        }
    }
    for (rel, e) in &a {
        if let (Kind::Symlink(target), None) = (&e.kind, b.get(rel)) {
            notes.push(format!("# agentos: symlink removed: {rel} (was -> {})\n", target.display()));
        }
    }
    // diff exits 0 when the trees are the same, 1 when they differ, 2 on error.
    let code = output.status.code();
    let stderr = String::from_utf8_lossy(&output.stderr);
    let only_symlink_complaints = stderr.lines().count() == notes.len()
        && stderr.lines().all(|l| l.ends_with(": No such file or directory"));
    let ok = code == Some(0) || code == Some(1) || (code == Some(2) && !notes.is_empty() && only_symlink_complaints);
    if !ok {
        bail!("diff failed: {}", stderr.trim());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let base_s = format!("{}/", base.display());
    let work_s = format!("{}/", work.display());
    // Rewrite the header paths to the usual a/ and b/ prefixes.
    let mut patched = String::with_capacity(text.len());
    if !notes.is_empty() {
        patched.push_str("# agentos: a unified diff cannot carry symlinks; apply these by hand:\n");
        for n in &notes {
            patched.push_str(n);
        }
    }
    for line in text.split_inclusive('\n') {
        if line.starts_with("diff ") || line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("Binary files ") || line.starts_with("Symbolic links ") {
            patched.push_str(&line.replace(&base_s, "a/").replace(&work_s, "b/"));
        } else {
            patched.push_str(line);
        }
    }
    let changed = code != Some(0) || !notes.is_empty();
    std::fs::write(out, patched.as_bytes())?;
    Ok(changed)
}
