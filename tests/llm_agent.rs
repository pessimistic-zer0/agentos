//! The LLM agent loop against a mock OpenAI-compatible endpoint.
//!
//! The mock is a tiny HTTP server in this test process. It plays a model
//! that fixes the demo repo: it looks at how many tool results the
//! conversation already holds and answers with the next step. Because its
//! answer depends only on the request, it also works after a crash, when the
//! conversation is rebuilt from the event log.
//!
//! This tests the loop, the logging and the resume, not a model's skill. The
//! real-model run is recorded in docs/milestones/M4-llm-agent.md.

mod common;

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use common::*;
use serde_json::{json, Value};

const FIXED: &str = "\"\"\"Small statistics helpers.\"\"\"\n\n\ndef mean(values):\n    \"\"\"Return the arithmetic mean of a non-empty list of numbers.\"\"\"\n    if not values:\n        raise ValueError(\"mean() of an empty list\")\n    return sum(values) / len(values)\n\n\ndef median(values):\n    \"\"\"Return the median of a non-empty list of numbers.\"\"\"\n    if not values:\n        raise ValueError(\"median() of an empty list\")\n    ordered = sorted(values)\n    mid = len(ordered) // 2\n    if len(ordered) % 2 == 1:\n        return ordered[mid]\n    return (ordered[mid - 1] + ordered[mid]) / 2\n";

/// The mock model's plan: step n is chosen by the number of tool results so far.
fn reply_for(messages: &[Value]) -> Value {
    let results: Vec<&Value> = messages.iter().filter(|m| m["role"] == "tool").collect();
    let call = |name: &str, args: Value| {
        json!({"role": "assistant", "content": "", "tool_calls": [{
            "id": format!("call_{}", results.len() + 1), "type": "function",
            "function": {"name": name, "arguments": args.to_string()}}]})
    };
    // A NOT APPLIED result (after a crash) does not count: that step is done again.
    let done = results.iter().filter(|m| !m["content"].as_str().unwrap_or("").starts_with("NOT APPLIED")).count();
    match done {
        0 => call("fs_list", json!({"path": "."})),
        1 => call("fs_read", json!({"path": "src/stats.py"})),
        2 => call("fs_read", json!({"path": ".env"})), // must be denied
        3 => call("fs_write", json!({"path": "src/stats.py", "content": FIXED})),
        4 => call("exec", json!({"argv": ["pytest", "-q"]})),
        _ => json!({"role": "assistant", "content": "Fixed mean(): it divided by len(values) - 1. The tests pass."}),
    }
}

/// Start the mock; returns its base URL and the requests it received.
fn start_mock() -> (String, Arc<Mutex<Vec<Value>>>) {
    start_mock_with(reply_for)
}

/// A mock endpoint whose answers come from `reply_for`.
fn start_mock_with(reply_for: fn(&[Value]) -> Value) -> (String, Arc<Mutex<Vec<Value>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}/v1", listener.local_addr().unwrap());
    let seen = Arc::new(Mutex::new(Vec::new()));
    let seen2 = seen.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { continue };
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut len = 0usize;
            let mut auth = String::new();
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                    break;
                }
                let lower = line.to_ascii_lowercase();
                if let Some(v) = lower.strip_prefix("content-length:") {
                    len = v.trim().parse().unwrap_or(0);
                }
                if lower.starts_with("authorization:") {
                    auth = line.trim().to_string();
                }
            }
            // A connection that sends nothing is the runtime checking that a
            // server is there (`LlmConfig::check_reachable`); it is not a request.
            if len == 0 {
                continue;
            }
            let mut body = vec![0u8; len];
            if reader.read_exact(&mut body).is_err() {
                continue;
            }
            let mut req: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
            req["_authorization"] = json!(auth);
            let message = reply_for(req["messages"].as_array().map(|v| v.as_slice()).unwrap_or(&[]));
            req["_nth"] = json!(seen2.lock().unwrap().len());
            seen2.lock().unwrap().push(req);
            // A reply of the form {"_status": 500, "_body": "..."} makes the
            // mock answer with that HTTP status instead of a message.
            let (status, text) = match message["_status"].as_u64() {
                Some(code) => (code, message["_body"].as_str().unwrap_or("").to_string()),
                None => (200, json!({"choices": [{"index": 0, "message": message}]}).to_string()),
            };
            // "_retry_after": seconds, sent as the Retry-After header.
            let extra = message["_retry_after"].as_str().map(|s| format!("Retry-After: {s}\r\n")).unwrap_or_default();
            let _ = write!(stream, "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\n{extra}Content-Length: {}\r\nConnection: close\r\n\r\n{}", text.len(), text);
        }
    });
    (url, seen)
}

fn run_llm(t: &TestEnv, url: &str, extra_env: &[(&str, &str)]) -> std::process::Output {
    run_llm_on(t, url, "examples/demo-repo", extra_env)
}

fn run_llm_on(t: &TestEnv, url: &str, repo: &str, extra_env: &[(&str, &str)]) -> std::process::Output {
    let mut c = t.cmd(&[
        "run", "--quiet", "--repo", repo, "--policy", t.policy.to_str().unwrap(),
        "--agent", "llm", "--task", "Make the failing test in tests/test_stats.py pass",
    ]);
    c.env("NORN_LLM_BASE_URL", url).env("NORN_LLM_MODEL", "mock").env("NORN_LLM_API_KEY", "KEY-CANARY-123");
    for (k, v) in extra_env {
        c.env(k, v);
    }
    c.output().unwrap()
}

#[test]
fn llm_loop_fixes_the_demo_and_logs_the_conversation() {
    let t = TestEnv::new("llm-loop");
    let (url, seen) = start_mock();
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);

    assert!(events.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"));
    assert_eq!(exec_exit_codes(&t, &events), vec![Some(0)], "pytest passes after the model's fix");
    // The model's attempt to read .env went through the gateway and was denied.
    assert!(events.iter().any(|e| e["type"] == "tool.denied" && e["rule"].as_str().unwrap().starts_with("filesystem.deny")));

    // ADR 0005: every message is an event: system, user, then per step an
    // assistant message and a tool result, then the final answer.
    let msgs: Vec<&Value> = events.iter().filter(|e| e["type"] == "agent.message").map(|e| &e["message"]).collect();
    let roles: Vec<&str> = msgs.iter().map(|m| m["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["system", "user", "assistant", "tool", "assistant", "tool", "assistant", "tool", "assistant", "tool", "assistant", "tool", "assistant"]);
    // Each assistant message is logged before the call it asks for.
    let seq_of = |pred: &dyn Fn(&Value) -> bool| events.iter().find(|e| pred(e)).unwrap()["seq"].as_u64().unwrap();
    let first_call_msg = seq_of(&|e| e["type"] == "agent.message" && e["message"]["tool_calls"].is_array());
    let first_request = seq_of(&|e| e["type"] == "tool.requested");
    assert!(first_call_msg < first_request);

    // The API key reached the endpoint but is nowhere on disk.
    assert!(seen.lock().unwrap()[0]["_authorization"].as_str().unwrap().contains("KEY-CANARY-123"));
    let grep = std::process::Command::new("grep").args(["-r", "-l", "-a", "KEY-CANARY-123"]).arg(t.exec_dir(&id)).output().unwrap();
    assert!(grep.stdout.is_empty(), "API key found in {}", String::from_utf8_lossy(&grep.stdout));
}

#[test]
fn llm_run_resumes_after_a_crash_with_the_conversation_from_the_log() {
    let t = TestEnv::new("llm-resume");
    let (url, seen) = start_mock();
    // Kill the runtime after the write (call 4) took effect, before its checkpoint.
    let out = run_llm(&t, &url, &[("NORN_CRASH_AT", "P4"), ("NORN_CRASH_CALL", "4")]);
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(out.status.signal(), Some(9));
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let before = seen.lock().unwrap().len();

    let out = t.cmd(&["run", "--quiet", "--resume", &id]).env("NORN_LLM_API_KEY", "KEY-CANARY-123").output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let events = t.events(&id);
    assert!(events.iter().any(|e| e["type"] == "tool.reconciled" && e["call_id"] == 4 && e["applied"] == true));
    assert!(events.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"));
    assert_eq!(exec_exit_codes(&t, &events), vec![Some(0)]);
    // Exactly one fs.write was made: the applied write was not repeated.
    assert_eq!(events.iter().filter(|e| e["type"] == "tool.requested" && e["call"]["tool"] == "fs.write").count(), 1);

    // The first request after the restart carried the whole earlier
    // conversation, rebuilt from the log, ending with the cut-off call's result.
    let reqs = seen.lock().unwrap();
    let msgs = reqs[before]["messages"].as_array().unwrap();
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs.iter().filter(|m| m["role"] == "tool").count(), 4);
    let last = msgs.last().unwrap();
    assert_eq!(last["role"], "tool");
    assert_eq!(last["tool_call_id"], "call_4");
}

#[test]
fn no_server_is_reported_before_anything_is_started() {
    let t = TestEnv::new("llm-no-server");
    // A port nobody listens on: take one from the kernel, then give it back.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let out = run_llm(&t, &format!("http://127.0.0.1:{port}/v1"), &[]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    for needle in ["no LLM server answers", "NORN_LLM_BASE_URL", "NORN_LLM_MODEL", "NORN_LLM_API_KEY", "llama-server", "Nothing was started"] {
        assert!(err.contains(needle), "`{needle}` is missing from:\n{err}");
    }
    assert!(!err.contains("KEY-CANARY-123"), "the API key was printed");
    assert!(!t.state.join("executions").exists(), "an execution was created although no server was reachable");

    let bad = run_llm(&t, "not a url", &[]);
    let err = String::from_utf8_lossy(&bad.stderr);
    assert!(!bad.status.success() && err.contains("NORN_LLM_BASE_URL"), "{err}");
}

// ---------------------------------------------------------------------------
// D-107: a model that stops while its last command is still failing.
// ---------------------------------------------------------------------------

fn was_told_to_go_on(messages: &[Value]) -> bool {
    messages.iter().any(|m| m["role"] == "user" && m["content"].as_str().is_some_and(|c| c.starts_with("[norn] You answered without a tool call")))
}

fn tool_call(n: usize, name: &str, args: Value) -> Value {
    json!({"role": "assistant", "content": "", "tool_calls": [{"id": format!("call_{n}"), "type": "function", "function": {"name": name, "arguments": args.to_string()}}]})
}

/// Runs the failing tests, then only talks.
fn gives_up(messages: &[Value]) -> Value {
    let results = messages.iter().filter(|m| m["role"] == "tool").count();
    match (results, was_told_to_go_on(messages)) {
        (0, _) => tool_call(1, "exec", json!({"argv": ["pytest", "-q"]})),
        (_, false) => json!({"role": "assistant", "content": "The test fails because mean() is wrong. I would need to change src/stats.py."}),
        (_, true) => json!({"role": "assistant", "content": "GIVE UP: I cannot work out the fix."}),
    }
}

/// Runs the failing tests, talks, and after being told to go on, fixes them.
fn goes_on_when_told(messages: &[Value]) -> Value {
    let results = messages.iter().filter(|m| m["role"] == "tool").count();
    match (results, was_told_to_go_on(messages)) {
        (0, _) => tool_call(1, "exec", json!({"argv": ["pytest", "-q"]})),
        (1, false) => json!({"role": "assistant", "content": "The test fails. Done looking."}),
        (1, true) => tool_call(2, "fs_write", json!({"path": "src/stats.py", "content": FIXED})),
        (2, _) => tool_call(3, "exec", json!({"argv": ["pytest", "-q"]})),
        _ => json!({"role": "assistant", "content": "Fixed; the tests pass."}),
    }
}

#[test]
fn a_model_that_stops_while_the_tests_fail_is_told_once_then_recorded_as_stopped() {
    let t = TestEnv::new("llm-gives-up");
    let (url, seen) = start_mock_with(gives_up);
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let end = events.iter().find(|e| e["type"] == "execution.finished").unwrap();
    assert_eq!(end["status"], "stopped", "not `completed`: {end}");
    let summary = end["summary"].as_str().unwrap();
    assert!(summary.contains("still failing") && summary.contains("GIVE UP: I cannot work out the fix."), "{summary}");
    assert_eq!(seen.lock().unwrap().len(), 3, "one call, one early stop, one answer to the reminder");
    let roles: Vec<&str> = events.iter().filter(|e| e["type"] == "agent.message").map(|e| e["message"]["role"].as_str().unwrap()).collect();
    assert_eq!(roles, ["system", "user", "assistant", "tool", "assistant", "user", "assistant"], "the reminder is in the log");
    assert!(t.norn(&["status", &id]).contains("stopped"));
}

#[test]
fn a_model_that_goes_on_when_told_finishes_as_completed() {
    let t = TestEnv::new("llm-goes-on");
    let (url, seen) = start_mock_with(goes_on_when_told);
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    assert!(events.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"), "{events:#?}");
    assert_eq!(exec_exit_codes(&t, &events), vec![Some(1), Some(0)]);
    assert_eq!(seen.lock().unwrap().len(), 5);
}

// ---------------------------------------------------------------------------
// D-111: what the model's server answers when it does not answer 200.
// ---------------------------------------------------------------------------

const CUT_OFF: &str = r#"{"error":{"code":500,"message":"Failed to parse tool call arguments as JSON: missing closing quote"}}"#;

fn was_told_reply_unusable(messages: &[Value]) -> usize {
    messages.iter().filter(|m| m["role"] == "user" && m["content"].as_str().is_some_and(|c| c.starts_with("[norn] Your last reply could not be used"))).count()
}

/// The first answer after reading the file is "cut off" (HTTP 500); told
/// so, the model does the job.
fn cut_off_once(messages: &[Value]) -> Value {
    let results = messages.iter().filter(|m| m["role"] == "tool").count();
    match (results, was_told_reply_unusable(messages)) {
        (0, _) => tool_call(1, "fs_read", json!({"path": "src/stats.py"})),
        (1, 0) => json!({"_status": 500, "_body": CUT_OFF}),
        (1, _) => tool_call(2, "fs_write", json!({"path": "src/stats.py", "content": FIXED})),
        (2, _) => tool_call(3, "exec", json!({"argv": ["pytest", "-q"]})),
        _ => json!({"role": "assistant", "content": "Fixed; the tests pass."}),
    }
}

/// Every answer after the first write is cut off.
fn cut_off_for_good(messages: &[Value]) -> Value {
    match messages.iter().filter(|m| m["role"] == "tool").count() {
        0 => tool_call(1, "fs_write", json!({"path": "src/extra.py", "content": "x = 1\n"})),
        _ => json!({"_status": 500, "_body": CUT_OFF}),
    }
}

/// Busy twice (503), then fine.
fn busy_twice(messages: &[Value]) -> Value {
    // The mock counts requests in `_nth` only for the test; here the count
    // of earlier refusals is kept in a static.
    use std::sync::atomic::{AtomicUsize, Ordering};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    if CALLS.fetch_add(1, Ordering::SeqCst) < 2 {
        return json!({"_status": 503, "_body": "loading model"});
    }
    reply_for(messages)
}

fn wrong_key(_: &[Value]) -> Value {
    json!({"_status": 401, "_body": "invalid api key"})
}

#[test]
fn a_reply_the_server_could_not_use_is_told_to_the_model_and_the_run_goes_on() {
    let t = TestEnv::new("llm-cut-off-once");
    let (url, seen) = start_mock_with(cut_off_once);
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    assert!(events.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"), "{events:#?}");
    assert_eq!(exec_exit_codes(&t, &events), vec![Some(0)]);
    let told: Vec<&Value> = events.iter().filter(|e| e["type"] == "agent.message" && e["message"]["role"] == "user").map(|e| &e["message"]["content"]).collect();
    assert!(told.iter().any(|c| c.as_str().unwrap().contains("Failed to parse tool call arguments")), "the server's words are in the log: {told:?}");
    assert_eq!(seen.lock().unwrap().len(), 5);
}

#[test]
fn replies_that_stay_unusable_end_the_run_as_stopped_with_its_patch() {
    let t = TestEnv::new("llm-cut-off-for-good");
    let (url, seen) = start_mock_with(cut_off_for_good);
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "not an error: the run ends in an orderly way\n{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let end = events.iter().find(|e| e["type"] == "execution.finished").expect("the execution is finished, not interrupted");
    assert_eq!(end["status"], "stopped");
    assert!(end["summary"].as_str().unwrap().contains("could not be used three times") && end["summary"].as_str().unwrap().contains("HTTP 500"), "{end}");
    assert_eq!(seen.lock().unwrap().len(), 4, "the write, then three tries");
    // The work done before is in the patch.
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    assert!(patch.contains("b/src/extra.py"), "{patch}");
    assert!(t.norn(&["status", &id]).contains("stopped"));
}

#[test]
fn a_busy_server_is_asked_again() {
    let t = TestEnv::new("llm-busy");
    let (url, seen) = start_mock_with(busy_twice);
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert!(t.events(&id).iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"));
    assert_eq!(seen.lock().unwrap().len(), 8, "two refusals and the six requests of the usual run");
    // The refusals left no trace in the conversation: the model was never told.
    let roles: Vec<String> = t.events(&id).iter().filter(|e| e["type"] == "agent.message").map(|e| e["message"]["role"].as_str().unwrap().to_string()).collect();
    assert_eq!(roles.iter().filter(|r| *r == "user").count(), 1);
}

#[test]
fn a_wrong_key_is_an_error_that_names_the_settings() {
    let t = TestEnv::new("llm-wrong-key");
    let (url, seen) = start_mock_with(wrong_key);
    let out = run_llm(&t, &url, &[]);
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    for needle in ["HTTP 401", "NORN_LLM_API_KEY", "NORN_LLM_MODEL", "NORN_LLM_BASE_URL"] {
        assert!(err.contains(needle), "`{needle}` is missing from:\n{err}");
    }
    assert!(!err.contains("KEY-CANARY-123"), "the key was printed");
    assert_eq!(seen.lock().unwrap().len(), 1, "no second try with a key that is wrong");
}

// ---------------------------------------------------------------------------
// fs_edit (D-120): the model changes one line without sending the file.
// ---------------------------------------------------------------------------

/// Reads the file, sends an edit whose text is not in the file (a common
/// slip: wrong indentation), is told so, sends the right one, runs the tests.
fn fixes_with_an_edit(messages: &[Value]) -> Value {
    let results: Vec<&Value> = messages.iter().filter(|m| m["role"] == "tool").collect();
    let n = results.len();
    let edit = |old: &str| tool_call(n + 1, "fs_edit", json!({"path": "src/stats.py", "old_str": old, "new_str": "    return sum(values) / len(values)"}));
    match n {
        0 => tool_call(1, "fs_read", json!({"path": "src/stats.py"})),
        1 => edit("return sum(values) / (len(values) - 1)  "),
        2 if results[1]["content"].as_str().unwrap().contains("was not found") => edit("    return sum(values) / (len(values) - 1)"),
        3 => tool_call(4, "exec", json!({"argv": ["pytest", "-q"]})),
        4 => json!({"role": "assistant", "content": "Fixed mean() with a one-line edit. The tests pass."}),
        _ => json!({"role": "assistant", "content": "unexpected"}),
    }
}

#[test]
fn a_model_fixes_the_demo_with_one_edit_and_is_told_when_its_text_is_not_found() {
    let t = TestEnv::new("llm-edit");
    let (url, seen) = start_mock_with(fixes_with_an_edit);
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    assert!(events.iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"), "{events:#?}");
    assert_eq!(exec_exit_codes(&t, &events), vec![Some(0)]);
    // The model is offered the tool and told when to use it.
    let first = &seen.lock().unwrap()[0];
    let names: Vec<&str> = first["tools"].as_array().unwrap().iter().map(|t| t["function"]["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["fs_list", "fs_read", "fs_edit", "fs_search", "fs_delete", "fs_write", "exec"]);
    assert!(first["messages"][0]["content"].as_str().unwrap().contains("use fs_edit"));
    // What it was told after each edit.
    let told: Vec<&str> = events.iter().filter(|e| e["type"] == "agent.message" && e["message"]["role"] == "tool").map(|e| e["message"]["content"].as_str().unwrap()).collect();
    assert!(told[1].starts_with("FAILED: old_str was not found"), "{}", told[1]);
    assert!(told[2].starts_with("OK: replaced the text that began at line 8"), "{}", told[2]);
    // The file differs from the original in that one line only, and the
    // model's requests held 2 short strings, not the file.
    let now = std::fs::read_to_string(t.exec_dir(&id).join("work/src/stats.py")).unwrap();
    assert_eq!(now, FIXED);
}

// ---------------------------------------------------------------------------
// D-124: a rate-limited server says how long to wait; the runtime waits that
// long instead of its own short pauses.
// ---------------------------------------------------------------------------

/// Over the rate limit at the second request: asks for 3 s, and refuses
/// every request that comes back sooner than that, as a real limit would.
fn rate_limited(messages: &[Value]) -> Value {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};
    static CALLS: AtomicUsize = AtomicUsize::new(0);
    static OPEN_AT: Mutex<Option<Instant>> = Mutex::new(None);
    let n = CALLS.fetch_add(1, Ordering::SeqCst);
    let mut open_at = OPEN_AT.lock().unwrap();
    if n == 1 {
        *open_at = Some(Instant::now() + Duration::from_secs(3));
    }
    if let Some(at) = *open_at {
        let left = at.saturating_duration_since(Instant::now());
        if !left.is_zero() {
            return json!({"_status": 429, "_body": "rate limit reached", "_retry_after": format!("{:.1}", left.as_secs_f64())});
        }
    }
    reply_for(messages)
}

fn limit_for_a_day(_: &[Value]) -> Value {
    json!({"_status": 429, "_body": "daily limit reached", "_retry_after": "7200"})
}

#[test]
fn a_rate_limited_server_is_given_the_time_it_asks_for() {
    let t = TestEnv::new("llm-rate");
    let (url, seen) = start_mock_with(rate_limited);
    let started = std::time::Instant::now();
    let out = run_llm(&t, &url, &[]);
    let took = started.elapsed();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    assert!(t.events(&id).iter().any(|e| e["type"] == "execution.finished" && e["status"] == "completed"));
    // One refusal only: the runtime came back when the server said, not
    // after its own 0.5 s pause (which would have been refused again).
    assert_eq!(seen.lock().unwrap().len(), 7, "one refusal and the six requests of the usual run");
    assert!(took >= std::time::Duration::from_secs(3), "{took:?}");
}

#[test]
fn a_wait_longer_than_norn_waits_ends_the_run_at_once_and_resumably() {
    let t = TestEnv::new("llm-day");
    let (url, seen) = start_mock_with(limit_for_a_day);
    let started = std::time::Instant::now();
    let out = run_llm(&t, &url, &[]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("asked for a wait of 7200 s") && err.contains("--resume"), "{err}");
    assert!(started.elapsed() < std::time::Duration::from_secs(5), "no pointless waiting: {:?}", started.elapsed());
    assert_eq!(seen.lock().unwrap().len(), 1, "one request, no retries");
}

// ---------------------------------------------------------------------------
// D-145: the stop rule in a project that was already red.
// ---------------------------------------------------------------------------

/// The demo with one more failing test that belongs to another ticket.
fn red_repo(t: &TestEnv) -> String {
    let repo = t.root.join("red");
    assert!(std::process::Command::new("cp").arg("-a").arg(repo_root().join("examples/demo-repo")).arg(&repo).status().unwrap().success());
    std::fs::write(repo.join("tests/test_other.py"), "def test_other_ticket():\n    assert 1 == 2\n").unwrap();
    repo.to_str().unwrap().to_string()
}

/// Runs the tests, fixes its own ticket, runs them again, and stops while
/// the other ticket's test still fails.
fn fixes_mine_in_a_red_project(messages: &[Value]) -> Value {
    match messages.iter().filter(|m| m["role"] == "tool").count() {
        0 => tool_call(1, "exec", json!({"argv": ["pytest", "-q"]})),
        1 => tool_call(2, "fs_write", json!({"path": "src/stats.py", "content": FIXED})),
        2 => tool_call(3, "exec", json!({"argv": ["pytest", "-q"]})),
        _ => json!({"role": "assistant", "content": "Fixed mean(); test_other_ticket belongs to another ticket."}),
    }
}

/// The same, but its change fixes nothing.
fn changes_nothing_useful(messages: &[Value]) -> Value {
    match (messages.iter().filter(|m| m["role"] == "tool").count(), was_told_to_go_on(messages)) {
        (0, _) => tool_call(1, "exec", json!({"argv": ["pytest", "-q"]})),
        (1, _) => tool_call(2, "fs_write", json!({"path": "src/notes.py", "content": "# looked at it\n"})),
        (2, _) => tool_call(3, "exec", json!({"argv": ["pytest", "-q"]})),
        (_, false) => json!({"role": "assistant", "content": "Done."}),
        (_, true) => json!({"role": "assistant", "content": "GIVE UP: no idea."}),
    }
}

#[test]
fn in_a_project_that_was_already_red_fewer_failures_and_no_new_ones_is_done() {
    let t = TestEnv::new("llm-red-progress");
    let repo = red_repo(&t);
    let (url, seen) = start_mock_with(fixes_mine_in_a_red_project);
    let out = run_llm_on(&t, &url, &repo, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let events = t.events(&id);
    let end = events.iter().find(|e| e["type"] == "execution.finished").unwrap();
    assert_eq!(end["status"], "completed", "{end}");
    let summary = end["summary"].as_str().unwrap();
    assert!(summary.contains("already failed 2 test(s). Now 1 fail. New failures since then: none."), "{summary}");
    let sent = seen.lock().unwrap();
    assert!(!was_told_to_go_on(sent.last().unwrap()["messages"].as_array().unwrap()), "no reminder was needed");
    // The model saw the comparison after its second test run.
    let tool_texts: Vec<String> = sent.last().unwrap()["messages"].as_array().unwrap().iter()
        .filter(|m| m["role"] == "tool").map(|m| m["content"].as_str().unwrap().to_string()).collect();
    assert!(!tool_texts[0].contains("[norn] Before the first file change"), "the first run is the baseline itself");
    assert!(tool_texts[2].contains("Fixed since then: tests/test_stats.py::test_mean"), "{}", tool_texts[2]);
    assert_eq!(exec_exit_codes(&t, &events), vec![Some(1), Some(1)]);
}

#[test]
fn in_a_project_that_was_already_red_no_progress_is_still_not_done() {
    let t = TestEnv::new("llm-red-none");
    let repo = red_repo(&t);
    let (url, _) = start_mock_with(changes_nothing_useful);
    let out = run_llm_on(&t, &url, &repo, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let end = t.events(&id).into_iter().find(|e| e["type"] == "execution.finished").unwrap();
    assert_eq!(end["status"], "stopped", "the same failures as before is not done (D-107 still applies): {end}");
}

// ---------------------------------------------------------------------------
// D-148: a conversation larger than the server accepts ends the run at once.
// ---------------------------------------------------------------------------

fn too_large_after_one_call(messages: &[Value]) -> Value {
    if messages.iter().any(|m| m["role"] == "tool") {
        json!({"_status": 413, "_body": r#"{"error":{"message":"Request too large for model: Limit 8000, Requested 9500"}}"#})
    } else {
        tool_call(1, "fs_list", json!({"path": "."}))
    }
}

#[test]
fn a_conversation_too_large_for_the_server_ends_the_run_at_once_as_stopped() {
    let t = TestEnv::new("llm-413");
    let (url, seen) = start_mock_with(too_large_after_one_call);
    let out = run_llm(&t, &url, &[]);
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let id = String::from_utf8(out.stdout).unwrap().trim().to_string();
    let end = t.events(&id).into_iter().find(|e| e["type"] == "execution.finished").unwrap();
    assert_eq!(end["status"], "stopped", "{end}");
    assert!(end["summary"].as_str().unwrap().contains("larger than the model's server accepts (HTTP 413"), "{end}");
    assert_eq!(seen.lock().unwrap().len(), 2, "no second attempt after the 413: telling the model would only make it larger");
    assert!(t.exec_dir(&id).join("result.patch").exists(), "it ends with its patch");
}

