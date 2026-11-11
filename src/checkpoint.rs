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
    // diff exits 0 when the trees are the same, 1 when they differ, 2 on error.
    let code = output.status.code();
    if code != Some(0) && code != Some(1) {
        bail!("diff failed: {}", String::from_utf8_lossy(&output.stderr).trim());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let base_s = format!("{}/", base.display());
    let work_s = format!("{}/", work.display());
    // Rewrite the header paths to the usual a/ and b/ prefixes.
    let mut patched = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if line.starts_with("diff ") || line.starts_with("--- ") || line.starts_with("+++ ") || line.starts_with("Binary files ") {
            patched.push_str(&line.replace(&base_s, "a/").replace(&work_s, "b/"));
        } else {
            patched.push_str(line);
        }
    }
    std::fs::write(out, patched.as_bytes())?;
    Ok(code == Some(1))
}
