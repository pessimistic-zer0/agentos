//! `norn mcp`: norn as an MCP server over standard input and output, so that
//! an agent norn did not write (Claude Code, Cursor, …) runs its file and
//! command calls through the gateway (v2, D-139).
//!
//! The client is the agent. Each `tools/call` becomes one `ToolCall` handed to
//! the same gateway as `norn run` uses: policy, intent, effect, checkpoint,
//! completion record. Nothing about durability is new here; this module only
//! translates messages.
//!
//! Three facts from the experiments shape it (`experiments/m15-mcp/NOTES.md`):
//!
//! - Two protocol revisions are served (D-140). A request that carries the
//!   2026-07-28 `_meta` fields is answered by those rules; a session that
//!   began with `initialize` is answered by the 2025-11-25 rules.
//! - A client stops the server with SIGINT and kills it half a second later,
//!   and starts it again by itself if it died (D-141). So a stop is only a
//!   stop (the handler of D-106), the execution is opened at the first tool
//!   call, and an unfinished one of the same repository is continued.
//! - Tool names on the wire use `_` (D-142).

use std::fs::File;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};

use crate::agent::{Action, Agent};
use crate::gateway::Gateway;
use crate::log::{EventBody, EventLog};
use crate::policy::Policy;
use crate::runtime::{self, RunOptions};
use crate::tools::{ToolCall, ToolOutcome, ToolResult};
use crate::workspace::ExecDir;

/// The revision whose rules apply when a request carries `_meta`.
pub const MODERN: &str = "2026-07-28";
/// Revisions served to a client that starts with `initialize`. The first is
/// the answer to a version norn does not know (the specification says to
/// answer with one the server supports).
const LEGACY: [&str; 3] = ["2025-11-25", "2025-06-18", "2025-03-26"];
/// A message longer than this is refused without being read into memory.
/// It is far above any file the policies allow to be written.
pub const MAX_MESSAGE: usize = 32 * 1024 * 1024;

const META_VERSION: &str = "io.modelcontextprotocol/protocolVersion";
const META_CAPS: &str = "io.modelcontextprotocol/clientCapabilities";
const META_CLIENT: &str = "io.modelcontextprotocol/clientInfo";

// JSON-RPC error codes. -32022 is the specification's "unsupported protocol
// version" (2026-07-28).
const PARSE_ERROR: i64 = -32700;
const INVALID_REQUEST: i64 = -32600;
const METHOD_NOT_FOUND: i64 = -32601;
const INVALID_PARAMS: i64 = -32602;
const INTERNAL_ERROR: i64 = -32603;
const UNSUPPORTED_VERSION: i64 = -32022;

const INSTRUCTIONS: &str = "These tools work on a copy of one repository, inside a sandbox, under a policy. \
Paths are relative to the repository root. Commands are lists of arguments run without a shell, and only those the policy allows; \
there is no network. A refused call did nothing and says why. The original repository is never changed: when the work is done, \
the person takes the result with `norn finish` and `norn apply`.";

/// What `norn mcp` was started with.
pub struct Config {
    pub state_dir: PathBuf,
    /// Canonical path of the repository.
    pub repo: PathBuf,
    pub policy: PathBuf,
}

/// The execution a session works on, once the first tool call opened it.
struct Open {
    gw: Gateway,
    /// Held for as long as the server runs (see `runtime::lock_execution`).
    _lock: File,
    /// Calls in this execution so far, for `limits.max_tool_calls`.
    calls: u64,
    /// What each command failed before the first file change (D-145).
    baseline: crate::baseline::Baseline,
}

pub struct Server {
    cfg: Config,
    policy: Policy,
    /// A session that began with `initialize`, and the version agreed then.
    legacy: Option<String>,
    /// Who the client said it is, for the log.
    client: Value,
    open: Option<Open>,
}

/// One line read from the client.
pub enum Line {
    Text(Vec<u8>),
    TooLarge,
    Eof,
}

/// Read one newline-ended message, but never more than `max` bytes of it
/// into memory: a longer one is skipped to its end and reported as such.
pub fn read_line(r: &mut impl BufRead, max: usize) -> io::Result<Line> {
    let mut buf = Vec::new();
    let mut too_large = false;
    loop {
        let (used, done) = {
            let avail = r.fill_buf()?;
            if avail.is_empty() {
                return Ok(if too_large {
                    Line::TooLarge
                } else if buf.is_empty() {
                    Line::Eof
                } else {
                    Line::Text(buf)
                });
            }
            let end = avail.iter().position(|&b| b == b'\n');
            let chunk = &avail[..end.unwrap_or(avail.len())];
            if !too_large {
                if buf.len() + chunk.len() > max {
                    too_large = true;
                    buf = Vec::new();
                } else {
                    buf.extend_from_slice(chunk);
                }
            }
            (end.map(|i| i + 1).unwrap_or(avail.len()), end.is_some())
        };
        r.consume(used);
        if done {
            return Ok(if too_large { Line::TooLarge } else { Line::Text(buf) });
        }
    }
}

impl Server {
    pub fn new(cfg: Config) -> Result<Server> {
        let (policy, _) = Policy::load(&cfg.policy)?;
        Ok(Server { cfg, policy, legacy: None, client: Value::Null, open: None })
    }

    /// Answer messages until the client goes away. The execution is left
    /// unfinished on purpose (D-141): `norn finish` ends it.
    pub fn serve(&mut self, input: &mut impl BufRead, out: &mut impl Write) -> Result<()> {
        loop {
            let reply = match read_line(input, MAX_MESSAGE)? {
                Line::Eof => return Ok(()),
                Line::TooLarge => Some(error(Value::Null, INVALID_REQUEST, &format!("message larger than {} MiB", MAX_MESSAGE / (1024 * 1024)), None)),
                Line::Text(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => None,
                Line::Text(bytes) => match serde_json::from_slice::<Value>(&bytes) {
                    Err(e) => Some(error(Value::Null, PARSE_ERROR, &format!("not JSON: {e}"), None)),
                    Ok(msg) => {
                        let id = msg.get("id").cloned().unwrap_or(Value::Null);
                        match self.handle(msg) {
                            Ok(reply) => reply,
                            // The runtime failed (the log could not be
                            // written): answer the call, then stop. The next
                            // start recovers it, like after a crash.
                            Err(e) => {
                                let text = format!("norn stopped: {e:#}. The call is settled when the server starts again.");
                                writeln!(out, "{}", error(id, INTERNAL_ERROR, &text, None))?;
                                out.flush()?;
                                return Err(e);
                            }
                        }
                    }
                },
            };
            if let Some(reply) = reply {
                // One message per line: serde_json never writes a raw newline
                // inside a value, so the line cannot be split.
                writeln!(out, "{reply}")?;
                out.flush()?;
            }
        }
    }

    /// One message in, at most one answer out. `Err` only when the runtime
    /// itself failed (the log could not be written): the server then stops,
    /// and the next start recovers.
    pub fn handle(&mut self, msg: Value) -> Result<Option<Value>> {
        let Some(obj) = msg.as_object() else {
            return Ok(Some(error(Value::Null, INVALID_REQUEST, "a message must be a JSON object (batches are not part of MCP)", None)));
        };
        let id = obj.get("id").cloned();
        let Some(method) = obj.get("method").and_then(Value::as_str) else {
            // An answer to a request: norn sends none, so there is nothing
            // to match it with.
            return Ok(None);
        };
        // A notification (no id) is never answered. The ones a client sends
        // (`notifications/initialized`, `notifications/cancelled`) need no
        // action: calls are handled one at a time, so a cancellation always
        // refers to a call that has already been answered.
        let Some(id) = id else { return Ok(None) };
        if !(id.is_string() || id.is_i64() || id.is_u64()) {
            return Ok(Some(error(Value::Null, INVALID_REQUEST, "the id must be a string or an integer", None)));
        }
        let params = obj.get("params").cloned().unwrap_or(json!({}));

        if method == "initialize" {
            return Ok(Some(self.initialize(id, &params)));
        }
        // Which rules apply to this request: the new ones if it carries the
        // new `_meta`, the old ones if this session began with `initialize`.
        let modern = params.get("_meta").and_then(|m| m.get(META_VERSION)).is_some() || self.legacy.is_none();
        if modern {
            if let Err(e) = self.check_meta(&id, &params) {
                return Ok(Some(e));
            }
        }
        let result = match method {
            "server/discover" if modern => self.discover(&params),
            "ping" if !modern => json!({}),
            "tools/list" => self.tools_list(modern),
            "tools/call" => match self.tools_call(&params)? {
                Ok(r) => r,
                Err((code, text)) => return Ok(Some(error(id, code, &text, None))),
            },
            other => return Ok(Some(error(id, METHOD_NOT_FOUND, &format!("method not found: {other}"), None))),
        };
        let mut result = result;
        if modern {
            result["resultType"] = json!("complete");
        }
        Ok(Some(json!({"jsonrpc": "2.0", "id": id, "result": result})))
    }

    fn initialize(&mut self, id: Value, params: &Value) -> Value {
        let asked = params.get("protocolVersion").and_then(Value::as_str).unwrap_or("");
        let version = if LEGACY.contains(&asked) { asked } else { LEGACY[0] };
        self.legacy = Some(version.to_string());
        self.client = params.get("clientInfo").cloned().unwrap_or(Value::Null);
        json!({"jsonrpc": "2.0", "id": id, "result": {
            "protocolVersion": version,
            "capabilities": {"tools": {}},
            "serverInfo": server_info(),
            "instructions": INSTRUCTIONS,
        }})
    }

    /// 2026-07-28: every request names its version and the client's
    /// capabilities; a request without them is malformed (-32602), and an
    /// unknown version is refused with the list of known ones.
    fn check_meta(&mut self, id: &Value, params: &Value) -> std::result::Result<(), Value> {
        let meta = params.get("_meta");
        let version = meta.and_then(|m| m.get(META_VERSION)).and_then(Value::as_str);
        let caps = meta.and_then(|m| m.get(META_CAPS));
        let Some(version) = version.filter(|_| caps.is_some_and(Value::is_object)) else {
            return Err(error(
                id.clone(),
                INVALID_PARAMS,
                &format!("params._meta must carry \"{META_VERSION}\" and \"{META_CAPS}\" (or start the session with initialize)"),
                None,
            ));
        };
        if version != MODERN {
            return Err(error(
                id.clone(),
                UNSUPPORTED_VERSION,
                "Unsupported protocol version",
                Some(json!({"supported": [MODERN], "requested": version})),
            ));
        }
        if let Some(client) = meta.and_then(|m| m.get(META_CLIENT)) {
            self.client = client.clone();
        }
        Ok(())
    }

    fn discover(&self, _params: &Value) -> Value {
        json!({
            "supportedVersions": [MODERN],
            "capabilities": {"tools": {}},
            "_meta": {"io.modelcontextprotocol/serverInfo": server_info()},
            "instructions": INSTRUCTIONS,
            // The tool list cannot change while this process runs, but a
            // new process may read a changed policy; 0 says "may fetch again
            // whenever needed".
            "ttlMs": 0,
            "cacheScope": "private",
        })
    }

    fn tools_list(&self, modern: bool) -> Value {
        let mut r = json!({"tools": tools(&self.policy)});
        if modern {
            r["ttlMs"] = json!(0);
            r["cacheScope"] = json!("private");
        }
        r
    }

    /// `Ok(Ok(result))`: an answer, a refused call included (`isError`).
    /// `Ok(Err(..))`: a JSON-RPC error (an unknown tool). `Err`: the runtime
    /// failed.
    #[allow(clippy::type_complexity)]
    fn tools_call(&mut self, params: &Value) -> Result<std::result::Result<Value, (i64, String)>> {
        let name = params.get("name").and_then(Value::as_str).unwrap_or("");
        let args = params.get("arguments").cloned().unwrap_or(json!({}));
        if !tools(&self.policy).iter().any(|t| t["name"] == name) {
            let known: Vec<String> = tools(&self.policy).iter().filter_map(|t| t["name"].as_str().map(str::to_string)).collect();
            return Ok(Err((INVALID_PARAMS, format!("Unknown tool: {name}. The tools are {}", known.join(", ")))));
        }
        // A wrong argument is the model's mistake to correct, so it is a
        // tool error it can read, not a protocol error.
        let call = match parse(name, &args) {
            Ok(call) => call,
            Err(e) => return Ok(Ok(tool_result(&format!("FAILED: {e:#}. Nothing was done."), true))),
        };
        let open = match self.open() {
            Ok(open) => open,
            // The sandbox or the copy could not be made: say why, to the
            // model and to the person reading the client's log.
            Err(e) => {
                eprintln!("norn mcp: {e:#}");
                return Ok(Ok(tool_result(&format!("FAILED: norn could not start: {e:#}"), true)));
            }
        };
        let max = open.gw.policy.limits.max_tool_calls;
        if open.calls >= max {
            return Ok(Ok(tool_result(
                &format!("REFUSED: this execution has used its budget of {max} tool calls (limits.max_tool_calls). Ask the person to finish it with `norn finish {}`.", open.gw.dir.id),
                true,
            )));
        }
        let result = open.gw.handle(call.clone()).context("the gateway failed")?;
        open.calls += 1;
        let is_error = !matches!(result.outcome, ToolOutcome::Ok);
        let mut text = crate::agent::llm::render_result(&result);
        if let (ToolCall::Exec { argv }, Some(crate::tools::ToolOutput::Exec(e))) = (&call, &result.output) {
            if let Some(c) = open.baseline.compare(argv, e) {
                text.push_str(&c.note(argv));
            }
        }
        open.baseline.observe(&call, &result);
        Ok(Ok(tool_result(&text, is_error)))
    }

    /// The execution this session works on: opened at the first tool call,
    /// so a client that only starts the server and lists its tools (Claude
    /// Code starts every configured server) copies nothing. An unfinished
    /// execution of `norn mcp` on the same repository is continued (D-141).
    fn open(&mut self) -> Result<&mut Open> {
        if self.open.is_none() {
            let opened = self.continue_or_create()?;
            self.open = Some(opened);
            let open = self.open.as_mut().expect("just set");
            // What the model was offered in this session, for the log
            // (spec §9.10: tool descriptions are part of what it is told).
            let session = json!({"mcp_session": {
                "client": self.client,
                "protocol": self.legacy.clone().unwrap_or_else(|| MODERN.to_string()),
                "tools": tools(&self.policy),
            }});
            open.gw.log.append(EventBody::AgentMessage { message: session })?;
        }
        Ok(self.open.as_mut().expect("opened above"))
    }

    fn continue_or_create(&self) -> Result<Open> {
        for id in runtime::unfinished_mcp_executions(&self.cfg.state_dir, &self.cfg.repo)? {
            match runtime::reopen(&self.cfg.state_dir, &id) {
                Ok((gw, lock, report)) => {
                    crate::interrupt::serving(&id);
                    let calls = calls_so_far(&gw.dir)?;
                    eprintln!("norn mcp: continuing execution {id} ({calls} calls so far). `norn finish {id}` ends it.");
                    if !report.clean {
                        for v in &report.verdicts {
                            let verdict = if v.applied { "applied" } else { "not applied" };
                            eprintln!("norn mcp: call {} ({}) was cut off by the last stop: {verdict}, {}", v.call_id, v.tool, v.reason);
                        }
                    }
                    let events = gw.log.events()?;
                    let history = crate::recovery::history(&events, &crate::blob::BlobStore::new(&gw.dir.blobs()))?;
                    let baseline = crate::baseline::Baseline::from_log(&events, &history);
                    return Ok(Open { gw, _lock: lock, calls, baseline });
                }
                // Another server holds it (a second client on the same
                // repository): leave it to that one.
                Err(e) => eprintln!("norn mcp: not continuing {id}: {e:#}"),
            }
        }
        let opts = RunOptions { state_dir: self.cfg.state_dir.clone(), repo: self.cfg.repo.clone(), policy: self.cfg.policy.clone(), verbose: false };
        let client = McpClient { name: self.client.get("name").and_then(Value::as_str).unwrap_or("an MCP client").to_string() };
        let (gw, lock) = runtime::create(&opts, &client)?;
        crate::interrupt::serving(&gw.dir.id);
        eprintln!("norn mcp: new execution {} on {}. `norn finish {}` ends it.", gw.dir.id, self.cfg.repo.display(), gw.dir.id);
        Ok(Open { gw, _lock: lock, calls: 0, baseline: Default::default() })
    }
}

fn calls_so_far(dir: &ExecDir) -> Result<u64> {
    Ok(crate::log::calls(&EventLog::open(&dir.log())?.events()?)?.len() as u64)
}

fn server_info() -> Value {
    json!({"name": "norn", "version": env!("CARGO_PKG_VERSION")})
}

fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut e = json!({"code": code, "message": message});
    if let Some(data) = data {
        e["data"] = data;
    }
    json!({"jsonrpc": "2.0", "id": id, "error": e})
}

fn tool_result(text: &str, is_error: bool) -> Value {
    json!({"content": [{"type": "text", "text": text}], "isError": is_error})
}

/// The tools offered: the file tools always (the policy decides each path),
/// `exec` if the policy allows any command, `ext_call` if it names a service.
pub fn tools(policy: &Policy) -> Vec<Value> {
    let mut list: Vec<Value> = crate::agent::llm::tool_schemas()
        .as_array()
        .expect("tool_schemas is a list")
        .iter()
        .map(|t| {
            let f = &t["function"];
            json!({"name": f["name"], "description": f["description"], "inputSchema": f["parameters"]})
        })
        .filter(|t| t["name"] != "exec" || !policy.exec.allow.is_empty())
        .collect();
    if !policy.external.services.is_empty() {
        let names: Vec<&str> = policy.external.services.iter().map(|s| s.name.as_str()).collect();
        list.push(json!({
            "name": "ext_call",
            "description": "Send a text payload to an outside service named in the policy. The request carries an idempotency key, so a call that is repeated after a lost answer is not applied twice.",
            "inputSchema": {"type": "object", "properties": {
                "service": {"type": "string", "enum": names},
                "payload": {"type": "string", "description": "The request body"}},
                "required": ["service", "payload"]},
        }));
    }
    list
}

/// A `tools/call` as a `ToolCall`. The file tools and `exec` share the LLM
/// agent's strict parser, so the two front doors accept the same arguments.
fn parse(name: &str, args: &Value) -> Result<ToolCall> {
    if !args.is_object() {
        bail!("arguments must be a JSON object");
    }
    if name == "ext_call" {
        let field = |k: &str| args.get(k).and_then(Value::as_str).map(str::to_string).with_context(|| format!("missing `{k}`"));
        return Ok(ToolCall::ExtCall { service: field("service")?, payload: field("payload")? });
    }
    crate::agent::llm::parse_call(name, &args.to_string())
}

/// The agent record of an MCP execution. The client drives the calls, so
/// this is only what `runtime::create` stores: the kind and a task line.
struct McpClient {
    name: String,
}

impl Agent for McpClient {
    fn next_action(&mut self, _last: Option<&ToolResult>) -> Result<Action> {
        bail!("an MCP execution is driven by its client, not by `norn run`; continue it with `norn mcp`")
    }
    fn kind(&self) -> &'static str {
        "mcp"
    }
    fn task(&self) -> String {
        format!("MCP session ({})", self.name)
    }
    fn persist(&self, _dir: &ExecDir) -> Result<()> {
        Ok(())
    }
    fn restore(&mut self, _history: &[ToolResult], _events: &[crate::log::Event]) -> Result<()> {
        Ok(())
    }
}

/// The repository as `norn mcp` stores it: canonical, so that the same folder
/// named two ways is still one repository.
pub fn canonical_repo(repo: &Path) -> Result<PathBuf> {
    let repo = std::fs::canonicalize(repo).with_context(|| format!("--repo {}", repo.display()))?;
    if !repo.is_dir() {
        bail!("--repo {} is not a folder", repo.display());
    }
    Ok(repo)
}

/// The protocol channel: standard output moved to a descriptor of its own,
/// and standard output pointed at standard error. A stray print anywhere in
/// the runtime (or in a program it starts) then lands in the log stream
/// instead of breaking the protocol, which allows only MCP messages on
/// standard output.
pub fn take_stdout() -> Result<File> {
    use std::os::fd::FromRawFd;
    // SAFETY: plain descriptor calls on 0-2, which exist in any process
    // started by a client. F_DUPFD_CLOEXEC gives a new descriptor that
    // programs started later do not inherit; dup2 then replaces 1 with a
    // copy of 2. The new descriptor is owned by the returned File alone.
    unsafe {
        let proto = libc::fcntl(1, libc::F_DUPFD_CLOEXEC, 3);
        if proto < 0 {
            return Err(io::Error::last_os_error()).context("duplicating standard output");
        }
        if libc::dup2(2, 1) < 0 {
            return Err(io::Error::last_os_error()).context("pointing standard output at standard error");
        }
        Ok(File::from_raw_fd(proto))
    }
}
