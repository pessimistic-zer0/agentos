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
        remove_tree(&tmp)?; // left over from an earlier crash
    }
    if let Err(e) = run_cp(work, &tmp) {
        let _ = remove_tree(&tmp); // do not leave half a copy behind
        return Err(e);
    }
    std::fs::rename(&tmp, &final_path).with_context(|| format!("renaming {} into place", tmp.display()))?;
    Ok(final_path)
}

/// Delete a folder tree that code in the sandbox may have shaped.
///
/// `remove_dir_all` alone is not enough for such a tree: the owner of a
/// folder cannot list it without the read bit, or delete from it without the
/// write and search bits, and a command can clear those bits (`chmod 000`).
/// A workspace like that made recovery fail for good (bug B-010). So when
/// the plain delete fails, every folder in the tree is given back the
/// owner's three bits and the delete is tried again.
///
/// This relies on nothing else touching the tree meanwhile: it is only
/// called when no sandboxed process is alive. Symlinks are never followed:
/// a mode is only changed on what `symlink_metadata` says is a real folder.
pub fn remove_tree(path: &Path) -> Result<()> {
    fn unlock(dir: &Path) -> std::io::Result<()> {
        let m = std::fs::symlink_metadata(dir)?;
        if !m.is_dir() {
            return Ok(());
        }
        let mode = m.mode() & 0o7777;
        if mode & 0o700 != 0o700 {
            std::fs::set_permissions(dir, std::fs::Permissions::from_mode(mode | 0o700))?;
        }
        for e in std::fs::read_dir(dir)? {
            let e = e?;
            if e.file_type()?.is_dir() {
                unlock(&e.path())?;
            }
        }
        Ok(())
    }
    match std::fs::remove_dir_all(path) {
        Ok(()) => return Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => {}
    }
    unlock(path).with_context(|| format!("making {} deletable", path.display()))?;
    std::fs::remove_dir_all(path).with_context(|| format!("removing {}", path.display()))
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
        remove_tree(&tmp)?; // left over from an interrupted restore
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
    remove_tree(&tmp)?;
    Ok(())
}

/// Remove half-made things a crash can leave: `.tmp-*` checkpoint copies and
/// the restore folder. Returns nothing; missing things are fine.
pub fn remove_leftovers(work: &Path, checkpoints: &Path) -> Result<()> {
    let tmp = restore_tmp(work);
    if tmp.exists() {
        remove_tree(&tmp)?;
    }
    if checkpoints.exists() {
        for e in std::fs::read_dir(checkpoints)? {
            let e = e?;
            if e.file_name().to_string_lossy().starts_with(".tmp-") {
                remove_tree(&e.path())?;
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
///
/// The copy is a child process, and a child normally outlives its parent. If
/// the runtime is killed during the copy, a surviving `cp` would go on
/// writing, and could create its folder after recovery has cleaned up (bug
/// B-007). So the child asks the kernel to kill it when the runtime dies.
pub fn run_cp(src: &Path, dst: &Path) -> Result<()> {
    use std::os::unix::process::CommandExt;
    let runtime_pid = nix::unistd::getpid();
    let mut cmd = Command::new("cp");
    cmd.arg("-a").arg("--reflink=auto").arg("--").arg(src).arg(dst);
    // SAFETY: the closure runs in the forked child before `exec`, where only
    // async-signal-safe calls are allowed. It makes two raw system calls
    // (`prctl`, `getppid`) and allocates nothing. It relies on:
    // - PR_SET_PDEATHSIG: the kernel sends SIGKILL to this child when the
    //   *thread* that forked it exits. Checkpoints are made on the runtime's
    //   main thread, which lives as long as the process.
    // - the `getppid` check: if the runtime died between `fork` and `prctl`,
    //   the signal was missed and the child now has a different parent, so it
    //   stops here instead of running `cp`.
    unsafe {
        cmd.pre_exec(move || {
            if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL as libc::c_ulong) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::getppid() != runtime_pid.as_raw() {
                return Err(std::io::Error::other("the runtime died before the copy started"));
            }
            Ok(())
        });
    }
    let out = cmd.output().context("running cp")?;
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
        remove_tree(&p).with_context(|| format!("removing `{rel}`"))?;
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
///
/// A unified diff is text. It cannot carry a binary file's content, a file's
/// permission bits, an empty folder or a symlink, and `patch` and `git apply`
/// skip what they do not understand without a word. So everything the patch
/// leaves out is found here, listed at the top of the patch as comment
/// lines, and returned, so the run can say so (D-090).
///
/// `scratch` says which paths are caches (`filesystem.scratch`): changes
/// there are left out of the patch and of its notes (D-099).
pub fn write_patch(base: &Path, work: &Path, out: &Path, scratch: impl Fn(&str) -> bool) -> Result<PatchReport> {
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
        if scratch(rel) {
            continue;
        }
        if let (Kind::Symlink(target), None) = (&e.kind, a.get(rel)) {
            notes.push(format!("# agentos: symlink added: {rel} -> {}\n", target.display()));
        }
    }
    for (rel, e) in &a {
        if scratch(rel) {
            continue;
        }
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
    // The diff is a run of sections, one per file. A text file's section
    // starts with a `diff …` line; a binary file's is one `Binary files …`
    // line. Sections for scratch paths are dropped.
    // Both header forms name the path twice: `a/<rel> … b/<rel>`.
    let same_path_twice = |s: &str, between: &str| s[..s.len().saturating_sub(between.len()) / 2].to_string();
    let mut dropping = false;
    let mut kept_any = false;
    for line in text.split_inclusive('\n') {
        let is_header = line.starts_with("diff ") || line.starts_with("Binary files ");
        if is_header || line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("Symbolic links ") {
            let line = line.replace(&base_s, "a/").replace(&work_s, "b/");
            if is_header {
                let body = line.trim_end();
                let rel = if let Some(rest) = body.strip_prefix("Binary files a/").and_then(|l| l.strip_suffix(" differ")) {
                    same_path_twice(rest, " and b/")
                } else {
                    body.split_once(" a/").map(|(_, rest)| same_path_twice(rest, " b/")).unwrap_or_default()
                };
                dropping = scratch(&rel);
                if dropping && line.starts_with("Binary files ") {
                    dropping = false; // a binary section is this one line
                    continue;
                }
            }
            if !dropping {
                kept_any = true;
                patched.push_str(&line);
            }
        } else if !dropping {
            patched.push_str(line);
        }
    }
    // What the diff above does not carry (symlinks were handled before it).
    let mut left_out = LeftOut::default();
    let mut extra = Vec::new();
    for line in patched.lines() {
        // "Binary files a/<rel> and b/<rel> differ": the same path twice.
        let Some(both) = line.strip_prefix("Binary files a/").and_then(|l| l.strip_suffix(" differ")) else { continue };
        let rel = &both[..both.len().saturating_sub(" and b/".len()) / 2];
        left_out.binary += 1;
        extra.push(match b.get(rel) {
            Some(_) => format!("# agentos: binary file not in this patch: {rel} (copy it from {})\n", work.join(rel).display()),
            None => format!("# agentos: binary file was deleted, which this patch cannot do: {rel}\n"),
        });
    }
    for (rel, e) in &b {
        if scratch(rel) {
            continue;
        }
        match (&e.kind, a.get(rel)) {
            (Kind::File, Some(old)) if old.kind == Kind::File && old.mode != e.mode => {
                left_out.modes += 1;
                extra.push(format!("# agentos: mode changed, which this patch cannot do: {rel} ({:o} -> {:o})\n", old.mode, e.mode));
            }
            // `diff` prints a header and no lines for a new file with nothing
            // in it, and `patch` and `git apply` then create nothing. The
            // usual victim is a Python package's empty `__init__.py` (B-012).
            (Kind::File, None) if e.size == 0 => {
                left_out.empty_files += 1;
                extra.push(format!("# agentos: empty new file not in this patch: {rel} (create it: touch {rel})\n"));
            }
            (Kind::File, None) if e.mode & 0o111 != 0 => {
                left_out.modes += 1;
                extra.push(format!("# agentos: new file is executable, but a patch creates plain files: {rel} (chmod {:o})\n", e.mode));
            }
            (Kind::Dir, None) if !b.keys().any(|k| k.strip_prefix(rel.as_str()).is_some_and(|rest| rest.starts_with('/'))) => {
                left_out.empty_dirs += 1;
                extra.push(format!("# agentos: empty folder not in this patch: {rel}\n"));
            }
            _ => {}
        }
    }
    for (rel, e) in &a {
        if e.kind == Kind::File && e.size == 0 && !b.contains_key(rel) && !scratch(rel) {
            left_out.empty_files += 1;
            extra.push(format!("# agentos: empty file was deleted, which this patch cannot do: {rel}\n"));
        }
    }
    left_out.symlinks = notes.len();
    if !extra.is_empty() {
        let head = format!("# agentos: this patch leaves out {} thing(s) a unified diff cannot carry; apply them by hand:\n{}", extra.len(), extra.concat());
        patched.insert_str(0, &head);
    }
    let changed = kept_any || !left_out.is_empty();
    std::fs::write(out, patched.as_bytes())?;
    Ok(PatchReport { changed, left_out })
}

/// What `write_patch` found.
#[derive(Debug, Default)]
pub struct PatchReport {
    /// The workspace differs from the base checkpoint in some way.
    pub changed: bool,
    pub left_out: LeftOut,
}

/// Counts of the changes a patch cannot carry. Each one is a comment line
/// at the top of the patch.
#[derive(Debug, Default, PartialEq)]
pub struct LeftOut {
    pub binary: usize,
    pub modes: usize,
    pub empty_files: usize,
    pub empty_dirs: usize,
    pub symlinks: usize,
}

impl LeftOut {
    pub fn is_empty(&self) -> bool {
        *self == LeftOut::default()
    }

    /// For example `2 binary files, 1 file mode`.
    pub fn describe(&self) -> String {
        [(self.binary, "binary file"), (self.modes, "file mode"), (self.empty_files, "empty file"), (self.empty_dirs, "empty folder"), (self.symlinks, "symlink")]
            .iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, what)| format!("{n} {what}{}", if *n == 1 { "" } else { "s" }))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

// ---------------------------------------------------------------------------
// `agentos apply`: copying the result into a repository (D-108).
//
// This is the one place where AgentOS writes outside its own state folder,
// and only when the user runs the command. It works from the same two
// trees as the patch (the base checkpoint and the final workspace) but
// copies files instead of describing lines, so binary files, empty files,
// file modes, empty folders and symlinks all come across.
//
// Two rules keep it safe:
//
// 1. Nothing is touched unless everything can be: for each path that the
//    run changed, the repository must still hold what the base checkpoint
//    holds (or already hold the result). One path that differs refuses the
//    whole apply, before any write.
// 2. Every write goes through a handle on its parent folder opened with
//    openat2(RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS), like the revert code
//    above, so a symlink in the repository cannot send a file elsewhere.
//
// Paths the run never saw (denied or skipped: they are in neither tree)
// are never looked at, written or deleted.
// ---------------------------------------------------------------------------

/// What `apply_changes` did, or why it did nothing.
#[derive(Debug)]
pub enum ApplyResult {
    /// The repository differs from the base checkpoint at these paths (or a
    /// change is of a kind `apply` does not do). Nothing was written.
    Refused(Vec<String>),
    Done(Applied),
}

#[derive(Debug, Default)]
pub struct Applied {
    /// One line per change made (or, in a dry run, that would be made):
    /// `A path`, `M path`, `D path`, `mode path 644 -> 755`, `L path -> target`.
    pub changes: Vec<String>,
    /// Paths that already held the result (an apply done twice).
    pub already: usize,
    /// Folders the run removed that are not empty in the repository
    /// (they hold files the run never saw) and were therefore kept.
    pub kept_folders: Vec<String>,
}

enum Op {
    Dir { rel: String, mode: u32, exists: bool },
    File { rel: String, mode: u32 },
    Link { rel: String, to: PathBuf, replace: bool },
    Delete { rel: String, dir: bool },
}

/// Make `target` hold what the run made of the repository: every
/// difference between `base` and `work`, except under `scratch` paths.
pub fn apply_changes(base: &Path, work: &Path, target: &Path, scratch: impl Fn(&str) -> bool, dry_run: bool) -> Result<ApplyResult> {
    let (a, b) = (scan(base)?, scan(work)?);
    let mut rels: Vec<&String> = a.keys().chain(b.keys()).filter(|r| !r.is_empty() && !scratch(r)).collect();
    rels.sort();
    rels.dedup();

    let mut out = Applied::default();
    let mut problems = Vec::new();
    let mut ops = Vec::new();
    let mode = |m: u32| format!("{m:o}");
    for rel in rels {
        let (old, new) = (a.get(rel), b.get(rel));
        if let (Some(o), Some(n)) = (old, new) {
            if same(base, work, rel, o, n)? {
                continue;
            }
        }
        // Where does the repository stand on this path?
        let now = scan_one(target, rel);
        let equals = |root: &Path, want: Option<&Entry>| -> Result<bool> {
            Ok(match (want, &now) {
                (None, None) => true,
                (Some(w), Some(cur)) => same(root, target, rel, w, cur)?,
                _ => false,
            })
        };
        if equals(work, new)? {
            out.already += 1;
            continue;
        }
        if !equals(base, old)? {
            problems.push(format!("{rel}: the repository no longer holds what the run started from"));
            continue;
        }
        if let Some(link) = symlink_on_the_way(target, rel) {
            problems.push(format!("{rel}: `{link}` is a symlink in the repository; apply does not write through symlinks"));
            continue;
        }
        let dir = |e: Option<&Entry>| e.is_some_and(|e| e.kind == Kind::Dir);
        match (old, new) {
            (Some(_), Some(_)) if dir(old) != dir(new) => {
                problems.push(format!("{rel}: a folder became a file or the other way round; apply does not do that, use the patch"));
            }
            (_, Some(n)) => match &n.kind {
                Kind::Dir => {
                    out.changes.push(match old {
                        Some(o) => format!("mode {rel}/ {} -> {}", mode(o.mode), mode(n.mode)),
                        None => format!("A {rel}/"),
                    });
                    ops.push(Op::Dir { rel: rel.clone(), mode: n.mode, exists: old.is_some() });
                }
                Kind::File => {
                    out.changes.push(match old {
                        // Same content, other mode: say so, it is the common case of `chmod +x`.
                        Some(o) if o.kind == Kind::File && o.size == n.size && std::fs::read(base.join(rel))? == std::fs::read(work.join(rel))? => {
                            format!("mode {rel} {} -> {}", mode(o.mode), mode(n.mode))
                        }
                        Some(_) => format!("M {rel}"),
                        None if n.mode & 0o111 != 0 => format!("A {rel} (executable, mode {})", mode(n.mode)),
                        None => format!("A {rel}"),
                    });
                    ops.push(Op::File { rel: rel.clone(), mode: n.mode });
                }
                Kind::Symlink(to) => {
                    out.changes.push(format!("L {rel} -> {}", to.display()));
                    ops.push(Op::Link { rel: rel.clone(), to: to.clone(), replace: old.is_some() });
                }
                Kind::Other => problems.push(format!("{rel}: not a file, folder or symlink; apply does not copy it")),
            },
            (Some(o), None) => {
                out.changes.push(format!("D {rel}{}", if o.kind == Kind::Dir { "/" } else { "" }));
                ops.push(Op::Delete { rel: rel.clone(), dir: o.kind == Kind::Dir });
            }
            (None, None) => unreachable!("the path came from one of the two trees"),
        }
    }
    if !problems.is_empty() {
        return Ok(ApplyResult::Refused(problems));
    }
    if dry_run {
        return Ok(ApplyResult::Done(out));
    }

    let root = crate::sandbox::beneath::open_root(target).context("opening the repository")?;
    // Additions and changes first, in path order, so a folder exists before
    // what goes into it. Deletions after, deepest first.
    for op in ops.iter().filter(|op| !matches!(op, Op::Delete { .. })) {
        match op {
            Op::Dir { rel, mode, exists } => {
                let (parent, name) = parent_of(&root, rel)?;
                if !exists {
                    nix::sys::stat::mkdirat(&parent, name.as_str(), nix::sys::stat::Mode::from_bits_truncate(0o755)).with_context(|| format!("creating `{rel}`"))?;
                }
                std::fs::set_permissions(via_fd(&parent, &name), std::fs::Permissions::from_mode(*mode))?;
            }
            Op::File { rel, mode } => {
                let (parent, name) = parent_of(&root, rel)?;
                // Written under another name and renamed into place, so the
                // repository never holds half a file.
                let tmp = format!(".agentos-apply-{}.tmp", std::process::id());
                let _ = nix::unistd::unlinkat(&parent, tmp.as_str(), nix::unistd::UnlinkatFlags::NoRemoveDir);
                let fd = nix::fcntl::openat(
                    &parent,
                    tmp.as_str(),
                    nix::fcntl::OFlag::O_WRONLY | nix::fcntl::OFlag::O_CREAT | nix::fcntl::OFlag::O_EXCL | nix::fcntl::OFlag::O_NOFOLLOW | nix::fcntl::OFlag::O_CLOEXEC,
                    nix::sys::stat::Mode::from_bits_truncate(*mode),
                )
                .with_context(|| format!("creating `{rel}`"))?;
                let mut dst = std::fs::File::from(fd);
                let copied = (|| -> Result<()> {
                    std::io::copy(&mut std::fs::File::open(work.join(rel))?, &mut dst)?;
                    dst.set_permissions(std::fs::Permissions::from_mode(*mode))?;
                    dst.sync_all()?;
                    nix::fcntl::renameat(&parent, tmp.as_str(), &parent, name.as_str())?;
                    Ok(())
                })();
                if copied.is_err() {
                    let _ = nix::unistd::unlinkat(&parent, tmp.as_str(), nix::unistd::UnlinkatFlags::NoRemoveDir);
                }
                copied.with_context(|| format!("writing `{rel}`"))?;
            }
            Op::Link { rel, to, replace } => {
                let (parent, name) = parent_of(&root, rel)?;
                if *replace {
                    nix::unistd::unlinkat(&parent, name.as_str(), nix::unistd::UnlinkatFlags::NoRemoveDir).with_context(|| format!("replacing `{rel}`"))?;
                }
                std::os::unix::fs::symlink(to, via_fd(&parent, &name)).with_context(|| format!("creating the symlink `{rel}`"))?;
            }
            Op::Delete { .. } => {}
        }
    }
    let mut deletions: Vec<(&String, bool)> = ops.iter().filter_map(|op| if let Op::Delete { rel, dir } = op { Some((rel, *dir)) } else { None }).collect();
    deletions.sort_by_key(|(rel, _)| std::cmp::Reverse(rel.matches('/').count()));
    for (rel, dir) in deletions {
        let (parent, name) = parent_of(&root, rel)?;
        let flag = if dir { nix::unistd::UnlinkatFlags::RemoveDir } else { nix::unistd::UnlinkatFlags::NoRemoveDir };
        match nix::unistd::unlinkat(&parent, name.as_str(), flag) {
            Ok(()) => {}
            // The folder still holds something the run never saw (a denied
            // or skipped file, or one made since): it stays.
            Err(nix::errno::Errno::ENOTEMPTY) | Err(nix::errno::Errno::EEXIST) if dir => out.kept_folders.push(rel.clone()),
            Err(e) => return Err(e).with_context(|| format!("removing `{rel}`")),
        }
    }
    Ok(ApplyResult::Done(out))
}

/// The first folder on the way to `rel` that is a symlink in `root`, if any.
fn symlink_on_the_way(root: &Path, rel: &str) -> Option<String> {
    let parts: Vec<&str> = rel.split('/').collect();
    (1..parts.len()).map(|i| parts[..i].join("/")).find(|anc| std::fs::symlink_metadata(root.join(anc)).is_ok_and(|m| m.file_type().is_symlink()))
}
