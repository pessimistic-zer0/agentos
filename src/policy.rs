//! The policy: a small TOML file that says which files the agent may read and
//! write, which commands it may run, and with what limits.
//!
//! Design rule 2: no matching rule means deny. Every decision names the rule
//! that produced it, so `agentos trace` can show *why* a call was allowed or
//! refused.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Errors in a policy file. Callers match on these (the CLI prints them, tests
/// check that a bad file is refused for the right reason), so they are typed.
#[derive(Debug, thiserror::Error)]
pub enum PolicyError {
    #[error("cannot read policy file {path}: {source}")]
    Read { path: PathBuf, source: std::io::Error },
    #[error("policy file {path} is not valid: {source}")]
    Parse { path: PathBuf, source: toml::de::Error },
    #[error("policy_version {0} is not supported (this runtime reads version 1)")]
    Version(u32),
    #[error("invalid glob `{glob}` in {field}: {reason}")]
    Glob { field: &'static str, glob: String, reason: &'static str },
    #[error("exec.allow[{0}] is empty; a rule must name at least the program")]
    EmptyExecRule(usize),
    #[error("sandbox.ro_mounts entry `{0}` must be an absolute path")]
    RelativeMount(String),
    #[error("limits.{0} must be greater than zero")]
    ZeroLimit(&'static str),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Policy {
    pub policy_version: u32,
    pub name: String,
    pub filesystem: FsRules,
    pub exec: ExecRules,
    pub sandbox: SandboxRules,
    pub limits: Limits,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FsRules {
    /// Paths `fs.read` and `fs.list` may open (relative to the workspace root).
    #[serde(default)]
    pub read: Vec<String>,
    /// Paths `fs.write` may replace, and paths `exec` may leave changed.
    #[serde(default)]
    pub write: Vec<String>,
    /// Paths that are never copied into the workspace and never opened.
    #[serde(default)]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExecRules {
    /// Allowed argv prefixes, e.g. `["pytest"]` or `["python3", "-m", "pytest"]`.
    #[serde(default)]
    pub allow: Vec<Vec<String>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SandboxRules {
    /// Host folders mounted read-only inside the sandbox (distro-specific; this
    /// is what the `fhs` and `nix` presets differ in).
    pub ro_mounts: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub memory_mb: u64,
    pub pids: u64,
    /// CPU share in percent of one core (200 = two cores' worth).
    pub cpu_percent: u64,
    /// Wall-clock limit for one `exec`; the watchdog kills the command after it.
    pub call_timeout_s: u64,
    /// Bytes of stdout and of stderr kept per `exec` (the rest is dropped).
    pub max_output_kb: u64,
    /// The run stops after this many tool calls.
    pub max_tool_calls: u64,
}

/// What the policy says about one call, and which rule said it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub allowed: bool,
    /// For example `filesystem.deny[0] ".env"` or `no matching rule`.
    pub rule: String,
}

impl Decision {
    fn allow(rule: String) -> Self {
        Decision { allowed: true, rule }
    }
    fn deny(rule: String) -> Self {
        Decision { allowed: false, rule }
    }
}

/// Which kind of file access is being asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Access {
    Read,
    Write,
}

impl Policy {
    pub fn load(path: &Path) -> Result<(Policy, String), PolicyError> {
        let text = std::fs::read_to_string(path)
            .map_err(|source| PolicyError::Read { path: path.to_path_buf(), source })?;
        let policy = Policy::parse(&text).map_err(|e| match e {
            PolicyError::Parse { source, .. } => PolicyError::Parse { path: path.to_path_buf(), source },
            other => other,
        })?;
        Ok((policy, text))
    }

    pub fn parse(text: &str) -> Result<Policy, PolicyError> {
        let policy: Policy = toml::from_str(text)
            .map_err(|source| PolicyError::Parse { path: PathBuf::from("<text>"), source })?;
        policy.validate()?;
        Ok(policy)
    }

    fn validate(&self) -> Result<(), PolicyError> {
        if self.policy_version != 1 {
            return Err(PolicyError::Version(self.policy_version));
        }
        for (field, globs) in [
            ("filesystem.read", &self.filesystem.read),
            ("filesystem.write", &self.filesystem.write),
            ("filesystem.deny", &self.filesystem.deny),
        ] {
            for g in globs {
                check_glob(g).map_err(|reason| PolicyError::Glob { field, glob: g.clone(), reason })?;
            }
        }
        for (i, rule) in self.exec.allow.iter().enumerate() {
            if rule.is_empty() || rule[0].is_empty() {
                return Err(PolicyError::EmptyExecRule(i));
            }
        }
        for m in &self.sandbox.ro_mounts {
            if !m.starts_with('/') {
                return Err(PolicyError::RelativeMount(m.clone()));
            }
        }
        let l = &self.limits;
        for (name, v) in [
            ("memory_mb", l.memory_mb),
            ("pids", l.pids),
            ("cpu_percent", l.cpu_percent),
            ("call_timeout_s", l.call_timeout_s),
            ("max_output_kb", l.max_output_kb),
            ("max_tool_calls", l.max_tool_calls),
        ] {
            if v == 0 {
                return Err(PolicyError::ZeroLimit(name));
            }
        }
        Ok(())
    }

    /// Is `rel` (a workspace-relative path with `/` separators, `""` for the
    /// root) denied outright? Used when the workspace is copied (D-008).
    pub fn denied(&self, rel: &str) -> Option<String> {
        first_match("filesystem.deny", &self.filesystem.deny, rel)
    }

    /// Decide a file access. Deny rules win over allow rules; no allow rule
    /// matching means deny.
    pub fn check_path(&self, access: Access, rel: &str) -> Decision {
        if let Some(rule) = self.denied(rel) {
            return Decision::deny(rule);
        }
        let (field, globs) = match access {
            Access::Read => ("filesystem.read", &self.filesystem.read),
            Access::Write => ("filesystem.write", &self.filesystem.write),
        };
        match first_match(field, globs, rel) {
            Some(rule) => Decision::allow(rule),
            None => Decision::deny("no matching rule".into()),
        }
    }

    /// Is this path inside the write globs (and not denied)? Used after `exec`
    /// to find changes that must be reverted (D-009).
    pub fn writable(&self, rel: &str) -> bool {
        self.check_path(Access::Write, rel).allowed
    }

    /// Decide an `exec` argv: allowed if some rule is a prefix of it.
    pub fn check_exec(&self, argv: &[String]) -> Decision {
        for (i, rule) in self.exec.allow.iter().enumerate() {
            if argv.len() >= rule.len() && argv[..rule.len()] == rule[..] {
                return Decision::allow(format!("exec.allow[{i}] {rule:?}"));
            }
        }
        Decision::deny("no matching rule".into())
    }

    /// The distinct program names that exec rules allow (`argv[0]` of each rule).
    pub fn programs(&self) -> Vec<String> {
        let mut out: Vec<String> = self.exec.allow.iter().map(|r| r[0].clone()).collect();
        out.sort();
        out.dedup();
        out
    }
}

fn first_match(field: &str, globs: &[String], rel: &str) -> Option<String> {
    globs
        .iter()
        .position(|g| glob_match(g, rel))
        .map(|i| format!("{field}[{i}] {:?}", globs[i]))
}

// ---------------------------------------------------------------------------
// Glob matching.
//
// A small matcher written here instead of a crate (see D-032). Rules:
// - A pattern and a path are split on `/` into components.
// - `**` as a whole component matches zero or more components, so `secrets/**`
//   matches `secrets`, `secrets/a` and `secrets/a/b`, and `**/.env` matches
//   `.env` at any depth.
// - Inside a component, `*` matches any run of characters and `?` matches one.
//   Neither matches `/`, because matching works on single components.
// - Everything else matches itself. There are no `[...]` classes.
// - Patterns are relative to the workspace root: no leading `/`, no `.` or
//   `..` components. The root itself is the empty path `""`, matched by `**`.
// ---------------------------------------------------------------------------

fn check_glob(g: &str) -> Result<(), &'static str> {
    if g.is_empty() {
        return Err("empty pattern");
    }
    if g.starts_with('/') {
        return Err("patterns are relative to the workspace; remove the leading /");
    }
    for part in g.split('/') {
        match part {
            "" => return Err("empty component (double / or trailing /)"),
            "." | ".." => return Err("`.` and `..` are not allowed in patterns"),
            p if p.contains("**") && p != "**" => return Err("`**` must be a whole component"),
            _ => {}
        }
    }
    Ok(())
}

pub fn glob_match(pattern: &str, path: &str) -> bool {
    let pat: Vec<&str> = pattern.split('/').collect();
    let comps: Vec<&str> = if path.is_empty() { Vec::new() } else { path.split('/').collect() };
    match_components(&pat, &comps)
}

fn match_components(pat: &[&str], path: &[&str]) -> bool {
    match pat.first() {
        None => path.is_empty(),
        Some(&"**") => {
            // Try letting `**` swallow 0, 1, 2, ... components.
            (0..=path.len()).any(|skip| match_components(&pat[1..], &path[skip..]))
        }
        Some(p) => !path.is_empty() && match_one(p.as_bytes(), path[0].as_bytes()) && match_components(&pat[1..], &path[1..]),
    }
}

/// Match one component against one pattern component (`*` and `?` wildcards).
fn match_one(p: &[u8], s: &[u8]) -> bool {
    match (p.first(), s.first()) {
        (None, None) => true,
        (Some(b'*'), _) => match_one(&p[1..], s) || (!s.is_empty() && match_one(p, &s[1..])),
        (Some(b'?'), Some(_)) => match_one(&p[1..], &s[1..]),
        (Some(a), Some(b)) if a == b => match_one(&p[1..], &s[1..]),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEMO: &str = r#"
policy_version = 1
name = "test"
[filesystem]
read = ["**"]
write = ["src/**", "tests/**"]
deny = [".env", ".env.*", "**/.env", "secrets/**", ".git/**"]
[exec]
allow = [["pytest"], ["python3", "-m", "pytest"]]
[sandbox]
ro_mounts = ["/usr"]
[limits]
memory_mb = 256
pids = 64
cpu_percent = 100
call_timeout_s = 30
max_output_kb = 64
max_tool_calls = 20
"#;

    fn demo() -> Policy {
        Policy::parse(DEMO).unwrap()
    }

    #[test]
    fn glob_basics() {
        assert!(glob_match("**", ""));
        assert!(glob_match("**", "a/b/c"));
        assert!(glob_match("src/**", "src"));
        assert!(glob_match("src/**", "src/a/b.py"));
        assert!(!glob_match("src/**", "srcx/a.py"));
        assert!(glob_match("**/.env", ".env"));
        assert!(glob_match("**/.env", "a/b/.env"));
        assert!(glob_match(".env.*", ".env.local"));
        assert!(!glob_match(".env.*", ".env"));
        assert!(glob_match("*.py", "a.py"));
        assert!(!glob_match("*.py", "dir/a.py"), "`*` must not cross `/`");
        assert!(glob_match("a?c", "abc"));
        assert!(!glob_match("a?c", "ac"));
        assert!(glob_match("**/__pycache__/**", "src/__pycache__/x.pyc"));
        assert!(glob_match("src/**/test_*.py", "src/test_a.py"));
    }

    #[test]
    fn deny_wins_over_allow() {
        let p = demo();
        let d = p.check_path(Access::Read, ".env");
        assert!(!d.allowed);
        assert_eq!(d.rule, r#"filesystem.deny[0] ".env""#);
        assert!(!p.check_path(Access::Read, "secrets/key.pem").allowed);
        assert!(p.check_path(Access::Read, "src/stats.py").allowed);
    }

    #[test]
    fn no_rule_means_deny() {
        let p = demo();
        let d = p.check_path(Access::Write, "pytest.ini");
        assert_eq!(d, Decision { allowed: false, rule: "no matching rule".into() });
        assert!(p.check_path(Access::Write, "src/stats.py").allowed);
    }

    #[test]
    fn exec_prefix_rules() {
        let p = demo();
        let argv = |a: &[&str]| a.iter().map(|s| s.to_string()).collect::<Vec<_>>();
        assert!(p.check_exec(&argv(&["pytest", "-q"])).allowed);
        assert!(p.check_exec(&argv(&["python3", "-m", "pytest", "-x"])).allowed);
        assert!(!p.check_exec(&argv(&["python3", "-c", "print(1)"])).allowed);
        assert!(!p.check_exec(&argv(&["/usr/bin/pytest"])).allowed, "paths do not match names");
        assert!(!p.check_exec(&argv(&["sh", "-c", "pytest"])).allowed);
        assert_eq!(p.programs(), vec!["pytest".to_string(), "python3".to_string()]);
    }

    #[test]
    fn bad_policies_are_refused() {
        let bad_glob = DEMO.replace(r#"write = ["src/**", "tests/**"]"#, r#"write = ["/etc/**"]"#);
        assert!(matches!(Policy::parse(&bad_glob), Err(PolicyError::Glob { .. })));
        let dotdot = DEMO.replace(r#"write = ["src/**", "tests/**"]"#, r#"write = ["../x"]"#);
        assert!(matches!(Policy::parse(&dotdot), Err(PolicyError::Glob { .. })));
        let version = DEMO.replace("policy_version = 1", "policy_version = 2");
        assert!(matches!(Policy::parse(&version), Err(PolicyError::Version(2))));
        let typo = DEMO.replace("memory_mb", "memory_mib");
        assert!(matches!(Policy::parse(&typo), Err(PolicyError::Parse { .. })));
        let zero = DEMO.replace("pids = 64", "pids = 0");
        assert!(matches!(Policy::parse(&zero), Err(PolicyError::ZeroLimit("pids"))));
        let rel = DEMO.replace(r#"ro_mounts = ["/usr"]"#, r#"ro_mounts = ["usr"]"#);
        assert!(matches!(Policy::parse(&rel), Err(PolicyError::RelativeMount(_))));
    }
}
