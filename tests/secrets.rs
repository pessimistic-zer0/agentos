//! Secrets used by the runtime, for `ext.call` to HTTPS services (M21; D-153).
//!
//! The runtime reads a secret at start and puts it into a request header
//! itself, outside the sandbox. These tests check that the service gets it,
//! and that it never shows up anywhere else: the event log, the blob store,
//! the workspace, the patch, a command's environment, or what the agent is
//! told, even when the service echoes it back.
//!
//! The service is the fake service of the bonus, served over HTTPS with the
//! test certificate in `tests/fixtures/tls/` (D-157).
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test secrets

mod common;

use std::path::{Path, PathBuf};
use std::time::Duration;

use common::fake_service::{FakeService, Fault};
use common::*;
use serde_json::Value;

const TOKEN: &str = "tok-7f3a9c1e5b2d8460";
const VAR: &str = "NORN_TEST_TRACKER_TOKEN";

fn ca() -> PathBuf {
    repo_root().join("tests/fixtures/tls/ca.pem")
}

/// The preset, `python3 -c` allowed, and one HTTPS service with a secret header.
fn policy(url: &str, extra_service_lines: &str) -> String {
    let base = std::fs::read_to_string(preset_policy()).unwrap().replace("  [\"pytest\"],\n", "  [\"pytest\"],\n  [\"python3\", \"-c\"],\n");
    assert!(base.contains("[\"python3\", \"-c\"]"), "the preset's exec list changed; update this test");
    format!(
        "{base}\n[secrets]\nTRACKER_TOKEN = {{ from = \"env\", var = \"{VAR}\" }}\n\n[external]\ntimeout_ms = 2000\nattempts = 1\n\n\
         [[external.service]]\nname = \"tracker\"\nurl = \"{url}\"\nca_file = \"{}\"\n{extra_service_lines}\n\
         [external.service.headers]\nAuthorization = \"Bearer {{secret:TRACKER_TOKEN}}\"\n",
        ca().display()
    )
}

fn run(t: &TestEnv, script: &Path, with_secret: bool) -> std::process::Output {
    let mut c = t.cmd(&["run", "--quiet", "--repo", repo_root().join("examples/demo-repo").to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()]);
    if with_secret {
        c.env(VAR, TOKEN);
    } else {
        c.env_remove(VAR);
    }
    c.output().unwrap()
}

/// Every file under `dir` whose bytes contain `needle`.
fn files_containing(dir: &Path, needle: &[u8]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap().flatten() {
            let p = e.path();
            let m = std::fs::symlink_metadata(&p).unwrap();
            if m.is_dir() {
                stack.push(p);
            } else if m.is_file() && std::fs::read(&p).unwrap().windows(needle.len()).any(|w| w == needle) {
                out.push(p);
            }
        }
    }
    out
}

#[test]
fn the_service_gets_the_secret_and_nothing_else_does() {
    let svc = FakeService::start_tls(0, Duration::from_secs(3)).unwrap();
    svc.require_auth(&format!("Bearer {TOKEN}"));
    let t = TestEnv::new("sec-header").with_policy_text(&policy(&svc.url(), "approval = false"));
    let script = t.write_script(
        "s.toml",
        "task = \"secrets\"\n\
         [[step]]\ntool = \"ext.call\"\nservice = \"tracker\"\npayload = \"echo order-1\"\n\
         [[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", \"import os; print(sorted(os.environ.items())); print(open('/proc/self/environ').read())\"]\n",
    );
    let out = run(&t, &script, true);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();

    // The service got the header, over HTTPS, exactly once.
    let seen = svc.seen();
    assert_eq!(seen.len(), 1, "{seen:?}");
    assert_eq!(seen[0].authorization.as_deref(), Some(format!("Bearer {TOKEN}").as_str()));
    assert_eq!(seen[0].outcome, "applied");

    // The service echoed it back; the agent saw it replaced.
    let events = t.events(&id);
    let answer = t.output_of(&id, &events, 1).expect("the call completed");
    let body = answer["body"].as_str().unwrap();
    assert!(body.contains("Bearer [secret:TRACKER_TOKEN]"), "{body}");
    // The command's environment does not have it.
    let env = t.output_of(&id, &events, 2).unwrap();
    assert_eq!(env["exit_code"], 0, "{env}");
    assert!(!env["stdout"].as_str().unwrap().contains("tok-"), "{env}");

    // And no file of the execution holds it: log, blobs, workspace,
    // checkpoints, patch, the saved policy.
    let found = files_containing(&t.exec_dir(&id), TOKEN.as_bytes());
    assert!(found.is_empty(), "the secret's value is in {found:?}");
    // The intent names the header's template, not its value.
    let intent = events.iter().find(|e| e["type"] == "tool.intent" && e["call_id"] == 1).unwrap();
    assert_eq!(intent["plan"]["headers"]["Authorization"], "Bearer {secret:TRACKER_TOKEN}");
}

#[test]
fn a_remote_service_waits_for_approval_by_default() {
    let svc = FakeService::start_tls(0, Duration::from_secs(3)).unwrap();
    let t = TestEnv::new("sec-approval").with_policy_text(&policy(&svc.url(), ""));
    let script = t.write_script("s.toml", "task = \"s\"\n[[step]]\ntool = \"ext.call\"\nservice = \"tracker\"\npayload = \"order-1\"\n");
    let out = run(&t, &script, true);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let asked: Vec<&Value> = events.iter().filter(|e| e["type"] == "approval.requested").collect();
    assert_eq!(asked.len(), 1, "{events:#?}");
    assert!(asked[0]["rule"].as_str().unwrap().contains("a remote service"), "{}", asked[0]);
    assert!(svc.seen().is_empty(), "nothing was sent");
}

#[test]
fn a_missing_secret_stops_the_run_before_it_starts() {
    let svc = FakeService::start_tls(0, Duration::from_secs(3)).unwrap();
    let t = TestEnv::new("sec-missing").with_policy_text(&policy(&svc.url(), "approval = false"));
    let script = t.write_script("s.toml", "task = \"s\"\n[[step]]\ntool = \"ext.call\"\nservice = \"tracker\"\npayload = \"order-1\"\n");
    let out = run(&t, &script, false);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains(&format!("the environment variable {VAR} is not set")), "{err}");
    assert!(!t.state.join("executions").exists() || std::fs::read_dir(t.state.join("executions")).unwrap().count() == 0, "no execution was made");
    let check = t.output(&["check", "--policy", t.policy.to_str().unwrap()]);
    assert!(!check.status.success(), "norn check counts it as a problem");
    assert!(String::from_utf8_lossy(&check.stdout).contains("secrets: PROBLEM"));
}

#[test]
fn a_service_with_an_untrusted_certificate_never_gets_the_secret() {
    // The same service, but the policy names no CA: its certificate is not
    // signed by anyone in the usual list, so the handshake fails and the
    // header is never sent. Like any request without an answer, the call
    // is then uncertain (D-077) and the run stops.
    let svc = FakeService::start_tls(0, Duration::from_secs(3)).unwrap();
    let text = policy(&svc.url(), "approval = false").replace(&format!("ca_file = \"{}\"\n", ca().display()), "");
    assert!(!text.contains("ca_file"));
    let t = TestEnv::new("sec-untrusted").with_policy_text(&text);
    let script = t.write_script("s.toml", "task = \"s\"\n[[step]]\ntool = \"ext.call\"\nservice = \"tracker\"\npayload = \"order-1\"\n");
    let out = run(&t, &script, true);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("UNCERTAIN") && err.contains("UnknownIssuer"), "{err}");
    assert!(svc.seen().is_empty(), "no HTTP request reached the service");
}

#[test]
fn recovery_sends_the_same_request_with_the_secret_again() {
    let svc = FakeService::start_tls(0, Duration::from_millis(2500)).unwrap();
    svc.require_auth(&format!("Bearer {TOKEN}"));
    svc.inject("order-9", &[Fault::DropAfter]);
    let t = TestEnv::new("sec-recover").with_policy_text(&policy(&svc.url(), "approval = false"));
    let script = t.write_script("s.toml", "task = \"s\"\n[[step]]\ntool = \"ext.call\"\nservice = \"tracker\"\npayload = \"order-9\"\n");
    let out = run(&t, &script, true);
    assert!(!out.status.success(), "the answer was lost: the run stops with the call uncertain");
    let id = t.norn(&["status"]).lines().next().unwrap().split_whitespace().next().unwrap().to_string();

    // Without the secret, recovery cannot send the same request, and says so.
    let no = t.cmd(&["recover", "--quiet", &id]).env_remove(VAR).output().unwrap();
    assert!(!no.status.success());
    assert!(String::from_utf8_lossy(&no.stderr).contains(VAR), "{}", String::from_utf8_lossy(&no.stderr));

    let yes = t.cmd(&["recover", "--quiet", &id]).env(VAR, TOKEN).output().unwrap();
    assert!(yes.status.success(), "{}", String::from_utf8_lossy(&yes.stderr));
    let seen = svc.seen();
    assert_eq!(seen.iter().map(|s| s.outcome).collect::<Vec<_>>(), ["applied-then-dropped", "replayed"]);
    assert!(seen.iter().all(|s| s.authorization.as_deref() == Some(format!("Bearer {TOKEN}").as_str())));
    assert_eq!(svc.applied().len(), 1, "applied once");
    assert!(files_containing(&t.exec_dir(&id), TOKEN.as_bytes()).is_empty());
}

#[test]
fn policies_that_would_leak_a_secret_are_refused() {
    let t = TestEnv::new("sec-policy");
    let http = policy("http://127.0.0.1:9/apply", "approval = false").replace(&format!("ca_file = \"{}\"\n", ca().display()), "");
    let t2 = TestEnv::new("sec-policy2").with_policy_text(&http);
    let out = t2.output(&["check", "--policy", t2.policy.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("only over https://"), "{}", String::from_utf8_lossy(&out.stdout));
    let unknown = policy("https://127.0.0.1:9/apply", "").replace("{secret:TRACKER_TOKEN}", "{secret:OTHER}");
    let t3 = TestEnv::new("sec-policy3").with_policy_text(&unknown);
    let out = t3.output(&["check", "--policy", t3.policy.to_str().unwrap()]);
    assert!(String::from_utf8_lossy(&out.stdout).contains("does not define"), "{}", String::from_utf8_lossy(&out.stdout));
    drop(t);
}
