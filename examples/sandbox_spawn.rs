//! Benchmark 1 of 3: the cost of starting one command in the sandbox.
//!
//!   sandbox_spawn once              one sandboxed `true` (for hyperfine)
//!   sandbox_spawn samples <N> <out> N sandboxed and N direct `true` in one
//!                                   process; writes raw times (µs) to
//!                                   <out>/spawn-sandbox.txt and spawn-direct.txt
//!
//! `once` includes this program's own start and the startup checks; `samples`
//! measures only the spawn itself (cgroup create → helper → namespaces → new
//! root → Landlock → execve(true) → wait → cgroup removed).
//!
//! The helper is the `norn` binary next to this one (target/<profile>/norn).

use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use norn::policy::Policy;
use norn::sandbox::{self, cgroup::CgroupRoot, SandboxSpec};
use norn::tools::exec::Programs;
use anyhow::{bail, Context, Result};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let preset = std::env::var("NORN_TEST_PRESET").unwrap_or_else(|_| "nix".into());
    let text = std::fs::read_to_string(root.join("policies").join(format!("{preset}.toml")))?;
    // The preset, with `true` as the only allowed command.
    let start = text.find("allow = [").context("policy has no exec.allow")?;
    let end = start + text[start..].find("\n]").context("unterminated allow list")? + 2;
    let policy = Policy::parse(&format!("{}allow = [[\"true\"]]{}", &text[..start], &text[end..]))?;
    let programs = Programs::resolve(&policy, &std::env::var("PATH").unwrap_or_default())?;
    let program = programs.lookup("true").context("`true` not found")?.to_path_buf();
    // target/<profile>/examples/sandbox_spawn -> target/<profile>/norn
    let helper = std::env::current_exe()?.parent().and_then(|p| p.parent()).context("no parent")?.join("norn");
    if !helper.exists() {
        bail!("{} not found; build it first (cargo build --release)", helper.display());
    }
    let cgroups = CgroupRoot::init()?;
    let work = root.join("target/bench-spawn").join(std::process::id().to_string());
    std::fs::create_dir_all(&work)?;
    let work = std::fs::canonicalize(&work)?;

    let mut n = 0u64;
    let mut spawn = || -> Result<Duration> {
        n += 1;
        let name = format!("bench-{}-{n}", std::process::id());
        let spec = SandboxSpec {
            workspace: work.clone(),
            ro_mounts: policy.sandbox.ro_mounts.iter().map(PathBuf::from).collect(),
            program: program.clone(),
            argv: vec!["true".into()],
            env: vec![("PATH".into(), programs.search_path()), ("HOME".into(), "/work".into())],
            cgroup: cgroups.path_of(&name),
            parent_pid: std::process::id() as i32,
            uid: nix::unistd::getuid().as_raw(),
            gid: nix::unistd::getgid().as_raw(),
            max_file_bytes: policy.limits.workspace_bytes(),
            ro_dirs: Vec::new(),
            proxy: false,
        };
        let t = Instant::now();
        let cg = cgroups.create(&name, &policy.limits)?;
        let disk = sandbox::DiskLimit { dir: work.clone(), max_bytes: policy.limits.workspace_bytes() };
        let (out, _) = sandbox::run(&helper, &spec, &cg, Duration::from_secs(10), 4096, &disk, &[]).map_err(|e| anyhow::anyhow!("{e}"))?;
        let d = t.elapsed();
        if out.exit_code != Some(0) {
            bail!("sandboxed true failed: {out:?}");
        }
        Ok(d)
    };

    match args.get(1).map(String::as_str) {
        Some("once") => {
            spawn()?;
        }
        Some("samples") => {
            let count: usize = args.get(2).context("samples <N> <out>")?.parse()?;
            let out = PathBuf::from(args.get(3).context("samples <N> <out>")?);
            let us = |d: Duration| format!("{:.1}\n", d.as_secs_f64() * 1e6);
            let (mut sandboxed, mut direct) = (String::new(), String::new());
            for _ in 0..10 {
                spawn()?; // warm-up
            }
            for _ in 0..count {
                sandboxed.push_str(&us(spawn()?));
                let t = Instant::now();
                // arg0: on some systems `true` is a multi-call binary that checks its own name.
                let st = std::process::Command::new(&program).arg0("true").env_clear().status()?;
                direct.push_str(&us(t.elapsed()));
                if !st.success() {
                    bail!("direct true failed");
                }
            }
            std::fs::write(out.join("spawn-sandbox.txt"), sandboxed)?;
            std::fs::write(out.join("spawn-direct.txt"), direct)?;
        }
        _ => bail!("usage: sandbox_spawn once | samples <N> <out-dir>"),
    }
    let _ = std::fs::remove_dir_all(&work);
    Ok(())
}
