//! Approvals (M20; D-153, D-159): calls named in `[approvals] require` wait
//! for a person's yes, given with `norn approve` from the person's shell.
//!
//! The tests drive `norn mcp` the way a client does: a call is answered
//! "waiting", the person runs `norn approve` (or `norn deny`), and the same
//! call is sent again.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test approvals

mod common;

use common::mcp_client::McpClient;
use common::*;
use serde_json::{json, Value};

fn demo() -> std::path::PathBuf {
    repo_root().join("examples/demo-repo")
}

fn policy(require: &[&str]) -> String {
    let list: Vec<String> = require.iter().map(|r| format!("{r:?}")).collect();
    format!("{}\n[approvals]\nrequire = [{}]\n", std::fs::read_to_string(preset_policy()).unwrap(), list.join(", "))
}

fn only_execution(t: &TestEnv) -> String {
    let out = t.norn(&["status"]);
    let lines: Vec<&str> = out.lines().filter(|l| !l.starts_with("no executions")).collect();
    assert_eq!(lines.len(), 1, "{lines:?}");
    lines[0].split_whitespace().next().unwrap().to_string()
}

/// The approval id in a "waiting" answer: the word after `norn approve <exec>`.
fn approval_in(text: &str) -> String {
    let at = text.find("norn approve ").unwrap_or_else(|| panic!("no approve command in: {text}"));
    text[at..].split_whitespace().nth(3).unwrap().trim_end_matches(['`', ')', '.']).to_string()
}

fn of_type<'a>(events: &'a [Value], ty: &str) -> Vec<&'a Value> {
    events.iter().filter(|e| e["type"] == ty).collect()
}

#[test]
fn a_write_waits_runs_after_approval_and_only_once() {
    let t = TestEnv::new("appr-write").with_policy_text(&policy(&["fs.write:tests/**"]));
    let mut c = McpClient::start(&t, &demo());
    let args = json!({"path": "tests/test_new.py", "content": "def test_new():\n    assert True\n"});

    // Not named by the rule: runs at once.
    let (err, _) = c.call("fs_write", json!({"path": "src/note.txt", "content": "x\n"}));
    assert!(!err);

    let (err, text) = c.call("fs_write", args.clone());
    assert!(err && text.starts_with("WAITING FOR APPROVAL"), "{text}");
    assert!(text.contains(r#"approvals.require[0] "fs.write:tests/**""#), "{text}");
    let id = only_execution(&t);
    let approval = approval_in(&text);
    assert!(!t.exec_dir(&id).join("work/tests/test_new.py").exists(), "nothing was written");

    // Asked again before anyone answered: the same approval, still waiting.
    let (err, again) = c.call("fs_write", args.clone());
    assert!(err && approval_in(&again) == approval, "the same call names the same approval: {again}");

    // A different content is a different call.
    let (_, other) = c.call("fs_write", json!({"path": "tests/test_new.py", "content": "something else\n"}));
    assert_ne!(approval_in(&other), approval);

    // The person sees what is waiting, with the diff, and approves.
    let list = t.norn(&["approve", &id]);
    assert!(list.contains(&approval) && list.contains("+def test_new():"), "{list}");
    assert!(list.contains("--- a/tests/test_new.py") && list.contains("+++ b/tests/test_new.py"), "{list}");
    let said = t.norn(&["approve", &id, &approval]);
    assert!(said.contains(&format!("approved {approval}")), "{said}");

    let (err, text) = c.call("fs_write", args.clone());
    assert!(!err, "{text}");
    assert_eq!(std::fs::read_to_string(t.exec_dir(&id).join("work/tests/test_new.py")).unwrap(), "def test_new():\n    assert True\n");

    // Used once: the same call again waits for a new yes.
    let (err, text) = c.call("fs_write", args);
    assert!(err && approval_in(&text) == approval, "{text}");
    c.stop();

    let events = t.events(&id);
    let used = of_type(&events, "approval.used");
    assert_eq!(used.len(), 1, "{events:#?}");
    assert_eq!(used[0]["approval"], approval.as_str());
    assert!(!used[0]["by"].as_str().unwrap().is_empty());
    // The yes is logged before the intent of the call it lets through.
    let call = used[0]["call_id"].as_u64().unwrap();
    let seq = |ty: &str| events.iter().find(|e| e["type"] == ty && e["call_id"] == call).unwrap()["seq"].as_u64().unwrap();
    assert!(seq("approval.used") < seq("tool.intent") && seq("tool.intent") < seq("tool.completed"));
    assert_eq!(of_type(&events, "approval.requested").len(), 4);
    let trace = t.norn(&["trace", &id]);
    assert!(trace.contains("WAIT") && trace.contains(&approval), "{trace}");
}

#[test]
fn a_refusal_is_final_for_that_exact_call() {
    let t = TestEnv::new("appr-deny").with_policy_text(&policy(&["fs.delete"]));
    let mut c = McpClient::start(&t, &demo());
    let (err, text) = c.call("fs_delete", json!({"path": "src/stats.py"}));
    assert!(err && text.contains("WAITING"), "{text}");
    let id = only_execution(&t);
    let approval = approval_in(&text);
    t.norn(&["deny", &id, &approval]);
    let (err, text) = c.call("fs_delete", json!({"path": "src/stats.py"}));
    assert!(err && text.contains("refused this exact call"), "{text}");
    let (err, text) = c.call("fs_delete", json!({"path": "src/stats.py"}));
    assert!(err && text.contains("refused this exact call"), "asked a third time: {text}");
    assert!(t.exec_dir(&id).join("work/src/stats.py").exists());
    c.stop();
    // Nothing is pending any more, and the refusal cannot be turned into a yes.
    assert!(t.norn(&["approve", &id]).contains("nothing is waiting"));
    let out = t.output(&["approve", &id, &approval]);
    assert!(!out.status.success());
    assert_eq!(of_type(&t.events(&id), "approval.refused").len(), 2);
}

#[test]
fn an_expired_approval_is_not_used() {
    let t = TestEnv::new("appr-expire").with_policy_text(&policy(&["exec:pytest"]));
    let mut c = McpClient::start(&t, &demo());
    let (_, text) = c.call("exec", json!({"argv": ["pytest", "-q"]}));
    let id = only_execution(&t);
    let approval = approval_in(&text);
    t.norn(&["approve", &id, &approval, "--minutes", "0"]);
    let (err, text) = c.call("exec", json!({"argv": ["pytest", "-q"]}));
    assert!(err && text.contains("WAITING"), "an approval for 0 minutes has expired: {text}");
    // Another argv is not covered by the old request, but `exec:pytest` is a prefix rule.
    let (err, text) = c.call("exec", json!({"argv": ["pytest", "-q", "-x"]}));
    assert!(err && approval_in(&text) != approval, "{text}");
    c.stop();
    assert!(of_type(&t.events(&id), "approval.used").is_empty());
}

#[test]
fn approval_is_on_top_of_the_policy_never_instead_of_it() {
    // A call the policy refuses is refused, whatever [approvals] says: it
    // never reaches the approval step, so nobody is asked to approve it.
    let t = TestEnv::new("appr-policy").with_policy_text(&policy(&["fs.write", "fs.delete"]));
    let mut c = McpClient::start(&t, &demo());
    let (err, text) = c.call("fs_write", json!({"path": "README.md", "content": "x\n"}));
    assert!(err && text.starts_with("DENIED"), "{text}");
    // The policy step runs again on every attempt, before the approval
    // step: an approved delete of a file that is gone by then fails as any
    // delete would, and the approval is not used up.
    let args = json!({"path": "src/stats.py"});
    let (_, text) = c.call("fs_delete", args.clone());
    let id = only_execution(&t);
    let approval = approval_in(&text);
    t.norn(&["approve", &id, &approval]);
    std::fs::remove_file(t.exec_dir(&id).join("work/src/stats.py")).unwrap();
    let (err, text) = c.call("fs_delete", args);
    assert!(err && text.starts_with("FAILED"), "{text}");
    c.stop();
    let events = t.events(&id);
    assert!(of_type(&events, "approval.used").is_empty(), "{events:#?}");
    assert_eq!(of_type(&events, "approval.requested").len(), 1, "the denied write asked nobody");
}

#[test]
fn approve_refuses_an_id_that_is_not_waiting() {
    let t = TestEnv::new("appr-typo").with_policy_text(&policy(&["fs.delete"]));
    let mut c = McpClient::start(&t, &demo());
    let (_, text) = c.call("fs_delete", json!({"path": "src/stats.py"}));
    let id = only_execution(&t);
    c.stop();
    let out = t.output(&["approve", &id, "a0000000000"]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("no pending approval `a0000000000`"));
    assert!(!t.exec_dir(&id).join("approvals").exists() || std::fs::read_dir(t.exec_dir(&id).join("approvals")).unwrap().count() == 0);
    let _ = text;
}

#[test]
fn without_an_approvals_section_nothing_waits() {
    let t = TestEnv::new("appr-off");
    let mut c = McpClient::start(&t, &demo());
    let (err, text) = c.call("fs_delete", json!({"path": "src/stats.py"}));
    assert!(!err, "{text}");
    c.stop();
}

#[test]
fn the_agent_cannot_write_its_own_approval() {
    // The answer file lives in the execution folder, next to the workspace,
    // never inside it (design rule 9). Every way the agent has of writing a
    // file starts in the workspace and cannot leave it.
    let with_python = policy(&["fs.delete"]).replace("  [\"pytest\"],\n", "  [\"pytest\"],\n  [\"python3\", \"-c\"],\n");
    assert!(with_python.contains("[\"python3\", \"-c\"]"), "the preset's exec list changed; update this test");
    let t = TestEnv::new("appr-self").with_policy_text(&with_python);
    let mut c = McpClient::start(&t, &demo());
    let (_, text) = c.call("fs_delete", json!({"path": "src/stats.py"}));
    let id = only_execution(&t);
    let approval = approval_in(&text);
    let forged = json!({"approve": true, "by": "agent", "at": "2026-01-01T00:00:00Z", "expires_unix": 4_000_000_000u64}).to_string();
    for path in [format!("../approvals/{approval}.json"), format!("/proc/self/cwd/../approvals/{approval}.json")] {
        let (err, text) = c.call("fs_write", json!({"path": path, "content": forged}));
        assert!(err && text.starts_with("DENIED"), "{path}: {text}");
    }
    let script = format!("import os\nos.makedirs('../approvals', exist_ok=True)\nopen('../approvals/{approval}.json', 'w').write({forged:?})\n");
    let (_, text) = c.call("exec", json!({"argv": ["python3", "-c", script]}));
    assert!(!text.starts_with("DENIED"), "the command must really run in the sandbox: {text}");
    let (err, text) = c.call("fs_delete", json!({"path": "src/stats.py"}));
    assert!(err && text.contains("WAITING"), "still waiting: {text}");
    c.stop();
    assert!(!t.exec_dir(&id).join("approvals").exists(), "nothing reached the execution folder");
    assert!(t.exec_dir(&id).join("work/src/stats.py").exists());
}
