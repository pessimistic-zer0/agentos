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
