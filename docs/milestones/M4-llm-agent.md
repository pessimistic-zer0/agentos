# M4 — LLM agent loop

| | |
|---|---|
| Weeks in the plan | 9 |
| Started / finished | 2026-11-12 / 2026-11-12 |
| Commits | the two commits after tag `m3` (tag `m4`) |
| Status | Done |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Until now the "agent" was a script. M4 adds the second agent the plan lists: a minimal loop around a real language model. The model sees the task and four tools; each reply asks for one tool call; the runtime sends that call through the same gateway, policy, sandbox and log as before, and gives the result back. The point of the milestone is to show the machinery of M1–M3 holding up when the caller is a real, unpredictable model instead of a test script. It adds no new part; it is a second user of the **gateway**, and it puts the conversation into the **event log**.

## 2. Requirements

Exit criterion from the v1 table: *"A real LLM completes the demo task inside the sandbox."*

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| A real LLM completes the demo task inside the sandbox | Yes | Local llama.cpp server with Qwen3-VL-4B-Instruct; `agentos run --agent llm --task "..."` on `examples/demo-repo`: 5 calls, last one `exec pytest -q … exit 0`, status `completed`. Repeated 3 more times with the same result. Three earlier runs **failed**; see §8 and `docs/results/llm-failed-runs-qwen3-vl-4b.md` |
| The patch is the right fix | Yes | `result.patch` changes `sum(values) / (len(values) - 1)` to `sum(values) / len(values)` and keeps `median()` (plus whitespace changes the model made) |
| OpenAI-compatible endpoint, local or hosted | Yes | `AGENTOS_LLM_BASE_URL`, `AGENTOS_LLM_MODEL`, `AGENTOS_LLM_API_KEY`; the mock in `tests/llm_agent.rs` and the real llama.cpp server both speak it. A hosted endpoint was **not** tried |
| The LLM is called by the runtime, outside the sandbox; commands get no network | Yes | `agent/llm.rs` runs in the runtime process; M2's `exec_has_no_network` still passes |
| Every LLM message is an event (ADR 0005) | Yes | `llm_loop_fixes_the_demo_and_logs_the_conversation`: 13 `agent.message` events in the expected role order; the assistant message precedes its `tool.requested` |
| The conversation survives a crash | Yes | Mock: `llm_run_resumes_after_a_crash_with_the_conversation_from_the_log`. Real model: killed at P4 on call #4, `run --resume` → `#4 fs.write APPLIED`, `#5 exec pytest -q exit 0`, `completed` (`docs/results/llm-demo-qwen3-vl-4b.md`) |
| The API key is never stored | Yes | Same test greps the execution folder for the key: not found; unit test `the_api_key_is_never_serialized` |
| Earlier suites still pass | Yes | `make test`: 22 unit, 2 LLM-mock, recovery (72 kills), 25 security, 4 vertical-slice; `cargo clippy --all-targets` clean |

Honest limits of the first row: one small model, one easy task, temperature 0 (so "4 of 4" is close to one sample repeated). It shows the loop works end to end, not that the agent is good.

## 3. What you need to know first

**Chat completion API.** You send a list of messages, each with a role (`system`: standing instructions; `user`: the task; `assistant`: the model's earlier replies; `tool`: results of tools). The model returns the next assistant message. "OpenAI-compatible" means a server accepts this format at `/v1/chat/completions`; llama.cpp's server does.

**Tool calls (function calling).** With the request you describe functions (name, parameters as a JSON schema). The model may answer with `tool_calls`: a function name and JSON arguments, instead of text. *The model runs nothing.* Your code decides what to do with the request. In AgentOS that request becomes a `ToolCall` for the gateway.

**The loop.** ask model → it requests a call → gateway runs it (or refuses) → append the result as a `tool` message → ask again. Ends when the model answers with plain text.

**Context window and `max_tokens`.** A model can only take in a limited amount of text, and generates until it decides to stop or hits a limit. `max_tokens` is that limit per reply.

**Temperature.** How random the model's choices are. 0 means "always the most likely next word": repeatable, but a model stuck in a loop stays stuck.

**Why this is safe to connect at all.** The model's output is treated like the scripted agent's: an untrusted request. A model confused by a hostile file (prompt injection) can only *ask*; the policy and the sandbox answer.

## 4. Experiments

The v1 table lists none for week 9. What was tried:

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| What is already on the machine | Looked for local models and a llama.cpp build | Found a 3.7 GB Qwen3-VL-4B GGUF and a CUDA `llama-server` in the Nix store | No download, no hosted API, no key needed (D-058) |
| Real run 1 | First version of the loop | Model deleted `median()` by a partial write, then listed folders until the 50-call budget ended | Detect repetition |
| Real run 2 | + repeat detection, + reminder after writes | Same partial write; model then repeated text until its context was full; run ended "completed" | Cap replies; "stopped talking" ≠ "done" |
| Real run 3 | + reply cap, + sharper prompt | Same partial write; tried a denied write; re-ran pytest 5 times; stopped by the repeat guard | Telling the model harder did not help |
| Real run 4 (+3 repeats) | + warning naming the lines a write dropped | Partial write, warning, full rewrite, tests pass | Report what the action *did* |
| Real run with a crash | Run 4's setup, killed at P4 on call #4 | Recovered as APPLIED, resumed, completed | M3's recovery works with the real agent |

## 5. What was built

| File | Purpose |
|---|---|
| `src/agent/llm.rs` | The LLM agent: config, the HTTP request, tool schemas, parsing calls, rendering results, guard rails, restore |
| `src/agent/mod.rs` | `Agent::take_messages`; `Action::GiveUp` |
| `src/runtime.rs` | Logs the agent's messages before acting; `GiveUp` → status `stopped`; summary cut for printing |
| `src/recovery.rs` | `history` now reads completed calls' outputs back from the blob store |
| `src/workspace.rs` | `ExecDir::llm_config` (`llm.json`: endpoint, model, task; no key) |
| `src/main.rs` | `--agent llm --task "..."`; `--resume` for LLM runs |
| `Cargo.toml` | `ureq` 3 with `json` |
| `tests/llm_agent.rs` | Mock endpoint; loop test and crash-resume test |
| `docs/results/llm-demo-qwen3-vl-4b.md` | Trace of the real run with a crash in the middle |
| `docs/results/llm-failed-runs-qwen3-vl-4b.md` | Traces of the three failed runs |

```text
            runtime process (trusted, has network)                 sandbox (no network)
 ┌───────────────────────────────────────────────────────┐
 │ LlmAgent.next_action(last result)                     │
 │   push {role: tool, content: result (+ warnings)}     │
 │   POST /v1/chat/completions  ───────────────►  llama.cpp server / hosted API
 │   ◄── {role: assistant, tool_calls: [fs_write {...}]} │
 │   parse → ToolCall                                    │
 │ runtime: log agent.message events                     │
 │ gateway.handle(ToolCall) ── policy ── intent ── effect ──────────►  exec runs here
 │   ◄── ToolResult                                      │
 └───────────────────────────────────────────────────────┘
```

## 6. The code, explained

### 6.1 The request: `LlmAgent::complete`

```rust
let body = json!({
    "model": self.cfg.model,
    "messages": self.messages,
    "tools": tool_schemas(),
    "temperature": 0,
    "max_tokens": MAX_REPLY_TOKENS,
});
let agent: ureq::Agent = ureq::Agent::config_builder()
    .timeout_global(Some(Duration::from_secs(600)))
    .http_status_as_error(false)
    .build().into();
let mut req = agent.post(&url).header("Content-Type", "application/json");
if let Some(key) = &self.cfg.api_key {
    req = req.header("Authorization", &format!("Bearer {key}"));
}
let mut resp = req.send_json(&body)?;
```

`json!` builds a JSON value from Rust expressions. `ureq` is a *blocking* HTTP client: `send_json` returns when the reply has arrived, which fits a runtime that does one thing at a time (D-004). `http_status_as_error(false)` lets the code read the body of an error reply, so the user sees the server's message instead of only "HTTP 400". The whole conversation is sent every time; the server keeps no memory.

`LlmConfig` has `#[serde(skip)]` on `api_key`: serde leaves that field out when the config is saved to `llm.json`. On resume the key is read from the environment again.

### 6.2 The tools the model sees: `tool_schemas` and `parse_call`

```rust
f("fs_write", "Replace a file with new content (the whole file).",
  json!({"path": path, "content": {"type": "string", "description": "The complete new content of the file"}}),
  &["path", "content"]),
```

Four schemas, one per tool. `parse_call` turns the model's answer into the same `ToolCall` enum the scripted agent uses, and is strict: an unknown function name or a missing argument is an error that is sent back to the model as a tool result (`invalid tool call: …`), at most three times in a row. There is deliberately no path from model output to anything except a `ToolCall`.

### 6.3 One step: `next_action`

```rust
if let (Some(result), Some(id)) = (last, self.pending.take()) {
    let mut content = render_result(result);
    if let Some(warning) = self.track_file_content(result) { content.push_str(&warning); }
    if let Some(n) = self.repeat_count() { ... nudge at 2, GiveUp at 5 ... }
    self.push(json!({"role": "tool", "tool_call_id": id, "content": content}));
}
...
let msg = self.complete()?;
let calls = msg["tool_calls"].as_array().cloned().unwrap_or_default();
let Some(first) = calls.first() else {
    ... return Ok(Action::Finish { summary });
};
```

`self.pending` holds the id of the tool call the model is waiting on. `Option::take()` returns the value and leaves `None` behind, so a result is attached to a call exactly once. `let Some(first) = ... else { ... }` ("let-else") handles the "no tool call" case up front: the reply is the final answer.

Only the *first* tool call of a reply is kept, and the stored assistant message is rewritten to contain only that one. The API requires one `tool` message per call; keeping one call keeps the conversation valid and the gateway at one call at a time.

`push` adds a message to `self.messages` (what the model sees) and to `self.unlogged` (what the runtime has not stored yet).

### 6.4 Into the log: `take_messages` and `runtime::drive`

```rust
let action = agent.next_action(last.as_ref());
for message in agent.take_messages() {
    gw.log.append(EventBody::AgentMessage { message })?;
}
match action? { ... gw.handle(call) ... }
```

The messages are appended *before* the action is acted on (and even if `next_action` returned an error). So the log always shows the message that led to a call. `std::mem::take(&mut self.unlogged)` hands over the list and leaves an empty one. The agent never touches the log; it has no handle on it.

### 6.5 What the model is told: `render_result`

Denials say `DENIED by policy: <rule>. Nothing was done.`; failures say nothing was changed; a call cut off by a crash says `NOT APPLIED: the runtime was restarted … call the tool again`. Exec results carry the exit status and both output streams, and mention reverted paths. Everything is cut at 12,000 characters.

### 6.6 The guard rails (D-060)

```rust
(ToolCall::FsWrite { path, content }, _) => {
    let old = self.seen.insert(path.clone(), content.clone())?;
    let kept: HashSet<&str> = content.lines().map(str::trim).collect();
    let dropped: Vec<&str> = old.lines().map(str::trim).filter(|l| !l.is_empty() && !kept.contains(l)).collect();
    if dropped.len() < DROPPED_LINES_WARNING { return None; }
    ...
}
```

`self.seen` remembers the last content read or written per file. `HashMap::insert` returns the previous value, and `?` on an `Option` returns `None` early if there was none (a file never read: nothing to compare). The comparison is a set difference of trimmed lines: crude, cheap, and enough to catch "sent one function of three". The warning is information added to the result. The model chooses what to do next.

`note_call` counts identical read-only calls since the last write (a write clears the counts, because reading again after a change is reasonable). `Action::GiveUp` is a third kind of action next to `Call` and `Finish`; the runtime records it as status `stopped`.

### 6.7 After a crash: `restore`

```rust
self.messages = events.iter().filter_map(|e| match &e.body {
    EventBody::AgentMessage { message } => Some(message.clone()),
    _ => None,
}).collect();
let made = self.messages.iter().filter(|m| m["tool_calls"].is_array() && is_valid_call(m)).count();
let last_is_call = self.messages.last().map(|m| m["tool_calls"].is_array()).unwrap_or(false);
if last_is_call {
    if made == history.len() {
        self.pending = /* id of that call */;
    } else {
        self.messages.pop();
    }
}
```

The conversation is whatever the log holds. How it ends tells the three cases apart:

- ends with an assistant call, and there are as many valid calls in the conversation as calls in the log → that call reached the gateway. The runtime passes its result (for a cut-off call, recovery's verdict) to `next_action`, which appends it.
- ends with an assistant call that has no call in the log → the crash came between logging the message and the gateway. Drop it from the working conversation and ask the model again.
- ends with a tool result → the request to the model was cut off. Ask again; the answer may differ, which is accepted (D-017).

`recovery::history` fills in the real output of completed calls from the blob store, so a result the model never saw (crash at P7) can still be shown to it.

### 6.8 The tests: a mock model

`tests/llm_agent.rs` starts a 40-line HTTP server on a thread: read headers, read `Content-Length` bytes, answer JSON. Its "model" (`reply_for`) counts the tool results in the request and returns the next step of a fixed plan, including a read of `.env` that must be denied. Because the reply depends only on the request, the same mock works after a crash, when the request is rebuilt from the log. The mock records every request, which is how the test checks that the first request after a restart carries the full earlier conversation.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-058 | Local llama.cpp with a model already on disk; the API key found in the environment was not used | Hosted NIM endpoint; downloading a model; stopping as blocked |
| D-059 | Native tool calls, one per reply; every message an event; restore from the log | JSON-in-text; several calls per reply; the agent writing the log |
| D-060 | Guard rails: reply cap, repeat detection, dropped-lines warning, `GiveUp` → `stopped` | A fifth (patch) tool; task-specific hints; only a stronger model |

Parking lot (spec §22): a patch/edit tool.

## 8. Problems hit

1. **Run 1 failed: partial write, then a loop.** Expected the model to send the whole file. It sent only `mean()`; `median()` was deleted; pytest failed on import. The model kept believing `median()` existed and listed `src` and `tests` in turn 42 times, until the call budget stopped the run. Found by reading the logged conversation (`agentos trace --json`). Response: count repeated calls; nudge, then stop.
2. **Run 2 failed: runaway text.** Same partial write. Then the model produced the same two paragraphs until its context was full, with no tool call, and the run was recorded as `completed` with a summary thousands of characters long. Response: `max_tokens`; cap the summary; later `Action::GiveUp`.
3. **Run 3 failed: prompt wording did not help.** A numbered, emphatic rule about whole-file writes changed nothing. The model also tried to write `__init__.py` in the repo root; the policy refused (`no matching rule`), which is the system working. The repeat guard ended the run after five identical pytest calls.
4. **Run 4 worked.** The change: when a write drops lines the model had read from that file, the tool result says how many and names four. The model then rewrote the file completely. Lesson recorded in Findings: reporting the *effect* of an action worked where repeating the *instruction* did not.
5. **A stopped run said "completed".** In runs 2 and 3 the status was `completed` because the agent had returned `Finish`. Fixed with `Action::GiveUp` → `stopped`.
6. **`pkill` ended more than the server.** Stopping the model server with `pkill -f` also matched the shell running the test command (its command line contained the pattern), which exited with code 144 before the tests ran. Rerun without the `pkill`. No effect on the project; noted because the first "result" of that command was not a test failure.

No runtime bug was found by the suites in M4, so `docs/bugs.md` is unchanged.

## 9. How to run it

With the mock (no model needed): `nix develop -c make test`.

With a real model, any OpenAI-compatible server. For llama.cpp:

```sh
llama-server -m <model>.gguf --jinja --port 8080 -ngl 99 -c 8192     # --jinja enables tool calls
export AGENTOS_LLM_BASE_URL=http://127.0.0.1:8080/v1 AGENTOS_LLM_MODEL=local
nix develop -c systemd-run --user --scope -p Delegate=yes cargo run -- run \
    --repo examples/demo-repo --policy policies/nix.toml --agent llm \
    --task "The test suite of this Python project has a failing test. Find the bug in the source code and fix it so that all tests pass. Run the tests with: pytest -q"
```

Output of the recorded run (ids and wording vary by model):

```text
  #1   exec     pytest -q                                allow  exit 1, 0.2s
  #2   fs.read  src/stats.py                             allow  554 bytes
  #3   fs.write src/stats.py (194 bytes)                 allow  wrote 194 bytes
  #4   fs.write src/stats.py (545 bytes)                 allow  wrote 545 bytes
  #5   exec     pytest -q                                allow  exit 0, 0.2s
<id>  COMPLETED  All tests now pass. The `mean` function has been fixed ...
```

For a hosted API set `AGENTOS_LLM_BASE_URL`, `AGENTOS_LLM_MODEL` and `AGENTOS_LLM_API_KEY`. `agentos trace --events` shows the conversation; `agentos run --resume <id>` continues an interrupted LLM run.

## 10. What this milestone does not do

- **It does not show the agent is good.** One 4B model, one small task. A different model may need different guard rails or none.
- **No hosted endpoint was tested.** The code path is the same; only the mock and llama.cpp were run.
- **No streaming, no parallel tool calls, no conversation trimming.** A long run will exceed the model's context window and fail with the server's error.
- **Prompt injection is not prevented, only contained.** A file can talk the model into asking for something bad. The claim is that the policy and sandbox then refuse or confine it (M2 tests), not that the model resists.
- **The conversation goes to the endpoint.** With a hosted API, file contents the agent reads leave the machine. Denied files are never in the workspace, so they cannot be sent.
- **The model server is not managed by AgentOS.** It must be started by the user.
- **`fhs` preset not yet verified.**

## 11. Check your understanding

1. In one sentence each: what do the roles `system`, `user`, `assistant` and `tool` mean in a chat request?
2. The model answers with `tool_calls`. What has been executed at that moment?
3. Why does the runtime keep only the first tool call of a reply?
4. Why are the agent's messages appended to the log *before* the gateway handles the call?
5. Where does the API key live during a run, and how do the tests show it is never written to disk?
6. After a crash, the stored conversation ends with an assistant message containing a tool call. Which two situations can that be, and how does `restore` tell them apart?
7. Why is it acceptable that a re-sent request to the model may get a different answer?
8. The model tried to write `__init__.py` in the repo root and was refused. Which rule refused it, and why is that the same mechanism that stops a hostile agent?
9. Three runs failed in the same way. What was the model's mistake, and why did a stronger instruction not fix it while a warning in the tool result did?
10. What does `Action::GiveUp` change compared with `Finish`, and what wrong record did its absence produce?
11. Why was the API key that was found in the environment not used?
12. What exactly does "a real LLM completes the demo task" show, and what does it not show?
