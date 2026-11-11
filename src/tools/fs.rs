//! `fs.read`, `fs.list` and `fs.write`, run by the trusted runtime on the
//! workspace folder.
//!
//! M1 (no sandbox yet): paths are cleaned up as text and joined onto the
//! workspace folder. That is a *string check*, which design rule 7 forbids
//! for confinement: a symlink inside the workspace can still lead outside.
//! M2 replaces `resolve` with `openat2(RESOLVE_BENEATH)`, where the kernel
//! does the check. Until then nothing here is a security boundary.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::blob::content_hash;
use crate::tools::{ListEntry, ToolError};

/// A path the agent asked for, after resolution.
#[derive(Debug, Clone)]
pub struct Resolved {
    /// Workspace-relative, `/`-separated, `""` for the root. Policy globs are
    /// matched against this.
    pub rel: String,
    /// Where it is on the host.
    pub host: PathBuf,
}

/// Turn the agent's path into a workspace-relative one. Accepts `.`, `./a`,
/// `a/b/../c`; refuses absolute paths and paths that climb above the root.
pub fn resolve(work: &Path, path: &str) -> Result<Resolved, ToolError> {
    if path.starts_with('/') {
        return Err(ToolError::new(format!("`{path}`: absolute paths are not allowed; paths are relative to the workspace")));
    }
    let mut parts: Vec<&str> = Vec::new();
    for comp in path.split('/') {
        match comp {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(ToolError::new(format!("`{path}` leaves the workspace")));
                }
            }
            c => parts.push(c),
        }
    }
    let rel = parts.join("/");
    let host = if rel.is_empty() { work.to_path_buf() } else { work.join(&rel) };
    Ok(Resolved { rel, host })
}

pub fn read(r: &Resolved, max_bytes: u64) -> Result<String, ToolError> {
    let meta = fs::metadata(&r.host).map_err(|e| io_err(&r.rel, e))?;
    if meta.is_dir() {
        return Err(ToolError::new(format!("`{}` is a directory; use fs.list", show(&r.rel))));
    }
    if meta.len() > max_bytes {
        return Err(ToolError::new(format!(
            "`{}` is {} bytes, more than the {max_bytes}-byte limit (limits.max_output_kb)",
            show(&r.rel),
            meta.len()
        )));
    }
    let bytes = fs::read(&r.host).map_err(|e| io_err(&r.rel, e))?;
    String::from_utf8(bytes).map_err(|_| ToolError::new(format!("`{}` is not UTF-8 text", show(&r.rel))))
}

pub fn list(r: &Resolved) -> Result<Vec<ListEntry>, ToolError> {
    let mut out = Vec::new();
    for entry in fs::read_dir(&r.host).map_err(|e| io_err(&r.rel, e))? {
        let entry = entry.map_err(|e| io_err(&r.rel, e))?;
        let meta = entry.metadata().map_err(|e| io_err(&r.rel, e))?; // does not follow symlinks
        let kind = if meta.is_dir() {
            "dir"
        } else if meta.is_file() {
            "file"
        } else if meta.file_type().is_symlink() {
            "symlink"
        } else {
            "other"
        };
        out.push(ListEntry { name: entry.file_name().to_string_lossy().into_owned(), kind: kind.into(), size: meta.len() });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

/// Hash of the current file at `r`, or `None` if there is no file. This is the
/// "before" fingerprint stored in the `fs.write` intent.
pub fn current_hash(r: &Resolved) -> Result<Option<String>, ToolError> {
    match fs::symlink_metadata(&r.host) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(io_err(&r.rel, e)),
        Ok(m) if m.is_file() => fs::read(&r.host).map(|b| Some(content_hash(&b))).map_err(|e| io_err(&r.rel, e)),
        Ok(_) => Err(ToolError::new(format!("`{}` exists and is not a regular file", show(&r.rel)))),
    }
}

/// Temporary file name used while writing `call_id`'s new content.
pub fn temp_name(call_id: u64) -> String {
    format!(".agentos-tmp-{call_id}")
}

/// Atomic replace: write a temp file next to the target, fsync it, rename it
/// over the target. A crash leaves either the old file or the new one, never a
/// half-written mix.
pub fn write(r: &Resolved, content: &str, call_id: u64) -> Result<u64, ToolError> {
    if r.rel.is_empty() {
        return Err(ToolError::new("cannot write to the workspace root"));
    }
    let parent = r.host.parent().expect("non-root path has a parent");
    if !parent.is_dir() {
        return Err(ToolError::new(format!("the folder for `{}` does not exist", r.rel)));
    }
    let tmp = parent.join(temp_name(call_id));
    let result = (|| -> std::io::Result<()> {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(content.as_bytes())?;
        f.sync_all()?;
        fs::rename(&tmp, &r.host)
    })();
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(io_err(&r.rel, e));
    }
    Ok(content.len() as u64)
}

fn show(rel: &str) -> &str {
    if rel.is_empty() {
        "."
    } else {
        rel
    }
}

fn io_err(rel: &str, e: std::io::Error) -> ToolError {
    ToolError::new(format!("`{}`: {e}", show(rel)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_cleans_paths_and_refuses_escapes() {
        let w = Path::new("/ws");
        assert_eq!(resolve(w, ".").unwrap().rel, "");
        assert_eq!(resolve(w, "./src//a.py").unwrap().rel, "src/a.py");
        assert_eq!(resolve(w, "src/../tests/t.py").unwrap().rel, "tests/t.py");
        assert!(resolve(w, "../etc/passwd").is_err());
        assert!(resolve(w, "/etc/passwd").is_err());
        assert!(resolve(w, "a/../../b").is_err());
    }
}
