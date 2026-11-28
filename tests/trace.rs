//! TraceWeaver (v4, M23): `norn trace` on executions made by this version.
//! The views on logs written by 1.2 are in `tests/compat.rs`.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test trace

mod common;

use std::time::{Duration, Instant};

use common::mcp_client::McpClient;
use common::*;
use serde_json::{json, Value};

fn demo() -> std::path::PathBuf {
    repo_root().join("examples/demo-repo")
}

/// The preset, with `python3 -c` allowed.
fn policy_with_python() -> String {
    let base = std::fs::read_to_string(preset_policy()).unwrap().replace("  [\"pytest\"],\n", "  [\"pytest\"],\n  [\"python3\", \"-c\"],\n");
    assert!(base.contains("[\"python3\", \"-c\"]"), "the preset's exec list changed; update this test");
    base
}

fn of_type<'a>(events: &'a [Value], ty: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == ty).collect()
}

#[test]
fn a_running_execution_can_be_traced_and_its_call_shows_as_running() {
    let t = TestEnv::new("trace-live").with_policy_text(&policy_with_python());
    let script = t.write_script(
        "s.toml",
        "task = \"slow\"\n[[step]]\ntool = \"fs.list\"\npath = \".\"\n\
         [[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", \"import time; time.sleep(4)\"]\n",
    );
    let child = t
        .cmd(&["run", "--quiet", "--repo", demo().to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    // Wait until the command's intent is in the log, then read while it sleeps.
    let start = Instant::now();
    let id = loop {
        assert!(start.elapsed() < Duration::from_secs(20), "the run never reached the command");
        let found = std::fs::read_dir(t.state.join("executions")).ok().and_then(|mut d| d.next()).and_then(|e| e.ok()).map(|e| e.file_name().to_string_lossy().to_string());
        if let Some(id) = found {
            let text = t.output(&["trace", &id, "--json"]);
            if String::from_utf8_lossy(&text.stdout).contains("\"type\":\"tool.intent\",\"call_id\":2") {
                break id;
            }
        }
        std::thread::sleep(Duration::from_millis(100));
    };
    let live = t.norn(&["trace", &id]);
    assert!(live.contains("RUNNING: intent recorded, not ended yet") && !live.contains("UNCERTAIN"), "{live}");
    assert!(t.norn(&["trace", &id, "--call", "2"]).contains("ended      not yet: the call is running"));
    assert!(t.norn(&["status"]).contains("running"));
    // The run was not disturbed by the reads.
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success());
    let done = t.norn(&["trace", &id]);
    assert!(done.contains("exit 0") && done.contains("finished: completed"), "{done}");
    // `took` of the command is its real time: at least the 4 s it slept.
    let line = done.lines().find(|l| l.starts_with("#2")).unwrap();
    let took = line.split_whitespace().nth(2).unwrap();
    assert!(took.ends_with('s') && took.trim_end_matches('s').parse::<f64>().unwrap() >= 4.0, "{line}");
}

#[test]
fn a_resume_is_recorded_and_shown_between_the_calls() {
    let t = TestEnv::new("trace-resume");
    let out = t
        .cmd(&["run", "--quiet", "--repo", demo().to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", repo_root().join("examples/scripts/fix-demo.toml").to_str().unwrap()])
        .env("NORN_CRASH_AT", "P4")
        .env("NORN_CRASH_CALL", "6")
        .output()
        .unwrap();
    assert!(!out.status.success(), "killed at P4 of call 6");
    let id = t.norn(&["status"]).lines().next().unwrap().split_whitespace().next().unwrap().to_string();
    assert!(of_type(&t.events(&id), "execution.resumed").is_empty());
    t.norn(&["run", "--quiet", "--resume", &id]);

    let events = t.events(&id);
    let resumed = of_type(&events, "execution.resumed");
    assert_eq!(resumed.len(), 1, "{events:#?}");
    assert_eq!(resumed[0]["by"], "norn run --resume");
    assert_eq!(resumed[0]["after_calls"], 6, "calls 1 to 6 were made before the stop");
    // After recovery (which settled call 6), before the next call.
    let seq = |e: &Value| e["seq"].as_u64().unwrap();
    let done = of_type(&events, "recovery.completed");
    let call7 = events.iter().find(|e| e["type"] == "tool.requested" && e["call_id"] == 7).unwrap();
    assert!(seq(done[0]) < seq(resumed[0]) && seq(resumed[0]) < seq(call7));

    let trace = t.norn(&["trace", &id]);
    let lines: Vec<&str> = trace.lines().collect();
    let at = lines.iter().position(|l| l.contains("-- resumed by `norn run --resume` after 6 call(s)")).unwrap_or_else(|| panic!("{trace}"));
    assert!(lines[at + 1].starts_with("#7"), "the resume sits just before call 7:\n{trace}");
    assert!(lines[..at].iter().any(|l| l.contains("-- recovery after a stop: 1 call(s) without an ending")), "{trace}");
}

#[test]
fn a_new_mcp_session_on_an_open_execution_is_recorded_as_a_resume() {
    let t = TestEnv::new("trace-mcp-resume");
    let mut c = McpClient::start(&t, &demo());
    let (err, _) = c.call("fs_write", json!({"path": "src/notes.txt", "content": "first\n"}));
    assert!(!err);
    c.kill();
    let mut c = McpClient::start(&t, &demo());
    let (err, _) = c.call("fs_read", json!({"path": "src/notes.txt"}));
    assert!(!err);
    c.stop();
    let id = t.norn(&["status"]).lines().next().unwrap().split_whitespace().next().unwrap().to_string();
    let events = t.events(&id);
    let resumed = of_type(&events, "execution.resumed");
    assert_eq!(resumed.len(), 1, "the first session is a start, the second a resume: {events:#?}");
    assert_eq!(resumed[0]["by"], "norn mcp");
    assert_eq!(resumed[0]["after_calls"], 1);
    assert!(t.norn(&["trace", &id]).contains("-- resumed by `norn mcp` after 1 call(s)"));
}

#[test]
fn filters_and_views_refuse_what_does_not_fit_together() {
    let t = TestEnv::new("trace-args");
    let id = t.run_script("examples/scripts/fix-demo.toml");
    for args in [vec!["--events", "--denied"], vec!["--json", "--call", "1"], vec!["--call", "1", "--security"], vec!["--full"], vec!["--security", "--tool", "exec"]] {
        let mut all = vec!["trace", id.as_str()];
        all.extend(args.iter().copied());
        assert!(!t.output(&all).status.success(), "{args:?} should be refused");
    }
    // A tool or a glob that matches nothing shows no call, and says so.
    let none = t.norn(&["trace", &id, "--tool", "fs.delete"]);
    assert!(none.contains("showing 0 of 7 calls: tool fs.delete") && !none.lines().any(|l| l.starts_with('#')), "{none}");
    // `--path` matches the path as the policy sees it: `./src/x` is `src/x`.
    let src = t.norn(&["trace", &id, "--path", "src/*.py"]);
    let ids: Vec<&str> = src.lines().filter(|l| l.starts_with('#')).map(|l| l.split_whitespace().next().unwrap()).collect();
    assert_eq!(ids, ["#4", "#6"], "{src}");
}

// ---------------------------------------------------------------------------
// M24: causes, the graph, per-call diffs.
// ---------------------------------------------------------------------------

/// Every node a DOT edge names is declared, and every label is quoted.
fn check_dot(dot: &str) {
    assert!(dot.starts_with("digraph norn {") && dot.trim_end().ends_with('}'), "{dot}");
    let mut nodes = std::collections::HashSet::new();
    for line in dot.lines().map(str::trim) {
        if let Some((name, rest)) = line.split_once(" [") {
            if !name.contains("->") && !name.contains(' ') {
                nodes.insert(name.to_string());
                assert!(rest.contains("label=\"") || rest.starts_with("shape") || name == "node", "{line}");
            }
        }
    }
    for line in dot.lines().map(str::trim).filter(|l| l.contains(" -> ")) {
        let (a, rest) = line.split_once(" -> ").unwrap();
        let b = rest.split([' ', ';']).next().unwrap();
        assert!(nodes.contains(a) && nodes.contains(b), "an edge names a node that is not declared: {line}\n{dot}");
    }
}

#[test]
fn a_scripted_run_records_its_steps_as_causes_and_its_graph_and_diffs_show_them() {
    let t = TestEnv::new("trace-graph");
    let id = t.run_script("examples/scripts/fix-demo.toml");
    let events = t.events(&id);
    let steps: Vec<(u64, u64)> = of_type(&events, "tool.requested")
        .iter()
        .map(|r| {
            assert_eq!(r["cause"]["kind"], "step", "{r}");
            (r["call_id"].as_u64().unwrap(), r["cause"]["step"].as_u64().unwrap())
        })
        .collect();
    assert_eq!(steps, (1..=7).map(|n| (n, n)).collect::<Vec<_>>());

    let graph = t.norn(&["trace", &id, "--graph"]);
    assert!(graph.contains("script step 5 -> #5 fs.read .env  DENIED") && graph.contains("script step 6 -> #6 fs.write src/stats.py"), "{graph}");
    check_dot(&t.norn(&["trace", &id, "--graph=dot"]));
    assert!(t.norn(&["trace", &id, "--call", "6"]).contains("cause      script step 6"));

    // Per-call diffs.
    let fix = t.norn(&["trace", &id, "--diff", "6"]);
    assert!(fix.starts_with("# call #6 fs.write src/stats.py (548 bytes): cp-0001 -> cp-0002"), "{fix}");
    assert!(fix.contains("-    return sum(values) / (len(values) - 1)") && fix.contains("+    return sum(values) / len(values)"), "{fix}");
    assert!(t.norn(&["trace", &id, "--diff", "2"]).contains("no change to the files"), "pytest changed no file a patch shows");
    assert!(t.norn(&["trace", &id, "--diff", "1"]).contains("changed no files: only calls that can change files get a checkpoint"));
    assert!(t.norn(&["trace", &id, "--diff", "5"]).contains("was not run"));
    // The whole run holds the same changes as result.patch.
    let whole = t.norn(&["trace", &id, "--diff"]);
    let patch = std::fs::read_to_string(t.exec_dir(&id).join("result.patch")).unwrap();
    let changes = |s: &str| -> Vec<String> { s.lines().filter(|l| (l.starts_with('+') || l.starts_with('-')) && !l.starts_with("+++") && !l.starts_with("---")).map(str::to_string).collect() };
    assert_eq!(changes(&whole), changes(&patch));
    assert!(!changes(&patch).is_empty());
    // Reading wrote nothing into the execution.
    let before = tree_digest(&t.exec_dir(&id).join("checkpoints"));
    t.norn(&["trace", &id, "--diff", "6"]);
    assert_eq!(tree_digest(&t.exec_dir(&id).join("checkpoints")), before);
}

#[test]
fn an_mcp_call_names_its_request_and_an_approval_links_two_calls() {
    let policy = format!("{}\n[approvals]\nrequire = [\"fs.delete:**\"]\n", std::fs::read_to_string(preset_policy()).unwrap());
    let t = TestEnv::new("trace-mcp-cause").with_policy_text(&policy);
    let mut c = McpClient::start(&t, &demo());
    let (err, _) = c.call("fs_write", json!({"path": "src/old.py", "content": "OLD = 1\n"}));
    assert!(!err);
    let (_, text) = c.call("fs_delete", json!({"path": "src/old.py"}));
    let id = t.norn(&["status"]).lines().next().unwrap().split_whitespace().next().unwrap().to_string();
    let approval = text.split_whitespace().skip_while(|w| *w != "approve").nth(2).unwrap().trim_end_matches(['`', ')', '.']).to_string();
    t.norn(&["approve", &id, &approval]);
    let (err, _) = c.call("fs_delete", json!({"path": "src/old.py"}));
    assert!(!err);
    // A string id the client chose, and one too long to keep.
    let a = c.request_plain("tools/call", json!({"name": "fs_list", "arguments": {"path": "."}, "_meta": common::mcp_client::meta()}));
    assert!(a["result"].is_object(), "{a}");
    c.send_raw(&json!({"jsonrpc": "2.0", "id": "req-ä", "method": "tools/call", "params": {"name": "fs_list", "arguments": {"path": "src"}, "_meta": common::mcp_client::meta()}}).to_string());
    assert_eq!(c.read()["id"], "req-ä");
    let long = "x".repeat(300);
    c.send_raw(&json!({"jsonrpc": "2.0", "id": long, "method": "tools/call", "params": {"name": "fs_list", "arguments": {"path": "tests"}, "_meta": common::mcp_client::meta()}}).to_string());
    assert_eq!(c.read()["id"], long.as_str());
    c.stop();

    let events = t.events(&id);
    let causes: Vec<Value> = of_type(&events, "tool.requested").iter().map(|r| r["cause"].clone()).collect();
    assert_eq!(causes.len(), 6);
    for (i, cause) in causes[..4].iter().enumerate() {
        assert_eq!(cause["kind"], "mcp_request", "{cause}");
        assert_eq!(cause["id"], (i + 1) as u64, "the client's own request ids, 1 to 4");
    }
    assert_eq!(causes[4], json!({"kind": "mcp_request", "id": "req-ä"}));
    assert!(causes[5].is_null(), "a 300-byte id is not kept: {}", causes[5]);

    let graph = t.norn(&["trace", &id, "--graph"]);
    assert!(graph.contains("client request 1 (its reasons are not sent to norn) -> #1 fs.write"), "{graph}");
    assert!(graph.contains("cause not recorded -> #6 fs.list"), "{graph}");
    assert!(graph.contains(&format!("{approval}: #2 waited -> ")) && graph.contains("said yes, #3 ran"), "{graph}");
    let dot = t.norn(&["trace", &id, "--graph=dot"]);
    check_dot(&dot);
    assert!(dot.contains(&format!("call2 -> appr_{approval} [label=\"waited\"]")), "{dot}");
    assert!(dot.contains("req_req___"), "a string id becomes a safe node name: {dot}");
}

// ---------------------------------------------------------------------------
// M25: replay and run diff (scripted runs; the LLM's are in tests/llm_agent.rs).
// ---------------------------------------------------------------------------

fn untouched(t: &TestEnv, id: &str) -> (Vec<String>, Vec<String>, Vec<String>, Vec<u8>) {
    let d = t.exec_dir(id);
    (tree_digest(&d.join("work")), tree_digest(&d.join("checkpoints")), tree_digest(&d.join("blobs")), std::fs::read(d.join("log.db")).unwrap())
}

#[test]
fn a_scripted_replay_matches_its_own_script_and_stops_where_another_differs() {
    let t = TestEnv::new("trace-replay");
    let id = t.run_script("examples/scripts/fix-demo.toml");
    let before = untouched(&t, &id);
    let same = t.norn(&["replay", &id]);
    assert!(same.contains("the replay asked for the same 7 call(s) as the recording\nand then ended the same way (completed): no divergence"), "{same}");
    assert!(same.contains("tokens not recorded"));

    let script = std::fs::read_to_string(repo_root().join("examples/scripts/fix-demo.toml")).unwrap();
    // Another path at step 4: stops there, names the field.
    let other = t.write_script("other.toml", &script.replacen("path = \"src/stats.py\"", "path = \"src/__init__.py\"", 1));
    let text = t.norn(&["replay", &id, "--script", other.to_str().unwrap()]);
    assert!(text.contains("same 3 call(s)") && text.contains("DIVERGENCE at call #4:") && text.contains("replayed  fs.read src/__init__.py") && text.contains("differs in: path"), "{text}");
    // Another content for the write (step 6): the hash differs; the write is not made.
    let other = t.write_script("write.toml", &script.replace("return sum(values) / len(values)", "return 0"));
    let text = t.norn(&["replay", &id, "--script", other.to_str().unwrap()]);
    assert!(text.contains("DIVERGENCE at call #6:") && text.contains("differs in: bytes, content_hash"), "{text}");
    // Shorter and longer scripts.
    let steps: Vec<&str> = script.split("\n[[step]]").collect();
    let short = t.write_script("short.toml", &steps[..3].join("\n[[step]]"));
    let text = t.norn(&["replay", &id, "--script", short.to_str().unwrap()]);
    assert!(text.contains("DIVERGENCE: the replay ended (completed) after 2 call(s); the recording went on to 7 call(s)"), "{text}");
    let long = t.write_script("long.toml", &format!("{script}\n[[step]]\ntool = \"fs.delete\"\npath = \"src/stats.py\"\n"));
    let text = t.norn(&["replay", &id, "--script", long.to_str().unwrap()]);
    assert!(text.contains("DIVERGENCE: the recording ended here, and the replay asked for call #8: fs.delete src/stats.py"), "{text}");
    // Nothing of the above was run or written.
    assert_eq!(untouched(&t, &id), before);
    assert_eq!(std::fs::read_dir(t.state.join("executions")).unwrap().count(), 1);
    assert!(!t.output(&["replay", &id, "--model", "m"]).status.success(), "a script is not a model");
}

#[test]
fn an_mcp_execution_cannot_be_replayed_and_says_why() {
    let t = TestEnv::new("trace-replay-mcp");
    let mut c = McpClient::start(&t, &demo());
    c.call("fs_list", json!({"path": "."}));
    c.stop();
    let id = t.norn(&["status"]).lines().next().unwrap().split_whitespace().next().unwrap().to_string();
    let out = t.output(&["replay", &id]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("driven by an MCP client; its reasoning is not in the log, so it cannot be replayed"));
}

#[test]
fn a_run_diff_lines_up_the_calls_and_finds_nothing_against_itself() {
    let t = TestEnv::new("trace-rundiff");
    let a = t.run_script("examples/scripts/fix-demo.toml");
    let itself = t.norn(&["diff", &a, &a]);
    assert!(itself.contains("7 call(s) the same and in the same order") && itself.contains("no differences in calls, results, ending or patch"), "{itself}");

    let script = std::fs::read_to_string(repo_root().join("examples/scripts/fix-demo.toml")).unwrap();
    // B reads another file at step 4 and leaves the bug in: the last pytest fails.
    let other = t.write_script("other.toml", &script.replacen("path = \"src/stats.py\"", "path = \"src/__init__.py\"", 1).replace("return sum(values) / len(values)", "return sum(values) / (len(values) - 1)"));
    let b = t.run_script(other.to_str().unwrap());
    let d = t.norn(&["diff", &a, &b]);
    for line in [
        "task same, repository same, policy same",
        "5 call(s) the same and in the same order",
        "- #4        fs.read src/stats.py  (only in A)",
        "+      #4   fs.read src/__init__.py  (only in B)",
        "~ #6   #6   fs.write src/stats.py (548 bytes): other arguments: bytes, content_hash",
        "! #7   #7   exec pytest -q  A: exit 0  B: exit 1",
        "= #2   #2   exec pytest -q  exit 1",
        "patches: DIFFERENT",
    ] {
        assert!(d.contains(line), "`{line}` is missing from:\n{d}");
    }
    assert!(d.contains("difference(s)") && !d.contains("no differences"));
}

// ---------------------------------------------------------------------------
// M26: OpenTelemetry file and search.
// ---------------------------------------------------------------------------

/// Checks the OTLP JSON rules found in E4 and returns the spans: one trace
/// id, lower-case hex ids of the right length, unique span ids, every parent
/// and link inside the file, start not after end.
fn check_otlp(text: &str) -> Vec<Value> {
    let doc: Value = serde_json::from_str(text).unwrap();
    let spans = doc["resourceSpans"][0]["scopeSpans"][0]["spans"].as_array().unwrap().clone();
    let hex = |s: &str, n: usize| s.len() == n && s.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase());
    let trace = spans[0]["traceId"].as_str().unwrap().to_string();
    assert!(hex(&trace, 32), "{trace}");
    let ids: Vec<&str> = spans.iter().map(|s| s["spanId"].as_str().unwrap()).collect();
    let mut unique = ids.clone();
    unique.sort();
    unique.dedup();
    assert_eq!(unique.len(), ids.len(), "span ids are unique");
    for s in &spans {
        assert_eq!(s["traceId"], trace.as_str());
        assert!(hex(s["spanId"].as_str().unwrap(), 16), "{s}");
        if let Some(p) = s.get("parentSpanId") {
            assert!(ids.contains(&p.as_str().unwrap()), "a parent that is not in the file (the collector would not notice, E4): {s}");
        }
        for l in s["links"].as_array().into_iter().flatten() {
            assert!(ids.contains(&l["spanId"].as_str().unwrap()), "{l}");
        }
        let (a, b): (u128, u128) = (s["startTimeUnixNano"].as_str().unwrap().parse().unwrap(), s["endTimeUnixNano"].as_str().unwrap().parse().unwrap());
        assert!(a <= b && a > 1_700_000_000_000_000_000, "{s}");
    }
    assert_eq!(spans.iter().filter(|s| s.get("parentSpanId").is_none()).count(), 1, "one root: the execution");
    spans
}

fn attr<'a>(span: &'a Value, key: &str) -> Option<&'a Value> {
    span["attributes"].as_array()?.iter().find(|a| a["key"] == key).map(|a| &a["value"])
}

#[test]
fn a_run_as_opentelemetry_spans_keeps_its_parts_causes_and_approvals() {
    // A run killed and resumed: three parts.
    let t = TestEnv::new("trace-otel");
    let out = t
        .cmd(&["run", "--quiet", "--repo", demo().to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", repo_root().join("examples/scripts/fix-demo.toml").to_str().unwrap()])
        .env("NORN_CRASH_AT", "P4")
        .env("NORN_CRASH_CALL", "6")
        .output()
        .unwrap();
    assert!(!out.status.success());
    let id = t.norn(&["status"]).lines().next().unwrap().split_whitespace().next().unwrap().to_string();
    t.norn(&["run", "--quiet", "--resume", &id]);
    let text = t.norn(&["trace", &id, "--otel"]);
    assert_eq!(text, t.norn(&["trace", &id, "--otel"]), "the same run gives the same file");
    let spans = check_otlp(&text);
    let names: Vec<&str> = spans.iter().map(|s| s["name"].as_str().unwrap()).collect();
    assert_eq!(names[..4], ["execution", "start", "recovery after a stop", "resumed by norn run --resume after 6 call(s)"], "{names:?}");
    assert_eq!(spans.len(), 4 + 7);
    let call = |n: u64| spans.iter().find(|s| attr(s, "norn.call_id").is_some_and(|v| v["intValue"].as_str() == Some(n.to_string().as_str()))).unwrap();
    assert_eq!(call(5)["status"]["code"], 2, "the denied read is an error span");
    assert!(call(5)["status"]["message"].as_str().unwrap().starts_with("denied: filesystem.deny"));
    assert_eq!(attr(call(6), "norn.reconciled").unwrap()["stringValue"], "applied");
    assert_eq!(attr(call(3), "norn.cause").unwrap()["stringValue"], "script step 3");
    assert_eq!(call(7)["parentSpanId"], spans[3]["spanId"], "call 7 is inside the resumed part");
    assert_eq!(call(1)["parentSpanId"], spans[1]["spanId"]);
    assert_eq!(attr(&spans[0], "norn.status").unwrap()["stringValue"], "completed");

    // An approval: the call that used it links to the call that waited.
    let policy = format!("{}\n[approvals]\nrequire = [\"fs.delete:**\"]\n", std::fs::read_to_string(preset_policy()).unwrap());
    let t = TestEnv::new("trace-otel-approval").with_policy_text(&policy);
    let mut c = McpClient::start(&t, &demo());
    c.call("fs_write", json!({"path": "src/old.py", "content": "OLD = 1\n"}));
    let (_, text) = c.call("fs_delete", json!({"path": "src/old.py"}));
    let id = t.norn(&["status"]).lines().next().unwrap().split_whitespace().next().unwrap().to_string();
    let approval = text.split_whitespace().skip_while(|w| *w != "approve").nth(2).unwrap().trim_end_matches(['`', ')', '.']).to_string();
    t.norn(&["approve", &id, &approval]);
    c.call("fs_delete", json!({"path": "src/old.py"}));
    c.stop();
    let spans = check_otlp(&t.norn(&["trace", &id, "--otel"]));
    let call = |n: u64| spans.iter().find(|s| attr(s, "norn.call_id").is_some_and(|v| v["intValue"].as_str() == Some(n.to_string().as_str()))).unwrap();
    assert_eq!(call(3)["links"][0]["spanId"], call(2)["spanId"]);
    assert_eq!(attr(call(2), "norn.approval.waiting").unwrap()["stringValue"], approval.as_str());
    assert_eq!(attr(call(1), "norn.cause").unwrap()["stringValue"], "MCP request 1");
}

#[test]
fn search_finds_calls_across_executions() {
    let t = TestEnv::new("trace-search");
    let a = t.run_script("examples/scripts/fix-demo.toml");
    let script = std::fs::read_to_string(repo_root().join("examples/scripts/fix-demo.toml")).unwrap();
    let other = t.write_script("other.toml", &script.replace("return sum(values) / len(values)", "return sum(values) / (len(values) - 1)"));
    let b = t.run_script(other.to_str().unwrap());

    let denied = t.norn(&["search", "--outcome", "denied"]);
    assert!(denied.contains(&format!("{a}  scripted #5   fs.read   .env")) && denied.contains(&format!("{b}  scripted #5")), "{denied}");
    assert!(denied.contains("2 call(s) in 2 execution(s)"), "{denied}");
    let writes = t.norn(&["search", "--tool", "fs.write", "--path", "src/**"]);
    assert!(writes.contains("2 call(s) in 2 execution(s)"), "{writes}");
    assert_eq!(t.norn(&["search", "--tool", "exec", "--outcome", "done"]).lines().filter(|l| l.contains("#2") || l.contains("#7")).count(), 4);
    // Executions are dated by their first event, in UTC.
    assert!(t.norn(&["search", "--since", "2999-01-01"]).contains("0 call(s) in 0 execution(s)"));
    assert!(t.norn(&["search", "--since", "2000-01-01"]).contains("14 call(s) in 2 execution(s)"));
    assert!(!t.output(&["search", "--since", "yesterday"]).status.success());
    assert!(!t.output(&["search", "--outcome", "broken"]).status.success());
    // A log that cannot be read is named, and the others are still searched.
    std::fs::write(t.exec_dir(&b).join("log.db"), b"not a database").unwrap();
    let out = t.norn(&["search", "--outcome", "denied"]);
    assert!(out.contains("1 call(s) in 1 execution(s)") && out.contains(&format!("not read: {b}")), "{out}");
}

#[test]
fn what_a_command_printed_cannot_send_commands_to_the_terminal() {
    // A command in the sandbox prints an escape sequence that would retitle
    // a terminal window; the inspector shows it as text (D-175).
    let t = TestEnv::new("trace-escape").with_policy_text(&policy_with_python());
    let script = t.write_script(
        "s.toml",
        "task = \"esc\"\n[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"-c\", \"print('\\\\x1b]0;owned\\\\x07 first'); print('\\\\x1b[2J second')\"]\n",
    );
    let id = t.run_script(script.to_str().unwrap());
    let events = t.events(&id);
    let stdout = t.output_of(&id, &events, 1).unwrap()["stdout"].as_str().unwrap().to_string();
    assert!(stdout.contains("\u{1b}]0;owned\u{7} first"), "the record keeps what the command printed: {stdout:?}");
    for args in [vec!["trace", id.as_str(), "--call", "1"], vec!["trace", id.as_str()], vec!["trace", id.as_str(), "--graph"], vec!["trace", id.as_str(), "--events"]] {
        let out = t.norn(&args);
        assert!(!out.contains('\u{1b}') && !out.contains('\u{7}'), "{args:?} printed a control character:\n{out:?}");
    }
    let call = t.norn(&["trace", &id, "--call", "1"]);
    assert!(call.contains("| \\u{1b}]0;owned\\u{7} first") && call.contains("| \\u{1b}[2J second"), "{call}");
    // JSON needs no filter: serde_json writes the characters escaped.
    assert!(!t.norn(&["trace", &id, "--json"]).contains('\u{1b}'));
}
