//! A small MCP client for the tests: starts `norn mcp` as a child process
//! and talks to it over its standard input and output, as Claude Code does
//! (`experiments/m15-mcp/NOTES.md`).

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Stdio};

use serde_json::{json, Value};

use super::TestEnv;

pub struct McpClient {
    pub child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

/// The `_meta` every 2026-07-28 request carries.
pub fn meta() -> Value {
    json!({
        "io.modelcontextprotocol/protocolVersion": "2026-07-28",
        "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "norn-tests", "version": "0"},
    })
}

impl McpClient {
    /// `norn mcp --repo <repo> --policy <t.policy>`, with standard error
    /// kept in `<t.root>/mcp-stderr-<n>.txt`.
    pub fn start(t: &TestEnv, repo: &Path) -> McpClient {
        // Numbered in start order: mcp-stderr-0.txt, -1, …
        let n = std::fs::read_dir(&t.root)
            .map(|d| d.filter(|e| e.as_ref().is_ok_and(|e| e.file_name().to_string_lossy().starts_with("mcp-stderr-"))).count())
            .unwrap_or(0);
        let err = std::fs::File::create(t.root.join(format!("mcp-stderr-{n}.txt"))).unwrap();
        let mut child = t
            .cmd(&["mcp", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap()])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(err)
            .spawn()
            .expect("starting norn mcp");
        let stdin = child.stdin.take();
        let stdout = BufReader::new(child.stdout.take().unwrap());
        McpClient { child, stdin, stdout, next_id: 1 }
    }

    /// Send one raw line.
    pub fn send_raw(&mut self, line: &str) {
        let w = self.stdin.as_mut().expect("stdin is open");
        w.write_all(line.as_bytes()).unwrap();
        w.write_all(b"\n").unwrap();
        w.flush().unwrap();
    }

    /// Read one line from the server. Every line must be one JSON message:
    /// that is the rule of the stdio transport.
    pub fn read(&mut self) -> Value {
        let mut line = String::new();
        let n = self.stdout.read_line(&mut line).unwrap();
        assert!(n > 0, "the server closed its output");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("not a JSON message on stdout ({e}): {line:?}"))
    }

    /// A request with the 2026-07-28 `_meta`; returns the whole answer.
    pub fn request(&mut self, method: &str, params: Value) -> Value {
        let mut params = params;
        params["_meta"] = meta();
        self.request_plain(method, params)
    }

    /// A request sent exactly as given.
    pub fn request_plain(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.send_raw(&json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params}).to_string());
        let answer = self.read();
        assert_eq!(answer["id"], id, "answer to another request: {answer}");
        answer
    }

    /// `tools/call`; returns (isError, text).
    pub fn call(&mut self, tool: &str, args: Value) -> (bool, String) {
        let a = self.request("tools/call", json!({"name": tool, "arguments": args}));
        let r = &a["result"];
        assert!(r.is_object(), "tools/call {tool} gave no result: {a}");
        (r["isError"].as_bool().unwrap(), r["content"][0]["text"].as_str().unwrap().to_string())
    }

    /// How Claude Code ends a session: SIGINT (E3b). Waits for the exit.
    pub fn stop(mut self) -> std::process::ExitStatus {
        // SAFETY: kill(2) on our own child's pid, which has not been reaped.
        unsafe { libc::kill(self.child.id() as i32, libc::SIGINT) };
        self.child.wait().unwrap()
    }

    /// A crash: SIGKILL.
    pub fn kill(mut self) {
        self.child.kill().unwrap();
        self.child.wait().unwrap();
    }

    /// Close the server's input, as some clients do, and wait for the exit.
    pub fn close(mut self) -> std::process::ExitStatus {
        drop(self.stdin.take());
        self.child.wait().unwrap()
    }
}
