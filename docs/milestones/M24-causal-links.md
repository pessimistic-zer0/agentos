# M24 — Causal links and the file-change view

| | |
|---|---|
| Weeks in the plan | v4 (D-169); no week plan |
| Started / finished | 2026-11-28 / 2026-11-28 on `nix`; `fhs` waits for a CI run |
| Commits | the M24 commit (tag `m24` after CI is green, D-113) |
| Status | Done on `nix`; not yet verified on `fhs` |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

M23 showed *what* a run did and *when*. M24 answers two more questions: **why** a call was made (what asked for it), and **what it changed**. A call now records its cause, an id that points at the message, script step or client request that asked for it. `norn trace --graph` draws all the links the log holds. `norn trace --diff N` shows the files call N changed. It touches the **gateway** (it records the cause), the **event log** (one optional field) and the **checkpoints** (read, to compare two of them).

## 2. Requirements

From `docs/v4-scope.md`, M24 (C1 to C4), with D-170 and D-171.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| C1 `tool.requested` gets an optional `cause`: the assistant message's `seq`; through MCP the client's request id; old logs say "cause not recorded" | Yes, `nix`, and the scripted agent's step as well | `each_call_names_the_assistant_message_that_asked_for_it` (`tests/llm_agent.rs`: every call, across a crash and resume, names an assistant message that asked for that tool); `an_mcp_call_names_its_request_and_an_approval_links_two_calls` (ids 1 to 4, a string id, a 300-byte id not kept); `a_scripted_run_records_its_steps_as_causes_…` (steps 1 to 7); the 1.2 fixture shows "cause not recorded" |
| C2 the links ids already give, shown together: approval asked → used, uncertain call → recovery, stop → recovery → resume | Yes, `nix` | the graph tests above; the LLM test shows "after a stop, resumed by `norn run --resume`" |
| C3 `--graph` as a tree; `--graph=dot` for Graphviz; no new dependency | Yes, `nix` | `check_dot` in `tests/trace.rs` (every edge's nodes are declared, labels quoted); `cargo test --lib trace::graph` (escaping); Graphviz drew a run's DOT without error (D-173) |
| C4 `--diff N` between the checkpoints around call N; `--diff` for the whole run | Yes, `nix` | `a_scripted_run_records_…`: the fix's two lines for call 6; "no change" for `pytest`; "changed no files" for `fs.list`; "was not run" for the denied read; the whole run holds the same changed lines as `result.patch`; the checkpoints are unchanged after reading |
| Every existing test passes unchanged | Yes, `nix` | `nix develop -c make test`: 282 passed, 0 failed |
| Both presets | Not yet | needs a CI run |

## 3. What you need to know first

**A cause is an id, not a guess.** The log is in order, so it is tempting to say "the message just before this call asked for it". Usually that is true; but "usually" is a guess, and spec §9.13 says causes come only from recorded ids. So the runtime writes the id at the moment it knows it: when it hands the call to the gateway.

**Where the id comes from.** The LLM agent's turn ends with an *assistant message* that holds one tool call; the runtime stores it as `agent.message` and gets its `seq` back from `append`. The scripted agent knows which step of its script it is on. An MCP client sends each `tools/call` as a JSON-RPC request with an `id` it chose; that is all norn knows about why: the client's thinking never reaches the server.

**Checkpoints are snapshots.** After every call that could change files, the runtime copies the workspace into `checkpoints/cp-NNNN`, and the call's ending names that copy. Two neighbouring checkpoints, compared with `diff`, are exactly what the call between them changed.

**DOT.** A plain-text format for graphs: `a -> b;` is an arrow, `a [label="…"];` names a box. Graphviz's `dot` program turns it into a picture. norn writes the text; drawing it is up to the reader (no new dependency).

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m24-trial-logs` (E2) | What the kept trial logs can answer | No call linked to its message by an id; through MCP, only the session's tool list | C1 is needed; MCP's cause can only be the request id (D-170) |
| `m24-call-diff` (E5) | The time of one call's diff with the patch code | 3 to 7 ms on a normal workspace, 53 to 58 ms on 2,779 files | On demand, no cache (D-171) |

## 5. What was built

| File | Purpose |
|---|---|
| `src/log.rs` | `Cause` (three kinds); `cause` on `ToolRequested` |
| `src/gateway.rs` | `handle_from(call, cause)`; `handle` passes none |
| `src/runtime.rs` | `drive` finds the assistant message that holds the call, or asks the agent for its step |
| `src/agent/mod.rs`, `scripted.rs` | `Agent::step`: the scripted agent's step number |
| `src/mcp.rs` | The request's id as the cause; ids over 200 bytes not kept |
| `src/checkpoint.rs` | `patch_text`, the patch as text; `write_patch` now writes what it returns |
| `src/trace/graph.rs` (new) | The parts of a run, causes, approvals, recovery: `render_tree`, `render_dot` |
| `src/trace/diff.rs` (new) | `render_diff`: which two checkpoints, and the patch between them |
| `src/trace/inspect.rs` | The inspector shows the cause |
| `docs/event-schema.md` | The `cause` field |
| `tests/trace.rs`, `tests/llm_agent.rs` | The tests in §2 |

```text
agent turn ──▶ drive: store messages (seq 12 = assistant with the call) ──▶ gateway.handle_from(call, Message{seq: 12})
                                                                              └▶ tool.requested {call_id 5, cause {kind: message, seq: 12}}
norn mcp: tools/call id=7 ──────────────────────────────────────────────────▶ handle_from(call, McpRequest{id: 7})
```

## 6. The code, explained

**`log::Cause`.** An enum with `#[serde(tag = "kind", rename_all = "snake_case")]`: serde writes the variant's name into a field `kind` beside the variant's own fields, so `Cause::Step { step: 3 }` becomes `{"kind": "step", "step": 3}`. The field on `ToolRequested` has `#[serde(default, skip_serializing_if = "Option::is_none")]`: absent when there is no cause, and an old log without it still reads.

**`runtime::drive`.** Before acting on the agent's answer it stores each new message. For each one it checks whether it is an assistant message with a non-empty `tool_calls`; if so, the `seq` that `append` returned is the cause. If no such message came (the scripted agent sends none), it asks `agent.step()`. Then `gw.handle_from(call, cause)`.

**`mcp::Server::tools_call`.** Gets the request's `id` from `handle`. A string id longer than 200 bytes gives no cause: a client picks its ids, and one could otherwise make every event in the log as large as it liked.

**`trace::graph::build`.** One pass over the events. Each `tool.requested` goes into the current *part*: the start, or a new part opened by `recovery.started` or `execution.resumed` (a resume right after a recovery joins it, "after a stop, resumed by …"). Recovery's verdicts are listed in their part. Approvals are gathered by their id: which calls waited, who said yes and which call used it. `render_tree` prints this; `render_dot` writes the same as boxes and arrows, with a dashed arrow labelled "cause not recorded" for calls from old logs.

**`trace::graph::quote`.** Every DOT label goes through it: backslashes and double quotes are escaped, line breaks written as `\n`. Without it, a model's message containing `"` would end the label and the rest would be read as DOT.

**`trace::diff::render_diff`.** Finds the checkpoint the call's ending names. Then, in `log::named_checkpoints` (the list the runtime itself trusts), the one just before it. Calls that took no checkpoint (a read), were not run (denied, waiting), or failed get a sentence instead of a diff. The two folders go to `checkpoint::patch_text` with the execution's own policy, so scratch paths are left out as they are in `result.patch`.

**Rust features met here.** `#[serde(tag = …)]` for an internally tagged enum. A function taking a closure (`impl Fn(u64) -> Option<&'a Value>`) so that the same `cause_text` works with the graph's message map and with a plain search of the events. A named struct (`ApprovalTrail`) instead of a tuple of three vectors, which clippy asked for.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-170 | `cause` through MCP is the request id | The model's own `tool_calls[].id`; guessing from order |
| D-171 | Per-call diffs on demand | A cache of diffs |
| D-173 | M24 as built: three kinds of cause, ids over 200 bytes dropped, parts of a run by `seq`, escaped DOT, `patch_text` | |

## 8. Problems hit

- **The first LLM cause test crashed at a call with no P4.** P4 is "after the effect of a call that changes files"; call 3 of the mock's run is a read, so the run did not stop. The test now crashes at call 4, the write, as the existing resume test does.
- **A clippy warning** (`type_complexity`) on a tuple of three vectors; it became `ApprovalTrail`.

## 9. How to run it

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test trace --test llm_agent
nix develop -c make test
norn trace <id> --graph
norn trace <id> --graph=dot | dot -Tsvg > run.svg     # Graphviz, if installed
norn trace <id> --diff 6
norn trace <id> --diff                               # the whole run
```

## 10. What this milestone does not do

- Say *why* an MCP client made a call: its reasons never reach norn.
- Link a tool's answer to the next message the model wrote: the order says it, but no id does.
- Show the model's reasons when it wrote none: the relay runs of M13 show messages with no text.
- Token counts and replay: M25.

## 11. Check your understanding

1. Why does `drive` look for an assistant message with `tool_calls`, rather than take the last message stored?
2. Through MCP, what does the cause of a call tell you, and what does it not?
3. Why is a 300-byte request id not recorded?
4. A call from a 1.2 log shows "cause not recorded". Why does the trace not use the message just before it?
5. What is a "part" of a run in the graph, and why is placing calls in parts by `seq` not a guess?
6. Why does `--diff` call `patch_text` instead of `write_patch`?
7. Call 2 ran `pytest`, which writes its cache folder `.pytest_cache`, yet `--diff 2` says "no change to the files". Why? (Look at `filesystem.scratch` in the preset.)
8. What would go wrong if a model's message holding `"]; evil -> x; ["` were put into a DOT label without `quote`?
9. Which checkpoint does `--diff 6` compare call 6's checkpoint with, and where does that order come from?
