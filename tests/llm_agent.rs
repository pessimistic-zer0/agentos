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
            let reply = json!({"choices": [{"index": 0, "message": reply_for(req["messages"].as_array().map(|v| v.as_slice()).unwrap_or(&[]))}]});
            seen2.lock().unwrap().push(req);
            let text = reply.to_string();
            let _ = write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", text.len(), text);
        }
    });
    (url, seen)
}

fn run_llm(t: &TestEnv, url: &str, extra_env: &[(&str, &str)]) -> std::process::Output {
    let mut c = t.cmd(&[
        "run", "--quiet", "--repo", "examples/demo-repo", "--policy", t.policy.to_str().unwrap(),
        "--agent", "llm", "--task", "Make the failing test in tests/test_stats.py pass",
    ]);
    c.env("AGENTOS_LLM_BASE_URL", url).env("AGENTOS_LLM_MODEL", "mock").env("AGENTOS_LLM_API_KEY", "KEY-CANARY-123");
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
    let out = run_llm(&t, &url, &[("AGENTOS_CRASH_AT", "P4"), ("AGENTOS_CRASH_CALL", "4")]);
    use std::os::unix::process::ExitStatusExt;
    assert_eq!(out.status.signal(), Some(9));
    let id = std::fs::read_dir(t.state.join("executions")).unwrap().next().unwrap().unwrap().file_name().to_string_lossy().into_owned();
    let before = seen.lock().unwrap().len();

    let out = t.cmd(&["run", "--quiet", "--resume", &id]).env("AGENTOS_LLM_API_KEY", "KEY-CANARY-123").output().unwrap();
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
    for needle in ["no LLM server answers", "AGENTOS_LLM_BASE_URL", "AGENTOS_LLM_MODEL", "AGENTOS_LLM_API_KEY", "llama-server", "Nothing was started"] {
        assert!(err.contains(needle), "`{needle}` is missing from:\n{err}");
    }
    assert!(!err.contains("KEY-CANARY-123"), "the API key was printed");
    assert!(!t.state.join("executions").exists(), "an execution was created although no server was reachable");

    let bad = run_llm(&t, "not a url", &[]);
    let err = String::from_utf8_lossy(&bad.stderr);
    assert!(!bad.status.success() && err.contains("AGENTOS_LLM_BASE_URL"), "{err}");
}

// ---------------------------------------------------------------------------
// D-107: a model that stops while its last command is still failing.
// ---------------------------------------------------------------------------

fn was_told_to_go_on(messages: &[Value]) -> bool {
    messages.iter().any(|m| m["role"] == "user" && m["content"].as_str().is_some_and(|c| c.starts_with("[agentos] You answered without a tool call")))
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
    assert!(t.agentos(&["status", &id]).contains("stopped"));
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
