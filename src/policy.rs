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
    // The inner error is named `error`, not `source`: a field called
    // `source` is also reported as the error's cause, and the command line
    // then printed the same text twice.
    #[error("cannot read policy file {path}: {error}")]
    Read { path: PathBuf, error: std::io::Error },
    #[error("policy file {path} is not valid: {error}")]
    Parse { path: PathBuf, error: toml::de::Error },
    #[error("policy_version {0} is not supported (this runtime reads version 1)")]
    Version(u32),
    #[error("invalid glob `{glob}` in {field}: {reason}")]
    Glob { field: &'static str, glob: String, reason: &'static str },
    #[error("exec.allow[{0}] is empty; a rule must name at least the program")]
    EmptyExecRule(usize),
    #[error("sandbox.ro_mounts entry `{0}` must be an absolute path")]
    RelativeMount(String),
    #[error("sandbox.ro_mounts entry `{0}` cannot be used: the sandbox puts its own folder at /work, /tmp, /proc and /dev, and `/` would show the whole machine; keep the folder somewhere else (for example under your home folder)")]
    ReservedMount(String),
    #[error("sandbox.env name `{name}`: {reason}")]
    Env { name: String, reason: &'static str },
    #[error("limits.{0} must be greater than zero")]
    ZeroLimit(&'static str),
    #[error("external.service `{name}`: {reason}")]
    External { name: String, reason: &'static str },
    #[error("external.{0} must be greater than zero")]
    ZeroExternal(&'static str),
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
    /// Services `ext.call` may reach (the v1 "Bonus"). Absent in the shipped
    /// policies, so by default every `ext.call` is denied (design rule 2).
    #[serde(default)]
    pub external: ExternalRules,
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
    /// Paths that `deny` would match but that are not secrets, for example
    /// `.env.example` next to a `.env.*` deny rule. The glob language has
    /// no "not", so exceptions are their own list (D-100).
    #[serde(default)]
    pub deny_except: Vec<String>,
    /// Paths that are not copied into the workspace because they are big
    /// and not needed there (a virtualenv that is mounted read-only, a
    /// build folder). Unlike `deny` this says nothing about secrecy: it
    /// only shapes the copy (D-099).
    #[serde(default)]
    pub skip: Vec<String>,
    /// Paths where commands may leave files without it counting as a
    /// change: caches such as `.pytest_cache`. They are not reverted, not
    /// reported, and not in the patch (D-099).
    #[serde(default)]
    pub scratch: Vec<String>,
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
    /// Extra environment variables for every command, on top of `PATH`,
    /// `HOME` and `LANG` (which the runtime sets and a policy cannot
    /// change). Written as a `[sandbox.env]` table. Values are fixed text:
    /// nothing is taken from the runtime's own environment (D-092).
    #[serde(default)]
    pub env: std::collections::BTreeMap<String, String>,
}

/// Places the sandbox builds itself. A host folder mounted there would be
/// covered up, or would stop the sandbox from starting.
const RESERVED_MOUNTS: [&str; 4] = ["/work", "/tmp", "/proc", "/dev"];
/// Variables the runtime sets for every command.
const RESERVED_ENV: [&str; 3] = ["PATH", "HOME", "LANG"];

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

/// The `[external]` section: which outside services the gateway may call for
/// the agent, and how long it waits for an answer.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalRules {
    /// `[[external.service]]` entries. The agent names a service; it never
    /// supplies a URL.
    #[serde(rename = "service", default)]
    pub services: Vec<ExternalService>,
    /// How long one request may take before it counts as "no answer".
    #[serde(default = "default_ext_timeout_ms")]
    pub timeout_ms: u64,
    /// How many times one call is sent (with the same idempotency key)
    /// before the runtime gives up and leaves the call uncertain.
    #[serde(default = "default_ext_attempts")]
    pub attempts: u32,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ExternalService {
    pub name: String,
    /// Must be `http://127.0.0.1:<port>[/path]`: the same machine, by number
    /// (D-074). Anything else is refused when the policy is loaded.
    pub url: String,
}

fn default_ext_timeout_ms() -> u64 {
    2000
}

fn default_ext_attempts() -> u32 {
    3
}

impl Default for ExternalRules {
    fn default() -> Self {
        ExternalRules { services: Vec::new(), timeout_ms: default_ext_timeout_ms(), attempts: default_ext_attempts() }
    }
}

/// Is `url` exactly `http://127.0.0.1:<port>` or that followed by `/path`?
/// A plain text check is right here: the policy file is trusted, and the
/// point is to refuse a policy that names another machine, not to confine
/// an attacker.
fn is_loopback_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://127.0.0.1:") else { return false };
    let port = rest.split('/').next().unwrap_or("");
    let path_ok = rest.len() == port.len() || rest[port.len()..].starts_with('/');
    path_ok
        && !port.is_empty()
        && port.bytes().all(|b| b.is_ascii_digit())
        && port.parse::<u16>().map(|p| p != 0).unwrap_or(false)
        && !url.bytes().any(|b| b.is_ascii_whitespace() || b.is_ascii_control() || b == b'@' || b == b'#')
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
            .map_err(|error| PolicyError::Read { path: path.to_path_buf(), error })?;
        let policy = Policy::parse(&text).map_err(|e| match e {
            PolicyError::Parse { error, .. } => PolicyError::Parse { path: path.to_path_buf(), error },
            other => other,
        })?;
        Ok((policy, text))
    }

    pub fn parse(text: &str) -> Result<Policy, PolicyError> {
        let policy: Policy = toml::from_str(text)
            .map_err(|error| PolicyError::Parse { path: PathBuf::from("<text>"), error })?;
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
            ("filesystem.deny_except", &self.filesystem.deny_except),
            ("filesystem.skip", &self.filesystem.skip),
            ("filesystem.scratch", &self.filesystem.scratch),
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
            let tidy = m.trim_end_matches('/');
            if tidy.is_empty() || RESERVED_MOUNTS.iter().any(|r| tidy == *r || tidy.starts_with(&format!("{r}/"))) {
                return Err(PolicyError::ReservedMount(m.clone()));
            }
        }
        for (name, value) in &self.sandbox.env {
            let err = |reason| PolicyError::Env { name: name.clone(), reason };
            if name.is_empty() || name.contains('=') || name.contains('\0') {
                return Err(err("a name must not be empty or contain `=`"));
            }
            if RESERVED_ENV.contains(&name.as_str()) {
                return Err(err("PATH, HOME and LANG are set by the runtime and cannot be changed"));
            }
            if value.contains('\0') {
                return Err(err("the value contains a NUL byte"));
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
        if self.external.timeout_ms == 0 {
            return Err(PolicyError::ZeroExternal("timeout_ms"));
        }
        if self.external.attempts == 0 {
            return Err(PolicyError::ZeroExternal("attempts"));
        }
        for (i, s) in self.external.services.iter().enumerate() {
            let err = |reason| PolicyError::External { name: s.name.clone(), reason };
            if s.name.is_empty() {
                return Err(err("the name is empty"));
            }
            if self.external.services[..i].iter().any(|o| o.name == s.name) {
                return Err(err("the name is used twice"));
            }
            if !is_loopback_url(&s.url) {
                return Err(err("the url must be http://127.0.0.1:<port>[/path]"));
            }
        }
        Ok(())
    }

    /// Is `rel` (a workspace-relative path with `/` separators, `""` for the
    /// root) denied outright? Used when the workspace is copied (D-008).
    pub fn denied(&self, rel: &str) -> Option<String> {
        let rule = first_match("filesystem.deny", &self.filesystem.deny, rel)?;
        match first_match("filesystem.deny_except", &self.filesystem.deny_except, rel) {
            Some(_) => None,
            None => Some(rule),
        }
    }

    /// Is `rel` left out of the workspace copy by a `skip` rule?
    pub fn skipped(&self, rel: &str) -> Option<String> {
        first_match("filesystem.skip", &self.filesystem.skip, rel)
    }

    /// Is `rel` a scratch path (a cache)? A denied path never is.
    pub fn scratch(&self, rel: &str) -> bool {
        self.denied(rel).is_none() && first_match("filesystem.scratch", &self.filesystem.scratch, rel).is_some()
    }

    /// May a *command* leave this path changed? The write globs, plus the
    /// scratch paths. (`fs.write` by the agent needs a write glob.)
    pub fn command_may_change(&self, rel: &str) -> bool {
        self.writable(rel) || self.scratch(rel)
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

    /// Decide an `ext.call`: allowed only if the policy lists a service with
    /// exactly this name. Returns the service too, so the caller uses the
    /// URL of the rule that allowed the call.
    pub fn check_external(&self, service: &str) -> (Decision, Option<&ExternalService>) {
        match self.external.services.iter().position(|s| s.name == service) {
            Some(i) => (Decision::allow(format!("external.service[{i}] {service:?}")), Some(&self.external.services[i])),
            None => (Decision::deny("no matching rule".into()), None),
        }
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
    fn deny_exceptions_skip_and_scratch() {
        let p = Policy::parse(&DEMO.replace(
            "[exec]",
            "deny_except = [\".env.example\", \"**/.env.example\"]\nskip = [\".venv/**\"]\nscratch = [\".pytest_cache/**\", \"secrets/cache/**\"]\n[exec]",
        ))
        .unwrap();
        assert!(p.denied(".env.local").is_some());
        assert!(p.denied(".env.example").is_none(), "an exception lifts the deny rule");
        assert!(p.check_path(Access::Read, ".env.example").allowed);
        assert!(p.denied(".env").is_some(), "only what the exception names");
        assert!(p.skipped(".venv").is_some() && p.skipped(".venv/lib/x.py").is_some());
        assert!(p.skipped("src/a.py").is_none());
        assert!(p.denied(".venv/lib/x.py").is_none(), "skipped is not denied");
        assert!(p.scratch(".pytest_cache/v/cache/lastfailed"));
        assert!(!p.scratch("secrets/cache/x"), "deny wins over scratch");
        assert!(p.command_may_change(".pytest_cache/x") && !p.writable(".pytest_cache/x"));
        assert!(!p.check_path(Access::Write, ".pytest_cache/x").allowed, "fs.write still needs a write glob");
        assert!(demo().filesystem.skip.is_empty() && demo().filesystem.scratch.is_empty() && demo().filesystem.deny_except.is_empty());
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
        for reserved in ["/", "/tmp", "/tmp/venv", "/proc/1", "/dev", "/work/", "/work/x"] {
            let p = DEMO.replace(r#"ro_mounts = ["/usr"]"#, &format!(r#"ro_mounts = ["/usr", "{reserved}"]"#));
            assert!(matches!(Policy::parse(&p), Err(PolicyError::ReservedMount(_))), "{reserved}");
        }
        for fine in ["/home/u/.venvs/proj", "/opt/tools", "/tmpfiles", "/workspace"] {
            let p = DEMO.replace(r#"ro_mounts = ["/usr"]"#, &format!(r#"ro_mounts = ["/usr", "{fine}"]"#));
            assert!(Policy::parse(&p).is_ok(), "{fine}");
        }
    }

    #[test]
    fn sandbox_env_is_fixed_text_with_three_names_reserved() {
        assert!(demo().sandbox.env.is_empty(), "no [sandbox.env] means no extra variables");
        let with = |table: &str| Policy::parse(&DEMO.replace("[limits]", &format!("[sandbox.env]\n{table}\n[limits]")));
        let p = with("PYTHONPATH = \"/opt/deps\"\nEMPTY = \"\"").unwrap();
        assert_eq!(p.sandbox.env.get("PYTHONPATH").map(String::as_str), Some("/opt/deps"));
        assert_eq!(p.sandbox.env.get("EMPTY").map(String::as_str), Some(""));
        for reserved in ["PATH", "HOME", "LANG"] {
            assert!(matches!(with(&format!("{reserved} = \"x\"")), Err(PolicyError::Env { .. })), "{reserved}");
        }
        assert!(matches!(with("\"A=B\" = \"x\""), Err(PolicyError::Env { .. })));
        assert!(matches!(with("\"\" = \"x\""), Err(PolicyError::Env { .. })));
        assert!(matches!(with("N = 1"), Err(PolicyError::Parse { .. })), "values are text");
    }

    fn with_external(extra: &str) -> Result<Policy, PolicyError> {
        Policy::parse(&format!("{DEMO}\n{extra}"))
    }

    #[test]
    fn external_services_are_denied_unless_listed() {
        // No [external] section at all: nothing is allowed.
        let none = demo();
        let (d, s) = none.check_external("ledger");
        assert_eq!(d, Decision { allowed: false, rule: "no matching rule".into() });
        assert!(s.is_none());

        let p = with_external("[external]\ntimeout_ms = 300\n[[external.service]]\nname = \"ledger\"\nurl = \"http://127.0.0.1:8099/apply\"\n").unwrap();
        assert_eq!((p.external.timeout_ms, p.external.attempts), (300, 3), "attempts falls back to its default");
        let (d, s) = p.check_external("ledger");
        assert!(d.allowed);
        assert_eq!(d.rule, r#"external.service[0] "ledger""#);
        assert_eq!(s.unwrap().url, "http://127.0.0.1:8099/apply");
        assert!(!p.check_external("Ledger").0.allowed, "names match exactly");
        assert!(!p.check_external("").0.allowed);
    }

    #[test]
    fn external_urls_must_be_loopback_by_number() {
        for ok in ["http://127.0.0.1:1", "http://127.0.0.1:8099", "http://127.0.0.1:8099/", "http://127.0.0.1:65535/a/b?x=1"] {
            assert!(is_loopback_url(ok), "{ok}");
        }
        for bad in [
            "https://127.0.0.1:8099/",          // not plain http
            "http://localhost:8099/",           // a name, not the number
            "http://127.0.0.1/",                // no port
            "http://127.0.0.1:0/",              // port 0
            "http://127.0.0.1:99999/",          // not a port
            "http://127.0.0.1:80@evil.example/", // user-info trick: the real host is evil.example
            "http://127.0.0.1:8099.evil.example/",
            "http://127.0.0.1:8099#x",
            "http://127.0.0.2:8099/",
            "http://example.com:80/",
            "http://127.0.0.1:80 80/",
        ] {
            assert!(!is_loopback_url(bad), "{bad}");
            let text = format!("[[external.service]]\nname = \"s\"\nurl = \"{bad}\"\n");
            assert!(matches!(with_external(&text), Err(PolicyError::External { .. })), "{bad}");
        }
        let twice = "[[external.service]]\nname = \"s\"\nurl = \"http://127.0.0.1:1\"\n[[external.service]]\nname = \"s\"\nurl = \"http://127.0.0.1:2\"\n";
        assert!(matches!(with_external(twice), Err(PolicyError::External { .. })));
        assert!(matches!(with_external("[external]\nattempts = 0\n"), Err(PolicyError::ZeroExternal("attempts"))));
        assert!(matches!(with_external("[external]\ntimeout_ms = 0\n"), Err(PolicyError::ZeroExternal("timeout_ms"))));
        assert!(matches!(with_external("[external]\nretries = 3\n"), Err(PolicyError::Parse { .. })), "unknown keys are refused");
    }
}
