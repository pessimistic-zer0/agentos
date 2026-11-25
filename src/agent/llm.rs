//! The LLM agent: a minimal loop against an OpenAI-compatible chat endpoint
//! (a local llama.cpp server, or a hosted API).
//!
//! The model is called by the trusted runtime, outside the sandbox (v1
//! design: "The LLM is called by the runtime"). The model never touches
//! anything: it answers with a *tool call*, which the runtime passes to the
//! gateway like any other agent's request. Whatever the model was told by a
//! hostile file, the policy and the sandbox still decide what happens.
//!
//! Every message of the conversation is handed to the runtime to be stored
//! in the event log (ADR 0005), so a restarted runtime can rebuild the
//! conversation exactly. The API key is never logged or saved.

use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::agent::{Action, Agent};
use crate::log::{Event, EventBody};
use crate::tools::{ToolCall, ToolOutcome, ToolOutput, ToolResult};
use crate::workspace::ExecDir;

/// Where the model is. Saved in the execution folder (without the key) so a
/// resumed run talks to the same model.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    /// For example `http://127.0.0.1:8080/v1`.
    pub base_url: String,
    pub model: String,
    pub task: String,
    /// From `NORN_LLM_API_KEY`; never serialized.
    #[serde(skip)]
    pub api_key: Option<String>,
}

impl LlmConfig {
    /// Read `NORN_LLM_BASE_URL` (default: a local llama.cpp server),
    /// `NORN_LLM_MODEL` and `NORN_LLM_API_KEY` (optional).
    pub fn from_env(task: String) -> LlmConfig {
        LlmConfig {
            base_url: crate::env_var("LLM_BASE_URL").unwrap_or_else(|| "http://127.0.0.1:8080/v1".into()),
            model: crate::env_var("LLM_MODEL").unwrap_or_else(|| "default".into()),
            task,
            api_key: crate::env_var("LLM_API_KEY").filter(|k| !k.is_empty()),
        }
    }

    /// Can a connection to the server be opened at all? Called before an
    /// execution is created, so that "no server is running" is reported at
    /// once, with the variables that decide where the server is, and not
    /// after the workspace was copied, leaving a half-started execution.
    ///
    /// Only a TCP connection is tried: every OpenAI-compatible server
    /// accepts that, while a harmless request that all of them answer does
    /// not exist. Skipped when a proxy variable is set, because the request
    /// then does not go to this address directly.
    pub fn check_reachable(&self) -> Result<()> {
        use std::net::ToSocketAddrs;
        if ["http_proxy", "https_proxy", "all_proxy", "HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY"].iter().any(|v| std::env::var_os(v).is_some()) {
            return Ok(());
        }
        let set = |v: &str| if crate::env_var(v.trim_start_matches("NORN_")).is_some() { "set" } else { "not set" };
        let help = format!(
            "The LLM agent finds its server through the environment:\n    \
             NORN_LLM_BASE_URL  ({}; in use: {})\n    \
             NORN_LLM_MODEL     ({}; in use: {})\n    \
             NORN_LLM_API_KEY   ({}; only needed for a hosted API)\n  \
             For a local model, start a server first, for example:\n    \
             llama-server -m <model>.gguf --jinja --port 8080",
            set("NORN_LLM_BASE_URL"),
            self.base_url,
            set("NORN_LLM_MODEL"),
            self.model,
            set("NORN_LLM_API_KEY"),
        );
        let uri: ureq::http::Uri = self.base_url.parse().map_err(|e| anyhow::anyhow!("NORN_LLM_BASE_URL `{}` is not a URL ({e}).\n  {help}", self.base_url))?;
        let (Some(host), Some(scheme)) = (uri.host(), uri.scheme_str()) else {
            bail!("NORN_LLM_BASE_URL `{}` must look like http://host:port/v1.\n  {help}", self.base_url);
        };
        let port = uri.port_u16().unwrap_or(if scheme == "https" { 443 } else { 80 });
        let host = host.trim_start_matches('[').trim_end_matches(']'); // IPv6 literal
        let mut last = String::from("the name did not resolve to an address");
        for addr in (host, port).to_socket_addrs().map_err(|e| anyhow::anyhow!("cannot look up `{host}` ({e}).\n  {help}"))? {
            match std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
                Ok(_) => return Ok(()),
                Err(e) => last = e.to_string(),
            }
        }
        bail!("no LLM server answers at {} ({last}). Nothing was started.\n  {help}", self.base_url)
    }
}

const SYSTEM_PROMPT: &str = "You are a coding agent working on a copy of a code repository. \
You can only act through the tools: fs_list, fs_read, fs_edit, fs_write and exec. Paths are relative to the repository root.\n\
Rules:\n\
1. Call exactly one tool per reply, and keep any text short.\n\
2. To change an existing file, use fs_edit: first fs_read the file, then give old_str (the exact text to replace, \
copied from the file, with enough of the lines around it that it occurs only once) and new_str (what to put there). \
The rest of the file stays as it is.\n\
3. fs_write is for a NEW file. It REPLACES THE WHOLE FILE: the content you send becomes the entire file. \
Never fs_write only the changed function of an existing file: every other function in it would be deleted.\n\
4. exec runs one allowed command as a list of arguments (no shell). The policy decides which commands are allowed; \
a refusal names the rule.\n\
5. After a change, run the tests. If they fail, read the error, fs_read the file you changed to see what it really contains now, and fix it.\n\
6. When the tests pass, reply with one or two sentences in plain text and no tool call. Do not stop before the tests pass.";

/// Upper bound on one reply from the model.
const MAX_REPLY_TOKENS: u32 = 4096;
/// The final answer is stored as the run's summary; keep it short.
const MAX_SUMMARY_CHARS: usize = 600;

/// How much of a tool result is sent back to the model.
const MAX_RESULT_CHARS: usize = 12_000;

pub struct LlmAgent {
    cfg: LlmConfig,
    /// The conversation so far, in OpenAI chat format.
    messages: Vec<Value>,
    /// Messages not yet handed to the runtime for logging.
    unlogged: Vec<Value>,
    /// Id of the tool call the model is waiting on, if any.
    pending: Option<String>,
    /// How often each read-only call was made since the last write. A model
    /// that repeats itself is told so, and stopped if it keeps going.
    repeats: std::collections::HashMap<String, u32>,
    last_key: Option<String>,
    /// The call being answered, and the last content read or written per
    /// file, used to warn when a write drops many lines.
    last_call: Option<ToolCall>,
    seen: std::collections::HashMap<String, String>,
    /// The last command the model ran did not exit with 0, and it has not
    /// run one since. A model that stops in this state is not done (D-107).
    last_command_failed: bool,
    /// The model was already told once that it stopped too early.
    nudged_to_go_on: bool,
}

/// What one exchange with the model's server gave (see `LlmAgent::complete`).
enum Reply {
    /// The assistant's message.
    Message(Value),
    /// The server refused this request in a way that sending it again
    /// unchanged will not cure. With a local server this is how a reply
    /// that was cut off in the middle of a tool call shows up (HTTP 500).
    Unusable(String),
}

enum Tried {
    Done(Reply),
    /// "Not now": worth sending again after a pause. The second value is
    /// how long the server asked us to wait (its `Retry-After` header), if
    /// it said.
    Busy(String, Option<Duration>),
}

/// Pauses between attempts when the server is busy: four attempts in all.
const RETRY_WAITS_MS: [u64; 3] = [500, 2_000, 6_000];
/// When the server names a wait itself (`Retry-After`), that is used in
/// place of the pause above, up to this long. A hosted API with a
/// tokens-per-minute limit asks for up to a minute; a wait longer than this
/// (a daily limit) ends the run as interrupted at once (D-124).
const MAX_SERVER_WAIT: Duration = Duration::from_secs(90);

/// Start of the message that tells the model it stopped too early. Also how
/// `restore` finds out, from the log, that the message was already sent.
const GO_ON_MARK: &str = "[norn] You answered without a tool call";
/// The same mark as written before the rename (D-134), found when an older
/// execution is resumed.
const GO_ON_MARK_OLD: &str = "[agentos] You answered without a tool call";

fn command_failed(result: &ToolResult) -> Option<bool> {
    match &result.output {
        Some(ToolOutput::Exec(e)) => Some(e.exit_code != Some(0)),
        _ => None,
    }
}

impl LlmAgent {
    pub fn new(cfg: LlmConfig) -> LlmAgent {
        let mut a = LlmAgent { cfg, messages: Vec::new(), unlogged: Vec::new(), pending: None, repeats: Default::default(), last_key: None, last_call: None, seen: Default::default(), last_command_failed: false, nudged_to_go_on: false };
        let task = a.cfg.task.clone();
        a.push(json!({"role": "system", "content": SYSTEM_PROMPT}));
        a.push(json!({"role": "user", "content": task}));
        a
    }

    fn push(&mut self, m: Value) {
        self.messages.push(m.clone());
        self.unlogged.push(m);
    }

    /// One request to `/chat/completions`, sent again a few times if the
    /// server says "not now". What comes back is sorted into three kinds,
    /// because they call for three different things (D-111):
    ///
    /// | The server said | Meaning | What happens |
    /// |---|---|---|
    /// | 200 with a message | an answer | `Reply::Message` |
    /// | 429, 502, 503, 504 | busy, restarting, or over a rate limit | wait (as long as the server's `Retry-After` says, if it says; D-124) and send again; if it stays so, an error: the run stops as interrupted and can be resumed later |
    /// | 401, 403, 404 | wrong key, model or address | an error at once, naming the settings |
    /// | any other status | this request will not get a better answer (the model's reply could not be used, or the conversation is too long) | `Reply::Unusable`: the caller tells the model, and gives up cleanly if it goes on |
    /// | no connection | the server is gone | an error: interrupted, resumable |
    fn complete(&self) -> Result<Reply> {
        let mut last = String::new();
        for wait_ms in RETRY_WAITS_MS.iter().copied().map(Some).chain([None]) {
            let asked = match self.complete_once()? {
                Tried::Done(reply) => return Ok(reply),
                Tried::Busy(why, asked) => {
                    last = why;
                    asked
                }
            };
            let Some(ms) = wait_ms else { break };
            match asked {
                Some(wait) if wait > MAX_SERVER_WAIT => {
                    last = format!("{last}; it asked for a wait of {} s, longer than norn waits", wait.as_secs());
                    break;
                }
                // A little longer than asked: the server's clock decides.
                Some(wait) => {
                    let wait = wait + Duration::from_millis(500);
                    if wait >= Duration::from_secs(5) {
                        eprintln!("  the model's server asked for a pause (rate limit): waiting {} s", wait.as_secs());
                    }
                    std::thread::sleep(wait);
                }
                None => std::thread::sleep(Duration::from_millis(ms)),
            }
        }
        bail!(
            "the LLM server keeps answering that it is busy ({last}). The execution is unfinished and nothing is lost: \
             run `norn run --resume <id>` when the server answers again"
        )
    }

    fn complete_once(&self) -> Result<Tried> {
        let url = format!("{}/chat/completions", self.cfg.base_url.trim_end_matches('/'));
        let body = json!({
            "model": self.cfg.model,
            "messages": self.messages,
            "tools": tool_schemas(),
            "temperature": 0,
            // A reply must fit a whole source file, but not run on for ever:
            // a small model once repeated one paragraph until its context was
            // full (Findings, 2026-11-12).
            "max_tokens": MAX_REPLY_TOKENS,
        });
        let agent: ureq::Agent = ureq::Agent::config_builder()
            .timeout_global(Some(Duration::from_secs(600)))
            .http_status_as_error(false)
            .build()
            .into();
        let mut req = agent.post(&url).header("Content-Type", "application/json");
        if let Some(key) = &self.cfg.api_key {
            req = req.header("Authorization", &format!("Bearer {key}"));
        }
        let mut resp = req.send_json(&body).with_context(|| format!("calling the LLM at {url}"))?;
        let status = resp.status().as_u16();
        // `Retry-After: <seconds>` (whole or with a fraction). The other
        // form the header may take, a date, is not used by the APIs this
        // talks to and is ignored.
        let retry_after = resp
            .headers()
            .get("retry-after")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().parse::<f64>().ok())
            .filter(|s| s.is_finite() && *s >= 0.0)
            .map(Duration::from_secs_f64);
        let text = resp.body_mut().read_to_string().context("reading the LLM's reply")?;
        let shown: String = text.chars().take(400).collect();
        Ok(match status {
            200 => {
                let reply: Value = serde_json::from_str(&text).context("the LLM's reply is not JSON")?;
                let message = reply["choices"][0]["message"].as_object().map(|m| Value::Object(m.clone())).context("the LLM's reply has no choices[0].message")?;
                Tried::Done(Reply::Message(message))
            }
            429 | 502 | 503 | 504 => Tried::Busy(format!("HTTP {status}: {shown}"), retry_after),
            401 | 403 | 404 => bail!(
                "the LLM endpoint at {url} answered HTTP {status}: {shown}\n  \
                 That is a wrong key, model name or address, not something the model did. Check NORN_LLM_API_KEY, \
                 NORN_LLM_MODEL (in use: {}) and NORN_LLM_BASE_URL (in use: {}).",
                self.cfg.model,
                self.cfg.base_url
            ),
            _ => Tried::Done(Reply::Unusable(format!("HTTP {status}: {shown}"))),
        })
    }
}

/// A write that drops at least this many previously read lines gets a warning.
const DROPPED_LINES_WARNING: usize = 4;
/// Same call this many times since the last write: add a hint to the result.
const REPEAT_NUDGE: u32 = 2;
/// ... and this many: stop the run instead of using up the call budget.
const REPEAT_STOP: u32 = 5;

impl LlmAgent {
    /// Count a call. A write changes the workspace, so earlier reads may
    /// rightly be repeated after it: the counts start again.
    fn note_call(&mut self, call: &ToolCall) {
        if matches!(call, ToolCall::FsWrite { .. } | ToolCall::FsEdit { .. }) {
            self.repeats.clear();
            self.last_key = None;
            return;
        }
        let key = serde_json::to_string(call).unwrap_or_default();
        *self.repeats.entry(key.clone()).or_insert(0) += 1;
        self.last_key = Some(key);
    }

    /// Remember what each file contains (from reads and writes). When a
    /// write drops several lines the model had seen in that file, return a
    /// warning to add to the tool result. `fs.write` replaces whole files,
    /// and sending only the changed part is a common model mistake.
    fn track_file_content(&mut self, result: &ToolResult) -> Option<String> {
        if result.outcome != ToolOutcome::Ok {
            return None;
        }
        match (self.last_call.take()?, &result.output) {
            (ToolCall::FsRead { path, .. }, Some(ToolOutput::Read { content, .. })) => {
                self.seen.insert(path, content.clone());
                None
            }
            // Keep the remembered text in step with the file, so a later
            // fs_write of it is compared with what is really there.
            (ToolCall::FsEdit { path, old_str, new_str }, _) => {
                if let Some(text) = self.seen.get_mut(&path) {
                    *text = text.replacen(&old_str, &new_str, 1);
                }
                None
            }
            // Base64 content is not lines of text; nothing to compare.
            (ToolCall::FsWrite { encoding: Some(_), .. }, _) => None,
            (ToolCall::FsWrite { path, content, .. }, _) => {
                let old = self.seen.insert(path.clone(), content.clone())?;
                let kept: std::collections::HashSet<&str> = content.lines().map(str::trim).collect();
                let dropped: Vec<&str> = old.lines().map(str::trim).filter(|l| !l.is_empty() && !kept.contains(l)).collect();
                if dropped.len() < DROPPED_LINES_WARNING {
                    return None;
                }
                let examples: Vec<String> = dropped.iter().take(4).map(|l| format!("`{l}`")).collect();
                Some(format!(
                    "\n[norn] Warning: {} lines that were in {path} when you read it are not in what you wrote, for example {}. \
                     If you did not mean to delete them, write the file again with ALL of its lines.",
                    dropped.len(),
                    examples.join(", ")
                ))
            }
            _ => None,
        }
    }

    /// How often the call that was just answered has been made.
    fn repeat_count(&self) -> Option<u32> {
        self.last_key.as_ref().and_then(|k| self.repeats.get(k)).copied()
    }
}

impl Agent for LlmAgent {
    fn next_action(&mut self, last: Option<&ToolResult>) -> Result<Action> {
        // Answer the model's open tool call with the gateway's result.
        if let (Some(result), Some(id)) = (last, self.pending.take()) {
            if let Some(failed) = command_failed(result) {
                self.last_command_failed = failed;
            }
            let mut content = render_result(result);
            if let Some(warning) = self.track_file_content(result) {
                content.push_str(&warning);
            }
            if let Some(n) = self.repeat_count() {
                if n >= REPEAT_STOP {
                    self.push(json!({"role": "tool", "tool_call_id": id, "content": content}));
                    return Ok(Action::GiveUp { reason: format!("the model made the same call {n} times without changing anything") });
                }
                if n >= REPEAT_NUDGE {
                    content.push_str(&format!(
                        "\n[norn] You have made this exact call {n} times and the result is the same. \
                         Repeating it will not help. Read the file you changed again and compare it with what the error says."
                    ));
                }
            }
            self.push(json!({"role": "tool", "tool_call_id": id, "content": content}));
        }
        // The model may answer with a malformed call; tell it and ask again,
        // a few times at most.
        let mut unusable: Option<String> = None;
        for _ in 0..3 {
            let msg = match self.complete()? {
                Reply::Message(msg) => msg,
                Reply::Unusable(why) => {
                    // Nothing came back that can be stored as the model's
                    // message. Say so in the conversation and ask again;
                    // the loop's limit of three covers this too.
                    unusable = Some(why.clone());
                    self.push(json!({"role": "user", "content": format!(
                        "[norn] Your last reply could not be used; the server said: {why}\n\
                         A reply that is too long gets cut off. Answer again with ONE tool call and keep it short. \
                         If a file is too long to send whole, say GIVE UP and why."
                    )}));
                    continue;
                }
            };
            let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
            let Some(first) = calls.first() else {
                let text = msg["content"].as_str().unwrap_or("").trim().to_string();
                self.push(json!({"role": "assistant", "content": text}));
                let mut summary: String = text.chars().take(MAX_SUMMARY_CHARS).collect();
                if text.chars().count() > MAX_SUMMARY_CHARS {
                    summary.push_str(" [...]");
                }
                // Words instead of a tool call normally mean "done". But if
                // the last command the model ran failed, it is not done: it
                // is told so once, and if it stops again the run is recorded
                // as stopped, not as completed (D-107).
                if self.last_command_failed {
                    if self.nudged_to_go_on {
                        return Ok(Action::GiveUp { reason: format!("the model stopped while the last command it ran was still failing. Its last words: {summary}") });
                    }
                    self.nudged_to_go_on = true;
                    self.push(json!({"role": "user", "content": format!(
                        "{GO_ON_MARK}, but the last command you ran did not succeed, so the task is not finished. \
                         Go on with a tool call: read the error, change the code, run the command again. \
                         If you cannot finish the task, answer with one sentence that starts with GIVE UP."
                    )}));
                    continue;
                }
                return Ok(Action::Finish { summary });
            };
            // Keep exactly one tool call in the stored message, so the
            // conversation has one result per call.
            let id = first["id"].as_str().unwrap_or("call_0").to_string();
            let name = first["function"]["name"].as_str().unwrap_or("").to_string();
            let args = first["function"]["arguments"].clone();
            let args_text = args.as_str().map(str::to_string).unwrap_or_else(|| args.to_string());
            self.push(json!({
                "role": "assistant",
                "content": msg["content"].as_str().unwrap_or(""),
                "tool_calls": [{"id": id, "type": "function", "function": {"name": name, "arguments": args_text}}],
            }));
            match parse_call(&name, &args_text) {
                Ok(call) => {
                    self.note_call(&call);
                    self.last_call = Some(call.clone());
                    self.pending = Some(id);
                    return Ok(Action::Call(call));
                }
                Err(e) => self.push(json!({"role": "tool", "tool_call_id": id, "content": format!("invalid tool call: {e}")})),
            }
        }
        Ok(Action::GiveUp {
            reason: match unusable {
                Some(why) => format!("the model's replies could not be used three times in a row; the server's last answer: {why}"),
                None => "the model sent three malformed tool calls in a row".into(),
            },
        })
    }

    fn kind(&self) -> &'static str {
        "llm"
    }

    fn task(&self) -> String {
        self.cfg.task.clone()
    }

    fn take_messages(&mut self) -> Vec<Value> {
        std::mem::take(&mut self.unlogged)
    }

    fn persist(&self, dir: &ExecDir) -> Result<()> {
        std::fs::write(dir.llm_config(), serde_json::to_string_pretty(&self.cfg)?)?;
        Ok(())
    }

    /// Rebuild the conversation from the `agent.message` events (ADR 0005).
    ///
    /// Three situations, told apart by how the stored conversation ends:
    /// - an assistant message with a tool call that became a gateway call:
    ///   the model is waiting for that call's result, which the runtime
    ///   passes to `next_action` (for a cut-off call, recovery's verdict);
    /// - an assistant message with a tool call that never reached the
    ///   gateway (the crash came first): drop it, the model is asked again;
    /// - a tool result: the request to the model was cut off; ask again.
    ///
    /// A cut-off request to the model is simply sent again, and the answer
    /// may differ (D-017).
    fn restore(&mut self, history: &[ToolResult], events: &[Event]) -> Result<()> {
        self.messages = events
            .iter()
            .filter_map(|e| match &e.body {
                EventBody::AgentMessage { message } => Some(message.clone()),
                _ => None,
            })
            .collect();
        self.unlogged.clear();
        self.pending = None;
        self.last_command_failed = history.iter().rev().find_map(command_failed).unwrap_or(false);
        self.nudged_to_go_on = self.messages.iter().any(|m| m["role"] == "user" && m["content"].as_str().is_some_and(|c| c.starts_with(GO_ON_MARK) || c.starts_with(GO_ON_MARK_OLD)));
        let made = self.messages.iter().filter(|m| m["tool_calls"].is_array() && is_valid_call(m)).count();
        let last_is_call = self.messages.last().map(|m| m["tool_calls"].is_array()).unwrap_or(false);
        if last_is_call {
            if made == history.len() {
                self.pending = self.messages.last().and_then(|m| m["tool_calls"][0]["id"].as_str()).map(str::to_string);
            } else {
                // Logged, but the gateway never saw it. The log keeps the old
                // message (it is append-only); the conversation continues
                // without it. The dropped message is noted for the trace.
                self.messages.pop();
            }
        }
        Ok(())
    }
}

fn is_valid_call(m: &Value) -> bool {
    let f = &m["tool_calls"][0]["function"];
    parse_call(f["name"].as_str().unwrap_or(""), f["arguments"].as_str().unwrap_or("")).is_ok()
}

/// The four tools, described for the model. Names use `_` because the API
/// does not allow `.` in function names.
pub(crate) fn tool_schemas() -> Value {
    let f = |name: &str, description: &str, props: Value, required: &[&str]| {
        json!({"type": "function", "function": {"name": name, "description": description,
            "parameters": {"type": "object", "properties": props, "required": required}}})
    };
    let path = json!({"type": "string", "description": "Path relative to the repository root"});
    json!([
        f("fs_list", "List the entries of a folder.", json!({"path": path}), &["path"]),
        f("fs_read", "Read a text file. A file that is too large is refused unless you ask for part of it with start_line and max_lines.",
          json!({"path": path,
                 "start_line": {"type": "integer", "description": "Optional. First line to return, counting from 1"},
                 "max_lines": {"type": "integer", "description": "Optional. How many lines to return"},
                 "encoding": {"type": "string", "enum": ["base64"], "description": "Optional. Only for a file that is not text: returns its bytes as base64"}}), &["path"]),
        f("fs_edit", "Change part of an existing text file: replace old_str with new_str. old_str must be in the file exactly once, character for character. Use this, not fs_write, to change a file that exists.",
          json!({"path": path,
                 "old_str": {"type": "string", "description": "The exact text to replace, copied from the file; include enough surrounding lines that it occurs only once"},
                 "new_str": {"type": "string", "description": "The text that takes its place"}}), &["path", "old_str", "new_str"]),
        f("fs_write", "Replace a file with new content (the whole file). A new file may be in a folder that does not exist yet; the folder is made.",
          json!({"path": path, "content": {"type": "string", "description": "The complete new content of the file"},
                 "executable": {"type": "boolean", "description": "Optional. true makes the file runnable (a script); leave it out to keep the file as it is"},
                 "encoding": {"type": "string", "enum": ["base64"], "description": "Optional. Only for a binary file: content is then base64"}}), &["path", "content"]),
        f("exec", "Run an allowed command, given as a list of arguments (no shell). Example: [\"pytest\", \"-q\"].",
          json!({"argv": {"type": "array", "items": {"type": "string"}}}), &["argv"]),
    ])
}

pub(crate) fn parse_call(name: &str, args: &str) -> Result<ToolCall> {
    #[derive(Deserialize)]
    struct Args {
        path: Option<String>,
        content: Option<String>,
        old_str: Option<String>,
        new_str: Option<String>,
        encoding: Option<String>,
        executable: Option<bool>,
        argv: Option<Vec<String>>,
        start_line: Option<u64>,
        max_lines: Option<u64>,
    }
    let a: Args = serde_json::from_str(if args.trim().is_empty() { "{}" } else { args }).context("arguments are not a JSON object")?;
    let path = || a.path.clone().context("missing `path`");
    Ok(match name {
        "fs_list" => ToolCall::FsList { path: path()? },
        "fs_read" => ToolCall::FsRead { path: path()?, start_line: a.start_line, max_lines: a.max_lines, encoding: a.encoding.clone() },
        "fs_write" => ToolCall::FsWrite {
            path: path()?,
            content: a.content.clone().context("missing `content`")?,
            encoding: a.encoding.clone(),
            executable: a.executable,
        },
        "fs_edit" => ToolCall::FsEdit {
            path: path()?,
            old_str: a.old_str.clone().context("missing `old_str`")?,
            new_str: a.new_str.clone().context("missing `new_str`")?,
        },
        "exec" => ToolCall::Exec { argv: a.argv.clone().context("missing `argv`")? },
        other => bail!("unknown tool `{other}`; the tools are fs_list, fs_read, fs_edit, fs_write, exec"),
    })
}

/// A tool result as the text the model sees.
pub(crate) fn render_result(r: &ToolResult) -> String {
    let text = match (&r.outcome, &r.output) {
        (ToolOutcome::Denied { rule }, _) => format!("DENIED by policy: {rule}. Nothing was done."),
        (ToolOutcome::Failed { error }, _) => format!("FAILED: {error}. Nothing was changed."),
        (ToolOutcome::NotApplied { reason }, _) => {
            format!("NOT APPLIED: the runtime was restarted while this call was in flight. {reason}. If you still need it, call the tool again.")
        }
        (ToolOutcome::Ok, Some(ToolOutput::Read { content, encoding: Some(e), .. })) => format!("[the file's bytes as {e}]\n{content}"),
        (ToolOutcome::Ok, Some(ToolOutput::Read { content, lines: None, .. })) => content.clone(),
        (ToolOutcome::Ok, Some(ToolOutput::Read { content, lines: Some(l), .. })) => {
            format!("[lines {}-{} of {}; this is only part of the file]\n{content}", l.first, l.last, l.total)
        }
        (ToolOutcome::Ok, Some(ToolOutput::List { entries })) => {
            entries.iter().map(|e| format!("{}{}", e.name, if e.kind == "dir" { "/" } else { "" })).collect::<Vec<_>>().join("\n")
        }
        (ToolOutcome::Ok, Some(ToolOutput::Write { bytes })) => format!(
            "OK: the file now holds exactly the {bytes} bytes you sent. Everything that was in it before and that you did not send is gone."
        ),
        (ToolOutcome::Ok, Some(ToolOutput::Edit { bytes, line })) => {
            format!("OK: replaced the text that began at line {line}. The rest of the file is unchanged; it is now {bytes} bytes.")
        }
        (ToolOutcome::Ok, Some(ToolOutput::Exec(e))) => {
            let mut s = format!("{}\n", ToolOutput::Exec(e.clone()).summary());
            if !e.stdout.is_empty() {
                s.push_str(&format!("--- stdout ---\n{}\n", e.stdout));
            }
            if !e.stderr.is_empty() {
                s.push_str(&format!("--- stderr ---\n{}\n", e.stderr));
            }
            if !e.reverted.is_empty() {
                s.push_str(&format!("The command changed paths outside the write rules; they were put back: {}\n", e.reverted.join(", ")));
            }
            s
        }
        // The LLM is not offered `ext.call` (D-073); this arm only keeps the
        // match complete.
        (ToolOutcome::Ok, Some(ToolOutput::External { status, body, .. })) => format!("HTTP {status}\n{body}"),
        (ToolOutcome::Ok, None) => "OK: the call was applied (the runtime was restarted; its output was not kept).".into(),
    };
    if text.chars().count() > MAX_RESULT_CHARS {
        let cut: String = text.chars().take(MAX_RESULT_CHARS).collect();
        format!("{cut}\n[... cut at {MAX_RESULT_CHARS} characters]")
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_calls_are_parsed_strictly() {
        assert_eq!(parse_call("fs_read", r#"{"path":"a.py"}"#).unwrap(), ToolCall::FsRead { path: "a.py".into(), start_line: None, max_lines: None, encoding: None });
        assert_eq!(
            parse_call("fs_read", r#"{"path":"big.csv","start_line":100,"max_lines":50}"#).unwrap(),
            ToolCall::FsRead { path: "big.csv".into(), start_line: Some(100), max_lines: Some(50), encoding: None }
        );
        assert_eq!(
            parse_call("exec", r#"{"argv":["pytest","-q"]}"#).unwrap(),
            ToolCall::Exec { argv: vec!["pytest".into(), "-q".into()] }
        );
        assert!(parse_call("fs_write", r#"{"path":"a.py"}"#).is_err(), "content is required");
        assert_eq!(
            parse_call("fs_write", r#"{"path":"run.sh","content":"IyEvYmluL3NoCg==","encoding":"base64","executable":true}"#).unwrap(),
            ToolCall::FsWrite { path: "run.sh".into(), content: "IyEvYmluL3NoCg==".into(), encoding: Some("base64".into()), executable: Some(true) }
        );
        assert_eq!(
            parse_call("fs_read", r#"{"path":"logo.png","encoding":"base64"}"#).unwrap(),
            ToolCall::FsRead { path: "logo.png".into(), start_line: None, max_lines: None, encoding: Some("base64".into()) }
        );
        // A script step or an old log without the new fields still reads.
        assert_eq!(
            serde_json::from_str::<ToolCall>(r#"{"tool":"fs.write","path":"a","content":"x"}"#).unwrap(),
            ToolCall::FsWrite { path: "a".into(), content: "x".into(), encoding: None, executable: None }
        );
        assert!(parse_call("shell", r#"{"cmd":"rm -rf /"}"#).is_err(), "unknown tools are refused");
        assert!(parse_call("exec", "not json").is_err());
    }

    #[test]
    fn results_are_rendered_for_the_model() {
        let r = ToolResult { call_id: 1, outcome: ToolOutcome::Denied { rule: "filesystem.deny[0] \".env\"".into() }, output: None };
        assert!(render_result(&r).starts_with("DENIED by policy"));
        let r = ToolResult { call_id: 1, outcome: ToolOutcome::NotApplied { reason: "rolled back".into() }, output: None };
        assert!(render_result(&r).contains("call the tool again"));
        let long = ToolResult { call_id: 1, outcome: ToolOutcome::Ok, output: Some(ToolOutput::Read { content: "x".repeat(50_000), lines: None, encoding: None }) };
        assert!(render_result(&long).len() < 13_000);
    }

    fn agent() -> LlmAgent {
        LlmAgent::new(LlmConfig { base_url: "http://unused".into(), model: "m".into(), task: "t".into(), api_key: None })
    }

    #[test]
    fn a_write_that_drops_lines_gets_a_warning() {
        let mut a = agent();
        let old = "def mean(v):\n    return 1\n\ndef median(v):\n    x = sorted(v)\n    return x[0]\n\ndef mode(v):\n    return v[0]\n";
        a.last_call = Some(ToolCall::FsRead { path: "s.py".into(), start_line: None, max_lines: None, encoding: None });
        let read = ToolResult { call_id: 1, outcome: ToolOutcome::Ok, output: Some(ToolOutput::Read { content: old.into(), lines: None, encoding: None }) };
        assert_eq!(a.track_file_content(&read), None);
        // Only one function sent back: five lines are gone.
        a.last_call = Some(ToolCall::FsWrite { path: "s.py".into(), content: "def mean(v):\n    return 2\n".into(), encoding: None, executable: None });
        let wrote = ToolResult { call_id: 2, outcome: ToolOutcome::Ok, output: Some(ToolOutput::Write { bytes: 10 }) };
        let w = a.track_file_content(&wrote).expect("a warning");
        assert!(w.contains("`def median(v):`"), "{w}");
        // A one-line fix of the whole file: no warning.
        a.seen.insert("s.py".into(), old.into());
        a.last_call = Some(ToolCall::FsWrite { path: "s.py".into(), content: old.replace("return 1", "return 2"), encoding: None, executable: None });
        assert_eq!(a.track_file_content(&wrote), None);
        // A file never read: nothing to compare with.
        a.last_call = Some(ToolCall::FsWrite { path: "new.py".into(), content: "x = 1\n".into(), encoding: None, executable: None });
        assert_eq!(a.track_file_content(&wrote), None);
    }

    #[test]
    fn repeated_calls_are_counted_until_a_write() {
        let mut a = agent();
        let list = ToolCall::FsList { path: "src".into() };
        for n in 1..=3 {
            a.note_call(&list);
            assert_eq!(a.repeat_count(), Some(n));
        }
        a.note_call(&ToolCall::FsWrite { path: "a".into(), content: "x".into(), encoding: None, executable: None });
        assert_eq!(a.repeat_count(), None, "a write starts the counts again");
        a.note_call(&list);
        assert_eq!(a.repeat_count(), Some(1));
    }

    #[test]
    fn the_api_key_is_never_serialized() {
        let cfg = LlmConfig { base_url: "u".into(), model: "m".into(), task: "t".into(), api_key: Some("SECRET".into()) };
        assert!(!serde_json::to_string(&cfg).unwrap().contains("SECRET"));
    }
}
