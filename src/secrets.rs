//! Secrets the runtime uses on the agent's behalf (M21; D-153).
//!
//! A policy names secrets and where each one comes from:
//!
//! ```toml
//! [secrets]
//! TRACKER_TOKEN = { from = "env" }                      # the runtime's environment
//! DEPLOY_KEY    = { from = "file", path = "/home/me/.config/deploy-key" }
//! ```
//!
//! The runtime reads them when it starts, and uses them in one place only:
//! the headers of an `ext.call` to an HTTPS service, which the runtime sends
//! itself, outside the sandbox (`Authorization = "Bearer {secret:TRACKER_TOKEN}"`).
//! So a secret never enters the sandbox, the workspace, the event log, the
//! blob store or the patch, and the agent never sees it.
//!
//! As a backstop (S3), anything the runtime stores or shows that came from
//! outside (a service's answer that echoes a header back, a command's
//! output) has each secret's value replaced by `[secret:NAME]` first.

use anyhow::{bail, Context, Result};
use serde_json::Value;

use crate::policy::Policy;

/// Shorter values are refused: replacing a 3-letter value everywhere it
/// appears would mangle unrelated text, and such a secret is no secret.
pub const MIN_LEN: usize = 8;

/// The secrets of one policy, with their values. `Debug` prints names only.
#[derive(Clone, Default)]
pub struct Secrets {
    values: Vec<(String, String)>,
}

impl std::fmt::Debug for Secrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.values.iter().map(|(n, _)| n)).finish()
    }
}

impl Secrets {
    /// Read every secret the policy names. An error names the secret and
    /// where it was looked for, never a value.
    pub fn load(policy: &Policy) -> Result<Secrets> {
        let mut values = Vec::new();
        for (name, src) in &policy.secrets {
            let value = match src.from.as_str() {
                "env" => {
                    let var = src.var.clone().unwrap_or_else(|| name.clone());
                    std::env::var(&var).with_context(|| format!("secret {name}: the environment variable {var} is not set (or not valid UTF-8)"))?
                }
                "file" => {
                    let path = src.path.as_deref().unwrap_or_default();
                    let text = std::fs::read_to_string(path).with_context(|| format!("secret {name}: cannot read {path}"))?;
                    text.strip_suffix('\n').map(|t| t.strip_suffix('\r').unwrap_or(t)).unwrap_or(&text).to_string()
                }
                other => bail!("secret {name}: unknown source `{other}`"),
            };
            if value.chars().count() < MIN_LEN {
                bail!("secret {name}: the value is shorter than {MIN_LEN} characters; that is too short to keep out of logs safely");
            }
            if value.bytes().any(|b| b == b'\r' || b == b'\n' || b == 0) {
                bail!("secret {name}: the value has a line break; it could not be sent in a header");
            }
            values.push((name.clone(), value));
        }
        Ok(Secrets { values })
    }

    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    pub fn names(&self) -> Vec<&str> {
        self.values.iter().map(|(n, _)| n.as_str()).collect()
    }

    /// `template` with each `{secret:NAME}` replaced by the value.
    pub fn expand(&self, template: &str) -> Result<String> {
        let mut out = template.to_string();
        while let Some(at) = out.find("{secret:") {
            let end = out[at..].find('}').map(|e| at + e).context("unclosed {secret:")?;
            let name = &out[at + "{secret:".len()..end];
            let value = self.values.iter().find(|(n, _)| n == name).map(|(_, v)| v.clone()).with_context(|| format!("unknown secret {name}"))?;
            out.replace_range(at..=end, &value);
        }
        Ok(out)
    }

    /// `text` with every secret's value replaced by `[secret:NAME]`.
    /// Longest first, so a value that contains another is replaced whole.
    pub fn redact(&self, text: &str) -> String {
        if self.values.is_empty() {
            return text.to_string();
        }
        let mut order: Vec<&(String, String)> = self.values.iter().collect();
        order.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        let mut out = text.to_string();
        for (name, value) in order {
            if out.contains(value.as_str()) {
                out = out.replace(value.as_str(), &format!("[secret:{name}]"));
            }
        }
        out
    }

    /// Every string in a JSON value, redacted.
    pub fn redact_json(&self, v: &mut Value) {
        if self.values.is_empty() {
            return;
        }
        match v {
            Value::String(s) => *s = self.redact(s),
            Value::Array(a) => a.iter_mut().for_each(|x| self.redact_json(x)),
            Value::Object(o) => o.values_mut().for_each(|x| self.redact_json(x)),
            _ => {}
        }
    }

    /// Does `text` hold any secret's value?
    pub fn found_in(&self, text: &[u8]) -> Vec<&str> {
        self.values.iter().filter(|(_, v)| text.windows(v.len()).any(|w| w == v.as_bytes())).map(|(n, _)| n.as_str()).collect()
    }

    #[cfg(test)]
    pub fn for_test(pairs: &[(&str, &str)]) -> Secrets {
        Secrets { values: pairs.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expand_fills_in_and_redact_takes_out() {
        let s = Secrets::for_test(&[("TOKEN", "tok-1234567890"), ("LONG", "tok-1234567890-and-more")]);
        assert_eq!(s.expand("Bearer {secret:TOKEN}").unwrap(), "Bearer tok-1234567890");
        assert!(s.expand("{secret:NOPE}").is_err());
        assert_eq!(s.redact("a tok-1234567890-and-more b tok-1234567890"), "a [secret:LONG] b [secret:TOKEN]");
        let mut v = serde_json::json!({"body": "Authorization: Bearer tok-1234567890", "n": 1, "list": ["tok-1234567890"]});
        s.redact_json(&mut v);
        assert_eq!(v, serde_json::json!({"body": "Authorization: Bearer [secret:TOKEN]", "n": 1, "list": ["[secret:TOKEN]"]}));
        assert_eq!(s.found_in(b"xx tok-1234567890 yy"), vec!["TOKEN"]);
        assert!(format!("{s:?}").contains("TOKEN") && !format!("{s:?}").contains("tok-"), "Debug shows names only");
    }

    #[test]
    fn load_reads_env_and_files_and_refuses_short_values() {
        let dir = std::env::temp_dir().join(format!("norn-secrets-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("key");
        std::fs::write(&file, "file-secret-value\n").unwrap();
        std::env::set_var("NORN_TEST_SECRET_ENV_A", "env-secret-value");
        let base = "policy_version = 1\nname = \"t\"\n[filesystem]\n[exec]\n[sandbox]\nro_mounts = [\"/usr\"]\n[limits]\nmemory_mb = 1\npids = 1\ncpu_percent = 1\ncall_timeout_s = 1\nmax_output_kb = 1\nmax_tool_calls = 1\n";
        let p = Policy::parse(&format!("{base}[secrets]\nA = {{ from = \"env\", var = \"NORN_TEST_SECRET_ENV_A\" }}\nB = {{ from = \"file\", path = \"{}\" }}\n", file.display())).unwrap();
        let s = Secrets::load(&p).unwrap();
        assert_eq!(s.expand("{secret:A}/{secret:B}").unwrap(), "env-secret-value/file-secret-value");
        let missing = Policy::parse(&format!("{base}[secrets]\nC = {{ from = \"env\", var = \"NORN_TEST_SECRET_NOT_SET\" }}\n")).unwrap();
        let e = format!("{:#}", Secrets::load(&missing).unwrap_err());
        assert!(e.contains("NORN_TEST_SECRET_NOT_SET"), "{e}");
        std::env::set_var("NORN_TEST_SECRET_SHORT", "abc");
        let short = Policy::parse(&format!("{base}[secrets]\nD = {{ from = \"env\", var = \"NORN_TEST_SECRET_SHORT\" }}\n")).unwrap();
        let e = format!("{:#}", Secrets::load(&short).unwrap_err());
        assert!(e.contains("shorter than 8") && !e.contains("abc"), "{e}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
