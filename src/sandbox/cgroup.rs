//! cgroup v2: one control group per `exec`, with memory, process-count and
//! CPU limits, created inside a subtree that systemd delegated to us (D-013).
//!
//! A cgroup is a folder under `/sys/fs/cgroup`. Writing a number into a file
//! in that folder sets a limit; writing a process id into `cgroup.procs`
//! moves the process into the group. Rootless use works only inside a
//! subtree the user owns, which `systemd-run --user --scope -p Delegate=yes`
//! provides. The cgroup files are written directly; no crate is used (v1
//! stack table).

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};

use crate::policy::Limits;

const CGROUP_FS: &str = "/sys/fs/cgroup";
/// The runtime and its parents live in this child of the delegated group.
const LEAF: &str = "norn-runtime";
const NEEDED: [&str; 3] = ["memory", "pids", "cpu"];

/// The delegated cgroup under which per-exec groups are made.
#[derive(Debug, Clone)]
pub struct CgroupRoot {
    pub dir: PathBuf,
}

/// The cgroup (as a path below /sys/fs/cgroup) this process is in.
fn own_cgroup() -> Result<PathBuf> {
    let text = fs::read_to_string("/proc/self/cgroup").context("reading /proc/self/cgroup")?;
    // cgroup v2 has a single line "0::/path".
    let line = text.lines().find(|l| l.starts_with("0::")).context("no cgroup v2 entry in /proc/self/cgroup (cgroup v1 is not supported)")?;
    Ok(Path::new(CGROUP_FS).join(line[3..].trim_start_matches('/')))
}

/// The error for "not in a delegated cgroup". It says what to do first and
/// what was found second: the finding (a missing `cpu` controller, say)
/// means nothing to someone who has not met cgroups, the command does.
fn not_delegated(found: String) -> anyhow::Error {
    anyhow::anyhow!(
        "norn must be started inside a cgroup that systemd has handed over to it, \
         because it sets memory, process and CPU limits for the commands it runs. Start it like this:\n      \
         systemd-run --user --scope -p Delegate=yes norn run ...\n    (what was found: {found})"
    )
}

/// True only if `systemctl` clearly answers `Delegate=no` for the unit, in
/// the user manager or the system manager.
fn systemd_says_not_delegated(unit: &str) -> bool {
    let ask = |user: bool| {
        let mut c = std::process::Command::new("systemctl");
        if user {
            c.arg("--user");
        }
        c.args(["show", unit, "-p", "Delegate", "--value"]).output().ok().filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    let answers: Vec<String> = [ask(true), ask(false)].into_iter().flatten().collect();
    !answers.is_empty() && answers.iter().all(|a| a == "no") 
}

impl CgroupRoot {
    /// Find the delegated group and prepare it: move every process in it into
    /// a leaf child, then enable the memory, pids and cpu controllers for
    /// children. (cgroup v2's "no internal processes" rule: a group that
    /// hands controllers to children may not hold processes itself;
    /// `experiments/m2-pids`.)
    pub fn init() -> Result<CgroupRoot> {
        let mut dir = own_cgroup()?;
        if dir.file_name().and_then(|n| n.to_str()) == Some(LEAF) {
            // Another norn process in this scope already set things up, and
            // we were started inside its leaf.
            dir = dir.parent().context("leaf cgroup has no parent")?.to_path_buf();
        }
        let controllers = fs::read_to_string(dir.join("cgroup.controllers")).unwrap_or_default();
        let missing: Vec<&str> = NEEDED.iter().copied().filter(|c| !controllers.split_whitespace().any(|x| x == *c)).collect();
        if !missing.is_empty() {
            return Err(not_delegated(format!("cgroup {} does not offer the {} controller(s)", dir.display(), missing.join(", "))));
        }
        // Ask systemd whether this unit was delegated to us, *before* changing
        // anything. Without this, running in a normal (not delegated) scope
        // such as a terminal's would move that scope's processes around and
        // then fail later, half-way through a run (Findings, 2026-11-11).
        // If systemd cannot be asked (no systemctl), the probe below decides.
        if let Some(unit) = dir.file_name().and_then(|n| n.to_str()) {
            if systemd_says_not_delegated(unit) {
                return Err(not_delegated(format!("systemd reports Delegate=no for {unit}")));
            }
        }
        let leaf = dir.join(LEAF);
        match fs::create_dir(&leaf) {
            Ok(()) => {}
            // Another norn process in this scope got there first: fine.
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(not_delegated(format!("cannot create a child cgroup in {}: {e}", dir.display()))),
        }
        let enabled = |dir: &Path| {
            let on = fs::read_to_string(dir.join("cgroup.subtree_control")).unwrap_or_default();
            NEEDED.iter().all(|c| on.split_whitespace().any(|x| x == *c))
        };
        let want: String = NEEDED.iter().map(|c| format!("+{c} ")).collect();
        // Several norn processes may start in the same scope at the same
        // moment (the test suites do this). A process that is spawned into
        // the scope's root between our "move everyone" and our "enable
        // controllers" makes the enable fail with EBUSY, so both steps are
        // retried together for a short while.
        let mut last_err = None;
        for _ in 0..200 {
            if enabled(&dir) {
                last_err = None;
                break;
            }
            // Move everyone (us, and e.g. the shell systemd-run started) into the leaf.
            let procs = fs::read_to_string(dir.join("cgroup.procs")).unwrap_or_default();
            for pid in procs.split_whitespace() {
                // A process may exit between the read and the write; that is fine.
                let _ = fs::write(leaf.join("cgroup.procs"), pid);
            }
            match fs::write(dir.join("cgroup.subtree_control"), want.trim()) {
                Ok(()) => {
                    last_err = None;
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }
        if let Some(e) = last_err {
            return Err(not_delegated(format!("cannot enable controllers in {}: {e}", dir.display())));
        }
        let root = CgroupRoot { dir };
        root.probe()?;
        Ok(root)
    }

    /// Create a throwaway group, set all three limits, remove it. If this
    /// works, a real exec group will work too.
    fn probe(&self) -> Result<()> {
        let limits = Limits { memory_mb: 64, pids: 16, cpu_percent: 100, call_timeout_s: 1, max_output_kb: 1, max_tool_calls: 1, workspace_mb: 1 };
        let name = format!("norn-probe-{}", std::process::id());
        let cg = self
            .create(&name, &limits)
            .map_err(|e| not_delegated(format!("cannot set limits in a child of {}: {e:#}", self.dir.display())))?;
        cg.destroy()
    }

    /// The path an exec cgroup with this name would have.
    pub fn path_of(&self, name: &str) -> PathBuf {
        self.dir.join(name)
    }

    /// Make a new group with the policy's limits.
    pub fn create(&self, name: &str, limits: &Limits) -> Result<ExecCgroup> {
        let dir = self.path_of(name);
        if dir.exists() {
            // Left over from a crash: empty it and start fresh.
            ExecCgroup { dir: dir.clone() }.destroy()?;
        }
        fs::create_dir(&dir).with_context(|| format!("creating cgroup {}", dir.display()))?;
        let cg = ExecCgroup { dir };
        cg.write("memory.max", &(limits.memory_mb * 1024 * 1024).to_string())?;
        // No swap: otherwise memory.max only moves the excess to swap.
        if cg.dir.join("memory.swap.max").exists() {
            cg.write("memory.swap.max", "0")?;
        }
        cg.write("pids.max", &limits.pids.to_string())?;
        // cpu.max is "<quota> <period>" in microseconds: 200% = 200000 per 100000.
        cg.write("cpu.max", &format!("{} 100000", limits.cpu_percent * 1000))?;
        Ok(cg)
    }
}

/// One `exec`'s cgroup.
#[derive(Debug, Clone)]
pub struct ExecCgroup {
    pub dir: PathBuf,
}

impl ExecCgroup {
    pub fn open(dir: PathBuf) -> ExecCgroup {
        ExecCgroup { dir }
    }

    fn write(&self, file: &str, value: &str) -> Result<()> {
        fs::write(self.dir.join(file), value).with_context(|| format!("writing {value} to {}/{file}", self.dir.display()))
    }

    /// Kill every process in the group at once (kernel 5.14+). No process
    /// id is involved, so there is no race with id reuse.
    pub fn kill(&self) -> Result<()> {
        self.write("cgroup.kill", "1")
    }

    pub fn is_populated(&self) -> bool {
        fs::read_to_string(self.dir.join("cgroup.events"))
            .map(|t| t.lines().any(|l| l == "populated 1"))
            .unwrap_or(false)
    }

    /// Did the kernel's OOM killer kill something in this group?
    pub fn oom_killed(&self) -> bool {
        fs::read_to_string(self.dir.join("memory.events"))
            .ok()
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("oom_kill ").and_then(|n| n.trim().parse::<u64>().ok()))
            })
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    /// Did a fork fail because of `pids.max`?
    pub fn pids_limit_hit(&self) -> bool {
        fs::read_to_string(self.dir.join("pids.events"))
            .ok()
            .and_then(|t| t.lines().find_map(|l| l.strip_prefix("max ").and_then(|n| n.trim().parse::<u64>().ok())))
            .map(|n| n > 0)
            .unwrap_or(false)
    }

    /// Kill whatever is left, wait until the group is empty, remove it.
    pub fn destroy(&self) -> Result<()> {
        if !self.dir.exists() {
            return Ok(());
        }
        let _ = self.kill();
        let deadline = Instant::now() + Duration::from_secs(5);
        while self.is_populated() {
            if Instant::now() > deadline {
                bail!("cgroup {} still has processes 5 s after cgroup.kill", self.dir.display());
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        // rmdir can briefly fail with EBUSY right after the last process exits.
        for _ in 0..100 {
            match fs::remove_dir(&self.dir) {
                Ok(()) => return Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
                Err(_) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        fs::remove_dir(&self.dir).with_context(|| format!("removing cgroup {}", self.dir.display()))
    }
}
