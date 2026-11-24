//! The v1 "Bonus": one fake external service, called through the gateway
//! with an idempotency key.
//!
//! Part 1 (here, no sandbox needed): the fake service keeps its promise, and
//! the client in `src/tools/ext.rs` reads each kind of answer correctly.
//! Part 2 (further down): `ext.call` through the real `norn` binary.
//! The kill and timeout matrix is in `tests/recovery/main.rs`.

mod common;

use std::time::{Duration, Instant};

use norn::tools::ext::{self, Answer};
use common::fake_service::{FakeService, Fault};

const TIMEOUT: Duration = Duration::from_millis(300);
const HOLD: Duration = Duration::from_millis(500);

fn body_json(a: &Answer) -> serde_json::Value {
    match a {
        Answer::Applied { body, .. } => serde_json::from_str(body).unwrap(),
        other => panic!("expected an applied answer, got {other:?}"),
    }
}

#[test]
fn the_service_applies_each_key_once_and_replays_its_answer() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let first = ext::send(&svc.url(), "k1", "order-1", TIMEOUT, 4096);
    assert_eq!(body_json(&first), serde_json::json!({"seq": 1, "key": "k1", "replayed": false}));
    let again = ext::send(&svc.url(), "k1", "order-1", TIMEOUT, 4096);
    assert_eq!(body_json(&again), serde_json::json!({"seq": 1, "key": "k1", "replayed": true}));
    let other = ext::send(&svc.url(), "k2", "order-1", TIMEOUT, 4096);
    assert_eq!(body_json(&other)["seq"], 2, "a new key is a new effect, even with the same payload");
    assert_eq!(svc.applied().len(), 2);
    assert_eq!(svc.seen().iter().map(|s| s.outcome).collect::<Vec<_>>(), vec!["applied", "replayed", "applied"]);
}

#[test]
fn refusals_are_definite_and_apply_nothing() {
    let svc = FakeService::start(0, HOLD).unwrap();
    assert!(matches!(ext::send(&svc.url(), "k1", "reject this", TIMEOUT, 4096), Answer::Refused { status: 422, .. }));
    // Same key, different payload: the service will not guess which one was meant.
    assert!(matches!(ext::send(&svc.url(), "k2", "a", TIMEOUT, 4096), Answer::Applied { .. }));
    assert!(matches!(ext::send(&svc.url(), "k2", "b", TIMEOUT, 4096), Answer::Refused { status: 409, .. }));
    // A path the service does not have.
    let wrong = svc.url().replace("/apply", "/nope");
    assert!(matches!(ext::send(&wrong, "k3", "x", TIMEOUT, 4096), Answer::Refused { status: 404, .. }));
    assert_eq!(svc.applied().len(), 1);
}

#[test]
fn a_lost_request_and_a_lost_answer_look_the_same_from_outside() {
    let svc = FakeService::start(0, HOLD).unwrap();
    svc.inject("lost-request", &[Fault::DropBefore]);
    svc.inject("lost-answer", &[Fault::DropAfter]);

    let started = Instant::now();
    let a = ext::send(&svc.url(), "ka", "lost-request", TIMEOUT, 4096);
    assert!(matches!(a, Answer::Unknown { .. }), "{a:?}");
    assert!(started.elapsed() >= TIMEOUT, "the client waited for its timeout");
    let b = ext::send(&svc.url(), "kb", "lost-answer", TIMEOUT, 4096);
    assert!(matches!(b, Answer::Unknown { .. }), "{b:?}");

    // Same view from the client; opposite truth at the service.
    let applied: Vec<String> = svc.applied().into_iter().map(|a| a.payload).collect();
    assert_eq!(applied, vec!["lost-answer".to_string()]);

    // Sending again with the same key settles both, and nothing is doubled.
    assert_eq!(body_json(&ext::send(&svc.url(), "ka", "lost-request", TIMEOUT, 4096))["replayed"], false);
    assert_eq!(body_json(&ext::send(&svc.url(), "kb", "lost-answer", TIMEOUT, 4096))["replayed"], true);
    assert_eq!(svc.applied().len(), 2);
}

#[test]
fn send_until_answered_retries_with_the_same_key_and_gives_up_as_unknown() {
    let svc = FakeService::start(0, HOLD).unwrap();
    svc.inject("flaky", &[Fault::DropAfter, Fault::DropBefore]);
    let (answer, sent) = ext::send_until_answered(&svc.url(), "k1", "flaky", TIMEOUT, 3, 4096);
    assert_eq!(sent, 3);
    assert_eq!(body_json(&answer)["replayed"], true, "attempt 1 applied it; attempt 3 got the stored answer");
    assert_eq!(svc.applied().len(), 1);
    assert!(svc.seen().iter().all(|s| s.key == "k1"));

    svc.inject("down", &[Fault::DropBefore; 3]);
    let (answer, sent) = ext::send_until_answered(&svc.url(), "k2", "down", TIMEOUT, 3, 4096);
    assert_eq!(sent, 3);
    assert!(matches!(answer, Answer::Unknown { .. }));
    assert_eq!(svc.applied().len(), 1, "nothing new was applied");
}

#[test]
fn nobody_listening_is_unknown_not_failed() {
    // Bind a port, then close it again, so nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let a = ext::send(&format!("http://127.0.0.1:{port}/apply"), "k", "x", TIMEOUT, 4096);
    assert!(matches!(a, Answer::Unknown { .. }), "{a:?}");
}

#[test]
fn a_redirect_is_not_followed() {
    use std::io::{Read, Write};
    // A one-shot server that answers every request with a redirect to the
    // fake service. If the client followed it, the service would see a request.
    let svc = FakeService::start(0, HOLD).unwrap();
    let target = svc.url();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in listener.incoming().take(1) {
            let mut conn = conn.unwrap();
            let mut buf = [0u8; 2048];
            let _ = conn.read(&mut buf);
            let _ = write!(conn, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    let a = ext::send(&format!("http://127.0.0.1:{port}/apply"), "k", "x", TIMEOUT, 4096);
    assert!(matches!(a, Answer::Unknown { .. }), "{a:?}");
    std::thread::sleep(Duration::from_millis(100));
    assert!(svc.seen().is_empty(), "the redirect was followed");
}

#[test]
fn long_answers_are_cut_at_the_limit() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let a = ext::send(&svc.url(), "a-rather-long-idempotency-key-0123456789", "x", TIMEOUT, 16);
    match a {
        Answer::Applied { body, .. } => assert_eq!(body.len(), 16),
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Part 2: `ext.call` through the real binary. Needs a delegated cgroup, like
// every test that starts `norn run`.
// ---------------------------------------------------------------------------

use std::os::unix::process::ExitStatusExt;

use common::*;
use serde_json::Value;

/// The preset policy, plus an `[external]` section for `svc` (short timeout,
/// so lost requests cost little time), optionally with `python3 -c` allowed.
fn policy_with(svc_url: Option<&str>, allow_python: bool) -> String {
    let base = std::fs::read_to_string(preset_policy()).unwrap();
    let mut out = String::new();
    let mut skip = false;
    for line in base.lines() {
        if allow_python && line.starts_with("allow = [") {
            skip = true;
            out.push_str("allow = [[\"python3\", \"-c\"]]\n");
            continue;
        }
        if skip {
            skip = !line.starts_with(']');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    if let Some(url) = svc_url {
        out.push_str(&format!("\n[external]\ntimeout_ms = 300\nattempts = 3\n\n[[external.service]]\nname = \"ledger\"\nurl = \"{url}\"\n"));
    }
    out
}

fn ext_step(service: &str, payload: &str) -> String {
    format!("[[step]]\ntool = \"ext.call\"\nservice = \"{service}\"\npayload = \"{payload}\"\n")
}

fn script(steps: &[String]) -> String {
    format!("task = \"external call test\"\n{}", steps.concat())
}

fn of_type<'a>(events: &'a [Value], ty: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == ty).collect()
}

fn only_execution(t: &TestEnv) -> String {
    std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned()
}

#[test]
fn shipped_policy_denies_ext_call() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let t = TestEnv::new("ext-shipped");
    let s = t.write_script("s.toml", &script(&[ext_step("ledger", "order-1")]));
    let id = t.run_script(s.to_str().unwrap());
    let events = t.events(&id);
    let denied = of_type(&events, "tool.denied");
    assert_eq!(denied.len(), 1);
    assert_eq!(denied[0]["rule"], "no matching rule");
    assert!(of_type(&events, "tool.intent").is_empty());
    assert!(svc.seen().is_empty(), "a denied call must not reach the service");
}

#[test]
fn a_policy_naming_another_machine_is_refused_at_startup() {
    let t = TestEnv::new("ext-nonlocal").with_policy_text(&policy_with(Some("http://example.com:80/apply"), false));
    let s = t.write_script("s.toml", &script(&[ext_step("ledger", "order-1")]));
    let out = t.output(&["run", "--quiet", "--repo", "examples/demo-repo", "--policy", t.policy.to_str().unwrap(), "--script", s.to_str().unwrap()]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("must be http://127.0.0.1:<port>"), "{}", String::from_utf8_lossy(&out.stderr));
    assert!(!t.state.join("executions").exists(), "nothing was created");
}

#[test]
fn allowed_call_is_logged_keyed_and_applied_once() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let t = TestEnv::new("ext-ok").with_policy_text(&policy_with(Some(&svc.url()), false));
    let s = t.write_script(
        "s.toml",
        &script(&[ext_step("ledger", "order-1"), ext_step("nobody", "x"), ext_step("ledger", "reject me"), ext_step("ledger", "order-2")]),
    );
    let id = t.run_script(s.to_str().unwrap());
    let events = t.events(&id);

    // Call 1: allowed, intent carries the key, completed without a checkpoint.
    let intent = &of_type(&events, "tool.intent")[0];
    assert_eq!(intent["rule"], r#"external.service[0] "ledger""#);
    let key = format!("agentos-{id}-c1");
    assert_eq!(intent["plan"]["key"], key.as_str());
    assert_eq!(intent["plan"]["url"], svc.url().as_str());
    let done = of_type(&events, "tool.completed");
    assert_eq!(done.len(), 2);
    assert!(done[0]["checkpoint"].is_null(), "ext.call does not change the workspace");
    assert_eq!(done[0]["summary"], "HTTP 200 after 1 request");
    let out = t.output_of(&id, &events, 1).unwrap();
    assert_eq!(out["kind"], "external");
    assert_eq!(serde_json::from_str::<Value>(out["body"].as_str().unwrap()).unwrap()["key"], key.as_str());
    // The payload is in the log as a hash only.
    let requested = &of_type(&events, "tool.requested")[0]["call"];
    assert_eq!(requested["tool"], "ext.call");
    assert!(requested["payload_hash"].as_str().unwrap().starts_with("sha256:"));
    assert!(requested.get("payload").is_none());

    // Call 2: a service the policy does not list.
    assert_eq!(of_type(&events, "tool.denied")[0]["rule"], "no matching rule");
    // Call 3: the service refused; recorded as failed, nothing applied.
    let failed = of_type(&events, "tool.failed");
    assert_eq!(failed.len(), 1);
    assert!(failed[0]["error"].as_str().unwrap().contains("refused the request (HTTP 422)"));

    // Ground truth at the service: exactly the two orders, under the keys of calls 1 and 4.
    let applied: Vec<(String, String)> = svc.applied().into_iter().map(|a| (a.key, a.payload)).collect();
    assert_eq!(applied, vec![(key, "order-1".to_string()), (format!("agentos-{id}-c4"), "order-2".to_string())]);
    assert!(t.norn(&["trace", &id]).contains("ext.call ledger (7 bytes)"));
}

#[test]
fn a_lost_answer_is_retried_with_the_same_key_and_applied_once() {
    let svc = FakeService::start(0, HOLD).unwrap();
    svc.inject("order-1", &[Fault::DropAfter]);
    let t = TestEnv::new("ext-lost-answer").with_policy_text(&policy_with(Some(&svc.url()), false));
    let s = t.write_script("s.toml", &script(&[ext_step("ledger", "order-1")]));
    let id = t.run_script(s.to_str().unwrap());
    let events = t.events(&id);
    assert_eq!(of_type(&events, "tool.completed")[0]["summary"], "HTTP 200 after 2 requests");
    assert_eq!(svc.applied().len(), 1);
    let seen = svc.seen();
    assert_eq!(seen.iter().map(|s| s.outcome).collect::<Vec<_>>(), vec!["applied-then-dropped", "replayed"]);
    assert_eq!(seen[0].key, seen[1].key);
}

#[test]
fn an_outage_leaves_the_call_uncertain_until_recovery_gets_an_answer() {
    let svc = FakeService::start(0, HOLD).unwrap();
    // Request 1 is applied but its answer is lost; the next five are lost
    // outright: the run's other two attempts and one whole recovery.
    svc.inject("order-1", &[Fault::DropAfter, Fault::DropBefore, Fault::DropBefore, Fault::DropBefore, Fault::DropBefore, Fault::DropBefore]);
    let t = TestEnv::new("ext-outage").with_policy_text(&policy_with(Some(&svc.url()), false));
    let s = t.write_script("s.toml", &script(&[ext_step("ledger", "order-1"), ext_step("ledger", "order-2")]));
    let out = t.output(&["run", "--quiet", "--repo", "examples/demo-repo", "--policy", t.policy.to_str().unwrap(), "--script", s.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1), "the run stops with an error; it is not killed");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("is UNCERTAIN: no answer after 3 request(s)"), "{err}");
    let id = only_execution(&t);
    assert!(t.norn(&["status", &id]).contains("interrupted"));
    let events = t.events(&id);
    assert_eq!(of_type(&events, "tool.intent").len(), 1);
    assert!(of_type(&events, "tool.completed").is_empty() && of_type(&events, "tool.failed").is_empty(), "neither would be true");
    assert!(t.norn(&["trace", &id]).contains("UNCERTAIN"));

    // The service is still down: recovery must not guess, and resume must not go on.
    for cmd in [["recover", "--quiet", id.as_str()].as_slice(), ["run", "--quiet", "--resume", id.as_str()].as_slice()] {
        if svc.faults_left() < 3 {
            svc.inject("order-1", &[Fault::DropBefore; 3]);
        }
        let out = t.output(cmd);
        assert!(!out.status.success());
        assert!(String::from_utf8_lossy(&out.stderr).contains("still UNCERTAIN"), "{}", String::from_utf8_lossy(&out.stderr));
        let events = t.events(&id);
        assert!(of_type(&events, "tool.reconciled").is_empty(), "no verdict without an answer");
        assert_eq!(of_type(&events, "tool.requested").len(), 1, "the agent did not continue");
    }
    assert_eq!(svc.faults_left(), 0);

    // The service is back.
    let text = t.norn(&["recover", &id]);
    assert!(text.contains("#1   ext.call APPLIED"), "{text}");
    let events = t.events(&id);
    let verdict = of_type(&events, "tool.reconciled")[0];
    assert_eq!(verdict["applied"], true);
    assert!(verdict["reason"].as_str().unwrap().contains("\"replayed\":true"), "the first request had been applied: {}", verdict["reason"]);
    t.norn(&["run", "--quiet", "--resume", &id]);
    let events = t.events(&id);
    assert!(of_type(&events, "execution.finished").iter().any(|e| e["status"] == "completed"));
    let applied: Vec<String> = svc.applied().into_iter().map(|a| a.payload).collect();
    assert_eq!(applied, vec!["order-1".to_string(), "order-2".to_string()], "each order exactly once");
    assert!(svc.seen().iter().filter(|s| s.payload == "order-1").all(|s| s.key == format!("agentos-{id}-c1")));
}

#[test]
fn killed_after_the_intent_recovery_sends_the_request() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let t = TestEnv::new("ext-p2").with_policy_text(&policy_with(Some(&svc.url()), false));
    let s = t.write_script("s.toml", &script(&[ext_step("ledger", "order-1")]));
    let out = t
        .cmd(&["run", "--quiet", "--repo", "examples/demo-repo", "--policy", t.policy.to_str().unwrap(), "--script", s.to_str().unwrap()])
        .env("NORN_CRASH_AT", "P2")
        .output()
        .unwrap();
    assert_eq!(out.status.signal(), Some(9));
    assert!(svc.seen().is_empty(), "killed before the request was sent");
    let id = only_execution(&t);
    t.norn(&["run", "--quiet", "--resume", &id]);
    let events = t.events(&id);
    assert_eq!(of_type(&events, "tool.reconciled")[0]["applied"], true);
    assert_eq!(of_type(&events, "tool.requested").len(), 1, "an applied call is not asked for again");
    assert_eq!(svc.applied().len(), 1);
    assert_eq!(svc.applied()[0].key, format!("agentos-{id}-c1"));
}

#[test]
fn sandboxed_code_cannot_reach_the_service() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let t = TestEnv::new("ext-sandbox").with_policy_text(&policy_with(Some(&svc.url()), true));
    let py = format!(
        "import socket\ns = socket.socket()\ns.settimeout(2)\ntry:\n    s.connect(('127.0.0.1', {port}))\n    s.sendall(b'POST /apply HTTP/1.1\\r\\nIdempotency-Key: stolen\\r\\nContent-Length: 1\\r\\n\\r\\nx')\n    print('ESCAPED: reached the service')\nexcept OSError as e:\n    print('BLOCKED:', e)\n",
        port = svc.port()
    );
    let s = t.write_script("s.toml", &format!("task = \"t\"\n[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", '''\n{py}''']\n"));
    let id = t.run_script(s.to_str().unwrap());
    let events = t.events(&id);
    let out = t.output_of(&id, &events, 1).unwrap();
    let stdout = out["stdout"].as_str().unwrap();
    assert!(stdout.contains("BLOCKED:") && !stdout.contains("ESCAPED"), "{stdout}");
    std::thread::sleep(Duration::from_millis(100));
    assert!(svc.seen().is_empty(), "the sandbox has its own, empty network; only the gateway talks to the service");
}
