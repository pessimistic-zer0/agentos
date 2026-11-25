//! `norn mcp`: norn as an MCP server (M15; D-139 to D-142).
//!
//! Each test starts the real binary as a child process and talks to it the
//! way Claude Code does (`experiments/m15-mcp/NOTES.md`): 2026-07-28 requests
//! with `_meta`, a stop by SIGINT, a restart with the same command line.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test mcp

mod common;

use common::mcp_client::{meta, McpClient};
use common::*;
use serde_json::{json, Value};

fn demo() -> std::path::PathBuf {
    repo_root().join("examples/demo-repo")
}

/// The fixed `src/stats.py`, as the demo script writes it.
fn fixed_stats() -> String {
    let script: toml::Value = toml::from_str(&std::fs::read_to_string(repo_root().join("examples/scripts/fix-demo.toml")).unwrap()).unwrap();
    let step = script["step"].as_array().unwrap().iter().find(|s| s["tool"].as_str() == Some("fs.write")).unwrap();
    step["content"].as_str().unwrap().to_string()
}

/// The executions in the state folder, as `norn status` lists them.
fn status_lines(t: &TestEnv) -> Vec<String> {
    let out = t.norn(&["status"]);
    out.lines().filter(|l| !l.starts_with("no executions")).map(str::to_string).collect()
}

fn only_execution(t: &TestEnv) -> String {
    let lines = status_lines(t);
    assert_eq!(lines.len(), 1, "expected one execution: {lines:?}");
    lines[0].split_whitespace().next().unwrap().to_string()
}

fn mcp_sessions(events: &[Value]) -> Vec<&Value> {
    events.iter().filter(|e| e["type"] == "agent.message" && e["message"]["mcp_session"].is_object()).collect()
}

#[test]
fn a_client_fixes_the_demo_and_the_person_finishes_and_applies_it() {
    let t = TestEnv::new("mcp-demo");
    let mut c = McpClient::start(&t, &demo());

    let d = c.request("server/discover", json!({}));
    let r = &d["result"];
    assert_eq!(r["resultType"], "complete", "{d}");
    assert_eq!(r["supportedVersions"], json!(["2026-07-28"]));
    assert_eq!(r["_meta"]["io.modelcontextprotocol/serverInfo"]["name"], "norn");
    assert!(r["capabilities"]["tools"].is_object());
    // E1: anything but "public" or "private" makes Claude Code drop every tool.
    assert_eq!(r["cacheScope"], "private");
    assert!(r["ttlMs"].as_u64().is_some());

    let list = c.request("tools/list", json!({}));
    let names: Vec<&str> = list["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert_eq!(names, ["fs_list", "fs_read", "fs_edit", "fs_write", "exec"], "D-142: wire names use `_`");
    assert_eq!(list["result"]["cacheScope"], "private");
    for tool in list["result"]["tools"].as_array().unwrap() {
        assert_eq!(tool["inputSchema"]["type"], "object", "{tool}");
        assert!(tool["description"].as_str().is_some_and(|d| !d.is_empty()), "{tool}");
    }
    // Listing tools creates nothing: a client starts every server it knows.
    assert!(status_lines(&t).is_empty(), "no execution before the first tool call");

    let (err, text) = c.call("exec", json!({"argv": ["pytest", "-q"]}));
    assert!(!err && text.contains("exit 1"), "the demo starts with a failing test: {text}");
    let (err, text) = c.call("fs_read", json!({"path": ".env"}));
    assert!(err && text.contains("DENIED"), "{text}");
    let (err, text) = c.call("fs_write", json!({"path": "src/stats.py", "content": fixed_stats()}));
    assert!(!err, "{text}");
    let (err, text) = c.call("exec", json!({"argv": ["pytest", "-q"]}));
    assert!(!err && text.contains("exit 0"), "{text}");

    // The client ends the session the way Claude Code does.
    c.stop();
    let id = only_execution(&t);
    assert!(t.norn(&["status", &id]).contains("open"), "unfinished between sessions (D-141)");
    let apply = t.output(&["apply", &id, "--dry-run"]);
    assert!(!apply.status.success() && String::from_utf8_lossy(&apply.stderr).contains("norn finish"), "apply points at finish");

    let out = t.norn(&["finish", &id]);
    assert!(out.contains("COMPLETED"), "{out}");
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    let changed_line = fixed_stats().lines().find(|l| !std::fs::read_to_string(demo().join("src/stats.py")).unwrap().contains(l)).unwrap().to_string();
    assert!(patch.contains("+++ b/src/stats.py") && patch.contains(&format!("+{changed_line}")), "{patch}");
    assert!(!patch.contains("DEMO_API_TOKEN"), "the denied file is not in the patch");
    let again = t.norn(&["finish", &id]);
    assert!(again.contains("already finished"), "{again}");

    // The result goes into a copy of the repository like any other.
    let copy = t.root.join("copy");
    assert!(std::process::Command::new("cp").arg("-a").arg(demo()).arg(&copy).status().unwrap().success());
    t.norn(&["apply", &id, "--to", copy.to_str().unwrap()]);
    assert_eq!(std::fs::read_to_string(copy.join("src/stats.py")).unwrap(), fixed_stats());

    // The log holds the calls and what the client was told.
    let events = t.events(&id);
    assert_eq!(events[0]["agent"], "mcp");
    assert!(events[0]["task"].as_str().unwrap().contains("norn-tests"));
    let sessions = mcp_sessions(&events);
    assert_eq!(sessions.len(), 1);
    assert_eq!(sessions[0]["message"]["mcp_session"]["protocol"], "2026-07-28");
    assert_eq!(sessions[0]["message"]["mcp_session"]["tools"].as_array().unwrap().len(), 5);
    let trace = t.norn(&["trace", &id]);
    assert!(trace.contains("DENY") && trace.contains("finished: completed"), "{trace}");
}

#[test]
fn a_server_the_client_restarts_continues_the_same_execution() {
    let t = TestEnv::new("mcp-restart");
    let mut c = McpClient::start(&t, &demo());
    let (err, _) = c.call("fs_write", json!({"path": "src/notes.txt", "content": "first session\n"}));
    assert!(!err);
    // A crash, not a stop: the client sees the connection close (E3).
    c.kill();

    // Claude Code starts the server again with the same command line.
    let mut c = McpClient::start(&t, &demo());
    let (err, text) = c.call("fs_read", json!({"path": "src/notes.txt"}));
    assert!(!err && text == "first session\n", "the work of the first session is there: {text}");
    let (err, _) = c.call("fs_edit", json!({"path": "src/notes.txt", "old_str": "first", "new_str": "second"}));
    assert!(!err);
    c.stop();

    let id = only_execution(&t);
    let events = t.events(&id);
    assert_eq!(mcp_sessions(&events).len(), 2, "one record per session");
    let ids: Vec<u64> = events.iter().filter(|e| e["type"] == "tool.requested").map(|e| e["call_id"].as_u64().unwrap()).collect();
    assert_eq!(ids, [1, 2, 3], "call ids go on across the restart");
    let stderr = std::fs::read_to_string(t.root.join("mcp-stderr-1.txt")).unwrap();
    assert!(stderr.contains(&format!("continuing execution {id}")), "{stderr}");

    // Once finished, the next server starts a new execution.
    t.norn(&["finish", &id]);
    let mut c = McpClient::start(&t, &demo());
    let (_, text) = c.call("fs_read", json!({"path": "src/notes.txt"}));
    assert!(text.contains("FAILED"), "a fresh copy has no notes.txt: {text}");
    c.stop();
    assert_eq!(status_lines(&t).len(), 2);
}

#[test]
fn the_older_revision_with_initialize_is_served_too() {
    let t = TestEnv::new("mcp-legacy");
    let mut c = McpClient::start(&t, &demo());
    let init = c.request_plain(
        "initialize",
        json!({"protocolVersion": "2025-11-25", "capabilities": {}, "clientInfo": {"name": "old-client", "version": "1"}}),
    );
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25", "{init}");
    assert_eq!(init["result"]["serverInfo"]["name"], "norn");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    c.send_raw(r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#);
    // Requests without `_meta` are fine in this era; `ping` exists here.
    assert_eq!(c.request_plain("ping", json!({}))["result"], json!({}));
    let list = c.request_plain("tools/list", json!({}));
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 5, "{list}");
    assert!(list["result"].get("resultType").is_none(), "2026 fields only in the 2026 era");
    let a = c.request_plain("tools/call", json!({"name": "fs_read", "arguments": {"path": "src/stats.py"}}));
    assert_eq!(a["result"]["isError"], false, "{a}");
    c.stop();
    let events = t.events(&only_execution(&t));
    assert_eq!(mcp_sessions(&events)[0]["message"]["mcp_session"]["protocol"], "2025-11-25");
    assert!(events[0]["task"].as_str().unwrap().contains("old-client"));

    // A version norn does not know is answered with one it does.
    let mut c = McpClient::start(&t, &demo());
    let init = c.request_plain("initialize", json!({"protocolVersion": "1999-01-01", "capabilities": {}, "clientInfo": {"name": "x", "version": "1"}}));
    assert_eq!(init["result"]["protocolVersion"], "2025-11-25");
    c.stop();
}

#[test]
fn malformed_and_unknown_requests_get_the_errors_the_specification_names() {
    let t = TestEnv::new("mcp-errors");
    let mut c = McpClient::start(&t, &demo());

    // 2026-07-28: a request without the required `_meta` is malformed.
    let a = c.request_plain("tools/list", json!({}));
    assert_eq!(a["error"]["code"], -32602, "{a}");
    let mut m = meta();
    m["io.modelcontextprotocol/protocolVersion"] = json!("2030-01-01");
    let a = c.request_plain("tools/list", json!({"_meta": m}));
    assert_eq!(a["error"]["code"], -32022, "{a}");
    assert_eq!(a["error"]["data"]["supported"], json!(["2026-07-28"]));
    assert_eq!(a["error"]["data"]["requested"], "2030-01-01");

    assert_eq!(c.request("resources/list", json!({}))["error"]["code"], -32601);
    // `ping` was removed by 2026-07-28.
    assert_eq!(c.request("ping", json!({}))["error"]["code"], -32601);
    let a = c.request("tools/call", json!({"name": "rm_rf", "arguments": {}}));
    assert_eq!(a["error"]["code"], -32602, "{a}");
    assert!(a["error"]["message"].as_str().unwrap().contains("fs_read"), "names the tools: {a}");
    // A tool with the internal spelling is unknown on the wire (D-142).
    assert_eq!(c.request("tools/call", json!({"name": "fs.read", "arguments": {"path": "a"}}))["error"]["code"], -32602);

    // A wrong argument is the model's to fix: a tool error, not a protocol one.
    let (err, text) = c.call("fs_read", json!({"pth": "src/stats.py"}));
    assert!(err && text.contains("missing `path`"), "{text}");
    let (err, text) = c.call("exec", json!({"argv": "pytest -q"}));
    assert!(err && text.contains("FAILED"), "{text}");

    c.send_raw("this is not json");
    assert_eq!(c.read()["error"]["code"], -32700);
    c.send_raw("[1, 2]");
    assert_eq!(c.read()["error"]["code"], -32600);
    c.send_raw(r#"{"jsonrpc":"2.0","id":null,"method":"tools/list","params":{}}"#);
    assert_eq!(c.read()["error"]["code"], -32600, "ids may not be null");
    // Notifications and blank lines are not answered: the next answer is
    // the one for the request after them.
    c.send_raw(r#"{"jsonrpc":"2.0","method":"notifications/cancelled","params":{"requestId":"9"}}"#);
    c.send_raw("");
    let a = c.request("tools/list", json!({}));
    assert!(a["result"]["tools"].is_array(), "{a}");

    // One message larger than the limit is refused, and the server goes on.
    let huge = format!(r#"{{"jsonrpc":"2.0","id":99,"method":"tools/call","params":{{"name":"fs_write","arguments":{{"path":"big","content":"{}"}}}}}}"#, "x".repeat(33 * 1024 * 1024));
    c.send_raw(&huge);
    let a = c.read();
    assert_eq!(a["error"]["code"], -32600, "{}", a.to_string().chars().take(200).collect::<String>());
    let (err, _) = c.call("fs_list", json!({"path": "."}));
    assert!(!err, "still serving after the large message");
    assert_eq!(c.close().code(), Some(0), "closing standard input ends the server");
}

#[test]
fn the_call_budget_is_enforced_and_said() {
    let policy = std::fs::read_to_string(preset_policy()).unwrap().replace("max_tool_calls = 50", "max_tool_calls = 2");
    let t = TestEnv::new("mcp-budget").with_policy_text(&policy);
    let mut c = McpClient::start(&t, &demo());
    assert!(!c.call("fs_list", json!({"path": "."})).0);
    assert!(!c.call("fs_read", json!({"path": "src/stats.py"})).0);
    let (err, text) = c.call("fs_list", json!({"path": "."}));
    assert!(err && text.contains("budget of 2"), "{text}");
    c.stop();
    let calls = t.events(&only_execution(&t)).iter().filter(|e| e["type"] == "tool.requested").count();
    assert_eq!(calls, 2, "the refused call never reached the gateway");
}

#[test]
fn a_policy_without_commands_offers_no_exec_and_a_service_offers_ext_call() {
    let base = std::fs::read_to_string(preset_policy()).unwrap();
    let start = base.find("\n[exec]\n").unwrap() + 1;
    let end = base[start + 1..].find("\n[").map(|i| start + 1 + i).unwrap_or(base.len());
    let no_exec = format!("{}[exec]\nallow = []\n{}", &base[..start], &base[end..]);
    let t = TestEnv::new("mcp-tools").with_policy_text(&no_exec);
    let mut c = McpClient::start(&t, &demo());
    let names: Vec<String> =
        c.request("tools/list", json!({}))["result"]["tools"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert!(!names.contains(&"exec".to_string()), "{names:?}");
    c.stop();

    let with_service = format!("{base}\n[external]\n[[external.service]]\nname = \"ledger\"\nurl = \"http://127.0.0.1:9/orders\"\n");
    let t = TestEnv::new("mcp-tools-ext").with_policy_text(&with_service);
    let mut c = McpClient::start(&t, &demo());
    let tools = c.request("tools/list", json!({}))["result"]["tools"].clone();
    let ext = tools.as_array().unwrap().iter().find(|t| t["name"] == "ext_call").expect("ext_call offered");
    assert_eq!(ext["inputSchema"]["properties"]["service"]["enum"], json!(["ledger"]));
    c.stop();
}

#[test]
fn standard_output_carries_only_protocol_messages() {
    // Everything a person should read goes to standard error, so the client
    // can parse every line of standard output (stdio transport rule).
    let t = TestEnv::new("mcp-stdout");
    let mut c = McpClient::start(&t, &demo());
    c.call("exec", json!({"argv": ["pytest", "-q"]}));
    c.call("fs_write", json!({"path": "x.txt", "content": "x"}));
    // `McpClient::read` panics on any line that is not one JSON message;
    // reaching here means each answer was exactly one line.
    c.stop();
    let stderr = std::fs::read_to_string(t.root.join("mcp-stderr-0.txt")).unwrap();
    assert!(stderr.contains("new execution"), "{stderr}");
    assert!(stderr.contains("norn finish"), "the stop message says how to finish: {stderr}");
}

#[test]
fn hostile_messages_never_crash_the_server_or_reach_the_gateway() {
    // M16 (B3): what a broken or hostile client can send. Each one gets an
    // error answer (or, for something that is not a request, silence), the
    // server goes on, and nothing reaches the gateway, so no execution is
    // even created.
    let t = TestEnv::new("mcp-hostile");
    let mut c = McpClient::start(&t, &demo());
    // Nesting far deeper than the JSON parser allows: an error, not a stack overflow.
    c.send_raw(&format!("{}{}", "[".repeat(100_000), "]".repeat(100_000)));
    assert_eq!(c.read()["error"]["code"], -32700);
    // Bytes that are not UTF-8.
    c.send_bytes(b"{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"\xff\xfe\"}\n");
    assert_eq!(c.read()["error"]["code"], -32700);
    // A method that is not a string: not a request, so not answered.
    c.send_raw(r#"{"jsonrpc":"2.0","id":4,"method":7}"#);
    // `_meta` that is not an object.
    let a = c.request_plain("tools/call", json!({"name": "fs_read", "arguments": {"path": "src/stats.py"}, "_meta": "x"}));
    assert_eq!(a["id"], 1, "the next answer is for the next request: {a}");
    assert_eq!(a["error"]["code"], -32602, "{a}");
    // A tools/call before `initialize` and without `_meta`: not served.
    let a = c.request_plain("tools/call", json!({"name": "exec", "arguments": {"argv": ["pytest"]}}));
    assert_eq!(a["error"]["code"], -32602, "{a}");
    // Arguments that are not an object.
    let a = c.request("tools/call", json!({"name": "fs_read", "arguments": "src/stats.py"}));
    assert_eq!(a["result"]["isError"], true, "{a}");
    // The same id twice: both are answered, in order.
    let req = json!({"jsonrpc": "2.0", "id": "same", "method": "tools/list", "params": {"_meta": meta()}}).to_string();
    c.send_raw(&req);
    c.send_raw(&req);
    assert_eq!(c.read()["id"], "same");
    assert_eq!(c.read()["id"], "same");
    // Still serving.
    let list = c.request("tools/list", json!({}));
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 5);
    assert_eq!(c.close().code(), Some(0));
    assert!(status_lines(&t).is_empty(), "nothing reached the gateway: {:?}", status_lines(&t));
}
