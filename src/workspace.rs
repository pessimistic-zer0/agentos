//! The execution directory and the workspace copy.
//!
//! Every execution gets its own directory under the state dir:
//!
//! ```text
//! <state>/executions/<id>/
//! ├── log.db         event log (SQLite)
//! ├── policy.toml    the policy text this execution runs under
//! ├── script.toml    the scripted agent's steps (to resume after a crash)
//! ├── llm.json       the LLM agent's endpoint, model and task (no API key)
//! ├── lock           held (flock) while a runtime works on this execution
//! ├── work/          the workspace: the only folder the sandbox sees, as /work
//! ├── checkpoints/   cp-0000, cp-0001, ...: full copies of work/
//! ├── blobs/         content-addressed file contents and tool outputs
//! └── result.patch   the output: diff from cp-0000 to the final work/
//! ```
//!
//! Only `work/` is ever mounted into the sandbox, so the log, checkpoints and
//! blobs stay out of the agent's reach (design rule 9).

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

use crate::policy::Policy;

#[derive(Debug, Clone)]
pub struct ExecDir {
    pub id: String,
    pub root: PathBuf,
}

impl ExecDir {
    pub fn new(state_dir: &Path, id: &str) -> ExecDir {
        ExecDir { id: id.to_string(), root: state_dir.join("executions").join(id) }
    }
    pub fn log(&self) -> PathBuf {
        self.root.join("log.db")
    }
    pub fn policy(&self) -> PathBuf {
        self.root.join("policy.toml")
    }
    /// The scripted agent's script, kept so a resumed run can reload it.
    pub fn script(&self) -> PathBuf {
        self.root.join("script.toml")
    }
    /// The LLM agent's endpoint, model and task (never the API key).
    pub fn llm_config(&self) -> PathBuf {
        self.root.join("llm.json")
    }
    pub fn lock(&self) -> PathBuf {
        self.root.join("lock")
    }
    pub fn work(&self) -> PathBuf {
        self.root.join("work")
    }
    pub fn checkpoints(&self) -> PathBuf {
        self.root.join("checkpoints")
    }
    pub fn blobs(&self) -> PathBuf {
        self.root.join("blobs")
    }
    pub fn patch(&self) -> PathBuf {
        self.root.join("result.patch")
    }
}

/// All execution ids under a state dir, oldest first (ids start with the time).
pub fn list_executions(state_dir: &Path) -> Result<Vec<String>> {
    let dir = state_dir.join("executions");
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut ids = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.path().join("log.db").exists() {
            if let Some(name) = entry.file_name().to_str() {
                ids.push(name.to_string());
            }
        }
    }
    ids.sort();
    Ok(ids)
}

/// Every folder under `executions/`, also ones without a log (a run that
/// died before its log was created). For `agentos rm`.
pub fn all_execution_dirs(state_dir: &Path) -> Result<Vec<String>> {
    let dir = state_dir.join("executions");
    let mut ids = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    ids.push(name.to_string());
                }
            }
        }
    }
    ids.sort();
    Ok(ids)
}

/// How many whole days ago an execution was created, from the date its id
/// starts with (`YYYYMMDD-…`, UTC). `None` if the name is not an id.
pub fn age_in_days(id: &str) -> Option<i64> {
    let d = id.get(..8)?;
    if !d.bytes().all(|b| b.is_ascii_digit()) || id.as_bytes().get(8) != Some(&b'-') {
        return None;
    }
    let (y, m, day): (i64, i64, i64) = (d[..4].parse().ok()?, d[4..6].parse().ok()?, d[6..8].parse().ok()?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&day) {
        return None;
    }
    // Days since 1970-01-01 for a calendar date (Howard Hinnant's
    // `days_from_civil`): years are counted from March so that the leap day
    // is the last day of a year.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y.rem_euclid(400);
    let doy = (153 * ((m + 9) % 12) + 2) / 5 + day - 1;
    let created = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64 / 86_400;
    Some(today - created)
}

/// What `create` would do with `repo`, without copying anything: how many
/// files and bytes would be copied and which paths left out. For
/// `agentos check`.
pub fn survey(repo: &Path, policy: &Policy) -> Result<(CopyReport, u64)> {
    fn walk(src: &Path, rel: &str, policy: &Policy, report: &mut CopyReport, bytes: &mut u64) -> Result<()> {
        for entry in fs::read_dir(src).with_context(|| format!("reading {}", src.display()))? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                bail!("file name {:?} in {} is not UTF-8; v1 does not support that", name, src.display());
            };
            let child_rel = if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
            if policy.denied(&child_rel).is_some() {
                report.denied.push(child_rel);
            } else if policy.ro_dir(&child_rel).is_some() {
                check_ro_dir(&entry.path(), &child_rel, policy)?;
                report.ro_dirs.push(child_rel);
            } else if policy.skipped(&child_rel).is_some() {
                report.skipped.push(child_rel);
            } else {
                let ft = entry.file_type()?;
                if ft.is_dir() {
                    walk(&entry.path(), &child_rel, policy, report, bytes)?;
                } else if ft.is_file() {
                    report.files_copied += 1;
                    *bytes += entry.metadata()?.len();
                } else if !ft.is_symlink() {
                    report.skipped_special.push(child_rel);
                }
            }
        }
        Ok(())
    }
    let mut report = CopyReport::default();
    let mut bytes = 0;
    walk(repo, "", policy, &mut report, &mut bytes)?;
    report.denied.sort();
    report.skipped.sort();
    all_ro_dirs_found(policy, &report)?;
    Ok((report, bytes))
}

/// A new execution id: `YYYYMMDD-HHMMSSmmm-xxxxxx` (UTC, to the millisecond).
/// Ids sort by creation time, which `agentos trace` and `status` rely on to
/// find the latest execution. The suffix (from the clock's nanoseconds and
/// the process id) keeps apart two runs started in the same millisecond;
/// only those two could sort in the wrong order.
pub fn new_execution_id() -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    let ts = crate::log::now_rfc3339(); // 2026-11-11T19:02:03.456Z
    let digits: String = ts.chars().filter(|c| c.is_ascii_digit()).collect(); // 20261111190203456
    let seed = format!("{}-{}", now.as_nanos(), std::process::id());
    let hash = crate::blob::content_hash(seed.as_bytes());
    format!("{}-{}-{}", &digits[..8], &digits[8..17], &hash[7..13])
}

#[derive(Debug, Default)]
pub struct CopyReport {
    pub files_copied: u64,
    /// Paths left out because a deny glob matched them.
    pub denied: Vec<String>,
    /// Paths left out because a skip glob matched them (D-099).
    pub skipped: Vec<String>,
    /// Sockets, FIFOs and device files: not copied (a source repo has no use for them).
    pub skipped_special: Vec<String>,
    /// `filesystem.ro_dirs` folders: an empty folder stands in the
    /// workspace; commands get the original mounted there (D-122).
    pub ro_dirs: Vec<String>,
}

/// A folder named in `filesystem.ro_dirs` will be shown to commands as it
/// is in the repository. So it must be a real folder, and nothing in it may
/// match a deny rule: design rule 6 says denied files never reach the
/// workspace, and a mount would bring them there.
fn check_ro_dir(dir: &Path, rel: &str, policy: &Policy) -> Result<()> {
    if !fs::symlink_metadata(dir)?.is_dir() {
        bail!("filesystem.ro_dirs entry `{rel}` is not a folder in the repository (a symlink does not count)");
    }
    fn walk(dir: &Path, rel: &str, policy: &Policy, found: &mut Vec<String>) -> Result<()> {
        for entry in fs::read_dir(dir).with_context(|| format!("reading {}", dir.display()))? {
            let entry = entry?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                bail!("file name {:?} in {} is not UTF-8; v1 does not support that", name, dir.display());
            };
            let child = format!("{rel}/{name}");
            if policy.denied(&child).is_some() {
                found.push(child);
            } else if entry.file_type()?.is_dir() {
                walk(&entry.path(), &child, policy, found)?;
            }
        }
        Ok(())
    }
    let mut found = Vec::new();
    walk(dir, rel, policy, &mut found)?;
    if !found.is_empty() {
        found.sort();
        let more = if found.len() > 5 { format!(", and {} more", found.len() - 5) } else { String::new() };
        bail!(
            "filesystem.ro_dirs entry `{rel}` holds paths that a deny rule matches: {}{more}. \
             A read-only folder is shown to commands as it is, so it must not hold denied files. \
             Remove them, or add them to filesystem.deny_except if they are not secrets.",
            found[..found.len().min(5)].join(", ")
        );
    }
    Ok(())
}

fn all_ro_dirs_found(policy: &Policy, report: &CopyReport) -> Result<()> {
    for d in &policy.filesystem.ro_dirs {
        if !report.ro_dirs.contains(d) {
            bail!("filesystem.ro_dirs entry `{d}` was not found in the repository (or a folder above it is left out by a deny or skip rule)");
        }
    }
    Ok(())
}

/// Copy `repo` into `dest` (which must not exist), leaving out every path the
/// policy denies (D-008). Symlinks are copied as symlinks and never followed,
/// so a link pointing outside the repo cannot pull outside files in.
pub fn create(repo: &Path, dest: &Path, policy: &Policy) -> Result<CopyReport> {
    let meta = fs::symlink_metadata(repo).with_context(|| format!("reading {}", repo.display()))?;
    if !meta.is_dir() {
        bail!("{} is not a directory", repo.display());
    }
    if dest.exists() {
        bail!("{} already exists", dest.display());
    }
    let mut report = CopyReport::default();
    copy_dir(repo, dest, "", policy, &mut report)?;
    report.denied.sort();
    report.skipped.sort();
    all_ro_dirs_found(policy, &report)?;
    Ok(report)
}

fn copy_dir(src: &Path, dst: &Path, rel: &str, policy: &Policy, report: &mut CopyReport) -> Result<()> {
    fs::create_dir(dst).with_context(|| format!("creating {}", dst.display()))?;
    let mut entries: Vec<_> = fs::read_dir(src)?.collect::<Result<_, _>>()?;
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            bail!("file name {:?} in {} is not UTF-8; v1 does not support that", name, src.display());
        };
        let child_rel = if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
        if policy.denied(&child_rel).is_some() {
            report.denied.push(child_rel);
            continue;
        }
        if policy.ro_dir(&child_rel).is_some() {
            // Not copied: an empty folder marks the place where commands
            // will find the original, mounted read-only.
            check_ro_dir(&entry.path(), &child_rel, policy)?;
            fs::create_dir(dst.join(name)).with_context(|| format!("creating the mount point {child_rel}"))?;
            report.ro_dirs.push(child_rel);
            continue;
        }
        if policy.skipped(&child_rel).is_some() {
            report.skipped.push(child_rel);
            continue;
        }
        let from = entry.path();
        let to = dst.join(name);
        let ft = entry.file_type()?; // does not follow symlinks
        if ft.is_symlink() {
            let target = fs::read_link(&from)?;
            std::os::unix::fs::symlink(&target, &to)?;
        } else if ft.is_dir() {
            copy_dir(&from, &to, &child_rel, policy, report)?;
        } else if ft.is_file() {
            // std::fs::copy keeps the permission bits; on btrfs and XFS it
            // uses copy_file_range, which can share blocks instead of copying.
            fs::copy(&from, &to).with_context(|| format!("copying {}", from.display()))?;
            report.files_copied += 1;
        } else {
            report.skipped_special.push(child_rel);
        }
    }
    // Set the folder's mode last, so a read-only source folder does not stop
    // us from creating its children.
    let mode = fs::symlink_metadata(src)?.permissions().mode();
    fs::set_permissions(dst, fs::Permissions::from_mode(mode))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn policy() -> Policy {
        let text = include_str!("../policies/nix.toml");
        Policy::parse(text).unwrap()
    }

    #[test]
    fn denied_files_are_not_copied_and_links_stay_links() -> Result<()> {
        let base = std::env::temp_dir().join(format!("agentos-ws-test-{}", std::process::id()));
        let _ = fs::remove_dir_all(&base);
        let repo = base.join("repo");
        fs::create_dir_all(repo.join("src"))?;
        fs::create_dir_all(repo.join("secrets"))?;
        fs::create_dir_all(repo.join("empty"))?;
        fs::write(repo.join("src/a.py"), "x = 1\n")?;
        fs::write(repo.join(".env"), "TOKEN=1\n")?;
        fs::write(repo.join("src/.env"), "TOKEN=2\n")?;
        fs::write(repo.join("secrets/key"), "k\n")?;
        fs::set_permissions(repo.join("src/a.py"), fs::Permissions::from_mode(0o600))?;
        std::os::unix::fs::symlink("/etc/passwd", repo.join("outside"))?;
        std::os::unix::fs::symlink(".env", repo.join("to-env"))?;

        let dest = base.join("work");
        let report = create(&repo, &dest, &policy())?;
        assert_eq!(report.denied, vec![".env", "secrets", "src/.env"]);
        assert!(!dest.join(".env").exists());
        assert!(!dest.join("src/.env").exists());
        assert!(!dest.join("secrets").exists());
        assert!(dest.join("empty").is_dir(), "empty folders are kept");
        assert_eq!(fs::read_to_string(dest.join("src/a.py"))?, "x = 1\n");
        assert_eq!(fs::metadata(dest.join("src/a.py"))?.permissions().mode() & 0o777, 0o600);
        assert_eq!(fs::read_link(dest.join("outside"))?, PathBuf::from("/etc/passwd"));
        // The link to a denied file is copied, but it now points at nothing.
        assert!(fs::symlink_metadata(dest.join("to-env"))?.file_type().is_symlink());
        assert!(fs::metadata(dest.join("to-env")).is_err());
        fs::remove_dir_all(&base)?;
        Ok(())
    }

    #[test]
    fn age_is_read_from_the_id() {
        assert_eq!(age_in_days(&new_execution_id()), Some(0));
        let today = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64 / 86_400;
        assert_eq!(age_in_days("19700101-000000000-abcdef"), Some(today));
        assert_eq!(age_in_days("19700102-000000000-abcdef"), Some(today - 1));
        assert_eq!(age_in_days("20000301-000000000-abcdef"), Some(today - 11_017), "the day after a leap day");
        assert_eq!(age_in_days("20000229-000000000-abcdef"), Some(today - 11_016));
        for bad in ["nope", "2026111-190203456-abcdef", "20261340-000000000-abcdef", "20261111"] {
            assert_eq!(age_in_days(bad), None, "{bad}");
        }
    }

    #[test]
    fn execution_ids_sort_by_time() {
        let a = new_execution_id();
        assert_eq!(a.len(), "20261111-190203456-abcdef".len());
        assert!(a.as_bytes()[8] == b'-' && a.as_bytes()[18] == b'-');
        // Two ids made a few milliseconds apart sort in the order they were made.
        std::thread::sleep(std::time::Duration::from_millis(3));
        let b = new_execution_id();
        assert!(a < b, "{a} should sort before {b}");
    }
}
