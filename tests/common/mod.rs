//! Helpers shared by the integration tests. Each test gets its own state
//! folder and drives the real `agentos` binary.
#![allow(dead_code)] // each test file uses a different subset

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use serde_json::Value;

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

pub fn agentos_bin() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_agentos"))
}

/// The policy preset under test: `AGENTOS_TEST_PRESET=fhs|nix` (default nix).
/// Both suites must pass on both presets (CLAUDE.md, "Portability").
pub fn preset() -> String {
    std::env::var("AGENTOS_TEST_PRESET").unwrap_or_else(|_| "nix".into())
}

pub fn preset_policy() -> PathBuf {
    repo_root().join("policies").join(format!("{}.toml", preset()))
}

pub struct TestEnv {
    pub root: PathBuf,
    pub state: PathBuf,
    pub policy: PathBuf,
}

impl TestEnv {
    pub fn new(name: &str) -> TestEnv {
        let root = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let state = root.join("state");
        TestEnv { root, state, policy: preset_policy() }
    }

    /// Use a policy written for this test instead of the preset.
    pub fn with_policy_text(mut self, text: &str) -> TestEnv {
        let p = self.root.join("policy.toml");
        std::fs::write(&p, text).unwrap();
        self.policy = p;
        self
    }

    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut c = Command::new(agentos_bin());
        c.arg("--state-dir").arg(&self.state).args(args).current_dir(repo_root());
        c
    }

    pub fn output(&self, args: &[&str]) -> Output {
        self.cmd(args).output().expect("running agentos")
    }

    /// Run agentos and return stdout; panic with stderr if it fails.
    pub fn agentos(&self, args: &[&str]) -> String {
        let out = self.output(args);
        if !out.status.success() {
            panic!(
                "agentos {args:?} failed ({}):\nstdout:\n{}\nstderr:\n{}",
                out.status,
                String::from_utf8_lossy(&out.stdout),
                String::from_utf8_lossy(&out.stderr)
            );
        }
        String::from_utf8(out.stdout).unwrap()
    }

    /// `agentos run` with the scripted agent on the demo repo; returns the id.
    pub fn run_script(&self, script: &str) -> String {
        self.run_script_on(script, &repo_root().join("examples/demo-repo"))
    }

    pub fn run_script_on(&self, script: &str, repo: &Path) -> String {
        let out = self.agentos(&[
            "run",
            "--quiet",
            "--repo",
            repo.to_str().unwrap(),
            "--policy",
            self.policy.to_str().unwrap(),
            "--agent",
            "scripted",
            "--script",
            script,
        ]);
        out.trim().to_string()
    }

    pub fn write_script(&self, name: &str, text: &str) -> PathBuf {
        let p = self.root.join(name);
        std::fs::write(&p, text).unwrap();
        p
    }

    pub fn exec_dir(&self, id: &str) -> PathBuf {
        self.state.join("executions").join(id)
    }

    /// All events of an execution, from `agentos trace --json`.
    pub fn events(&self, id: &str) -> Vec<Value> {
        self.agentos(&["trace", id, "--json"]).lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }

    /// The full tool output stored in the blob store for a completed call.
    pub fn output_of(&self, id: &str, events: &[Value], call_id: u64) -> Option<Value> {
        let done = events.iter().find(|e| e["type"] == "tool.completed" && e["call_id"] == call_id)?;
        let hash = done["output"].as_str()?.strip_prefix("sha256:")?.to_string();
        let path = self.exec_dir(id).join("blobs").join(&hash[..2]).join(&hash);
        Some(serde_json::from_slice(&std::fs::read(path).ok()?).unwrap())
    }
}

/// Exit code of each completed `exec` call, in order.
pub fn exec_exit_codes(t: &TestEnv, events: &[Value]) -> Vec<Option<i64>> {
    let id = events[0]["execution_id"].as_str().unwrap().to_string();
    let exec_calls: Vec<u64> = events
        .iter()
        .filter(|e| e["type"] == "tool.requested" && e["call"]["tool"] == "exec")
        .map(|e| e["call_id"].as_u64().unwrap())
        .collect();
    exec_calls
        .into_iter()
        .filter_map(|c| t.output_of(&id, events, c))
        .map(|o| o["exit_code"].as_i64())
        .collect()
}

/// A fingerprint of a whole folder: every path, its type, mode and content.
pub fn tree_digest(dir: &Path) -> Vec<String> {
    use std::os::unix::fs::PermissionsExt;
    let mut out = Vec::new();
    fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir).unwrap().map(|e| e.unwrap()).collect();
        entries.sort_by_key(|e| e.file_name());
        for e in entries {
            let p = e.path();
            let rel = p.strip_prefix(base).unwrap().display().to_string();
            let m = std::fs::symlink_metadata(&p).unwrap();
            let mode = m.permissions().mode();
            if m.file_type().is_symlink() {
                out.push(format!("L {rel} {mode:o} -> {}", std::fs::read_link(&p).unwrap().display()));
            } else if m.is_dir() {
                out.push(format!("D {rel} {mode:o}"));
                walk(base, &p, out);
            } else {
                let bytes = std::fs::read(&p).unwrap();
                out.push(format!("F {rel} {mode:o} {} {}", bytes.len(), simple_hash(&bytes)));
            }
        }
    }
    walk(dir, dir, &mut out);
    out
}

fn simple_hash(b: &[u8]) -> u64 {
    // FNV-1a: enough to tell files apart in a test; not a security hash.
    let mut h: u64 = 0xcbf29ce484222325;
    for x in b {
        h ^= *x as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}
