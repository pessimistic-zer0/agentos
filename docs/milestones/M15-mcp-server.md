# M15 — norn as an MCP server

| | |
|---|---|
| Weeks in the plan | v2 (D-139); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` waits for a CI run |
| Commits | `5dd95f8` (experiments) … the M15 commit (tag `m15` after CI is green, D-113) |
| Status | Done. Verified on both presets and Debian 12 by CI run 45728656310 on `1571d09`; tagged there (D-158) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Until now norn ran only its own agents (a script, or a small LLM loop). M15 lets an agent norn did not write use it: **`norn mcp`** speaks MCP (the Model Context Protocol), the standard that Claude Code, Cursor and others use to call tools in another program. The client's file and command calls then go through norn's gateway, policy, sandbox, event log and checkpoints, exactly like a call from `norn run`. No part of the durable machinery changes; M15 adds a front door to it.

## 2. Requirements

From `docs/v2-scope.md`, M15, as changed by the experiments (D-141).

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| A1 `norn mcp --repo --policy`, an MCP server over stdio | Yes, `nix` | `tests/mcp.rs` (7 tests); `make test` 175 passed |
| A2 the tools the policy allows; each call through the gateway; denials as tool errors | Yes, `nix` | `a_client_fixes_the_demo…` (`.env` denied, `isError: true`), `a_policy_without_commands_offers_no_exec…` |
| A3 (D-141) a session's end only stops the server; `norn finish` takes the result | Yes, `nix` | `a_client_fixes_the_demo…`: SIGINT, `status` says `open`, `apply` points at `finish`, `finish` writes the patch, `apply --to` applies it |
| A4 what the client was offered is in the log | Yes, `nix` | the `mcp_session` message: client, protocol revision, tool list |
| A5 setup for Claude Code with its own tools off | Yes, `nix` | README "From an MCP client"; E2; `docs/results/mcp-claude-code-demo-nix.md` |
| Experiments E1 to E3 first | Yes | `experiments/m15-mcp/NOTES.md` |
| A real client does a task end to end | Yes, `nix` | Claude Code fixed the demo through norn; `finish` + `apply`; 4 of 4 tests pass in the repository copy |
| Both presets | `fhs` not yet | needs a push and a CI run |

## 3. What you need to know first

**MCP.** A client (the AI app) starts a server (here `norn mcp`) as a child process and talks to it through the child's standard input and output, one JSON message per line. The messages follow JSON-RPC 2.0: a request has a `method`, `params` and an `id`; the answer has the same `id` and either a `result` or an `error`. A *notification* has no `id` and gets no answer.

**Two revisions.** The current one, 2026-07-28, has no opening handshake: every request carries `_meta` with the protocol version and the client's capabilities, and the first request is `server/discover`. The previous one, 2025-11-25, opens with `initialize`. Claude Code uses the new one; norn serves both (D-140).

**Tool error or protocol error.** If the model asks for something wrong *within* a tool (a missing file, a refused path, a missing argument), the answer is a normal result marked `isError: true`, with text the model reads and can act on. If the request itself is wrong (an unknown method or tool, missing `_meta`), the answer is a JSON-RPC `error`.

**File descriptors.** A process refers to open files and pipes by small numbers: 0 is standard input, 1 standard output, 2 standard error. `dup2(a, b)` makes `b` point where `a` points. Close-on-exec marks a descriptor so programs started later do not inherit it.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| E1 (reading) | The specification on 2026-11-26 | Current revision 2026-07-28 removes `initialize` | The plan's handshake was out of date before a line was written |
| E1 (probe) | An 80-line Python server with one tool, `text.upper`, and Claude Code | `server/discover`, no `initialize`; a wrong `cacheScope` made Claude Code drop every tool silently; dots in tool names renamed to `_` for the model; `isError` shown as an error | Test against the real client with its debug log on; be exact in what the server sends |
| E2 | `--tools ""`: can the model act any other way? | Only the probe's tool; no file reading, no shell | With Claude Code the client can be confined to norn's tools; it is the client's setup, not norn's |
| E3 | The server exits in the middle of a call | `Connection closed` to the model; Claude Code starts the server again for the next call, same command line | A restart must continue the same execution (D-141) |
| E3b | A server that ignores signals | SIGINT, SIGTERM after 100 ms, SIGKILL after another 400 ms; standard input never closed | No time to write a patch at the end of a session (D-141) |

## 5. What was built

| File | Purpose |
|---|---|
| `src/mcp.rs` | The server: reading messages, the two revisions, `tools/list`, `tools/call` into the gateway, opening or continuing the execution, the protocol channel |
| `src/runtime.rs` | `reopen` (lock, recover, rebuild the gateway), `finish_execution` (`norn finish`), `unfinished_mcp_executions`; `resume` now shares `reopen_gateway` with them |
| `src/interrupt.rs` | `serving`: the message printed when a client stops the server |
| `src/main.rs` | The commands `norn mcp` and `norn finish` |
| `src/trace.rs` | `status` says `open` or `serving` for an MCP execution instead of `interrupted` |
| `src/agent/llm.rs` | The tool schemas, the argument parser and the result text are shared with `src/mcp.rs` |
| `tests/mcp.rs`, `tests/common/mcp_client.rs` | Seven tests and a small client that talks to the real binary |
| `experiments/m15-mcp/` | The probe server and the notes |

How the pieces talk:

```
Claude Code ──stdin──▶ norn mcp (src/mcp.rs) ──ToolCall──▶ Gateway::handle ──▶ policy, intent, effect, checkpoint, completed
            ◀─stdout── (one JSON line per answer) ◀──ToolResult──┘
                       stderr: messages for the person
```

## 6. The code, explained

**`main.rs`, `Cmd::Mcp`.** The first thing it does is `take_stdout()`: from then on, standard output belongs to the protocol.

**`mcp::take_stdout`.** `fcntl(1, F_DUPFD_CLOEXEC, 3)` makes a new descriptor (3 or higher) that points where standard output points, and marks it close-on-exec so that programs norn starts do not get it. `dup2(2, 1)` then points descriptor 1 at standard error. The new descriptor is wrapped in a `File` and used only for protocol messages. Anything that later prints with `println!`, in norn or in a program it starts, lands on standard error, which the specification allows for logging. Without this, one stray print would be read by the client as a broken message.

**`mcp::read_line`.** Reads up to a newline, but keeps at most `MAX_MESSAGE` (32 MiB) in memory. It works on the reader's buffer directly (`fill_buf` / `consume`): it looks for `\n` in what is buffered, copies that part, and tells the reader how much was used. A longer line is skipped to its end and reported as `TooLarge`, so the server answers `-32600` and goes on. The obvious `read_line` from the standard library would read the whole line into memory, however long.

**`Server::handle`.** Decides what kind of message it is: not an object (`-32600`), an answer (ignored: norn sends no requests), a notification (no `id`: never answered), or a request. For a request it picks the rules: `initialize` starts the 2025-11-25 era; a request with the new `_meta` is served by the 2026-07-28 rules; one without `_meta` in a session that never sent `initialize` is malformed (`-32602`). `check_meta` refuses an unknown version with `-32022` and the list of known ones, as the specification says. In the new era every result gets `resultType: "complete"`.

**`Server::tools_call`.** Checks the name against the tools this policy offers; an unknown name is a protocol error that lists the real names. The arguments go through the same strict parser as the LLM agent's (`llm::parse_call`), so both front doors accept exactly the same calls; a mistake there becomes a tool error the model can read. Then `open()` provides the execution, the budget is checked, and `gw.handle(call)` does the rest. The answer's text comes from `llm::render_result`, the text the built-in agent's model sees too.

**`Server::open` and `continue_or_create`.** The execution is made at the first tool call, not at start: Claude Code starts every configured server whenever it starts. `continue_or_create` first asks `runtime::unfinished_mcp_executions` for executions made by `norn mcp` on the same repository (compared as canonical paths, so `./repo` and `/home/…/repo` are the same) that have not finished, newest first. The first one it can lock is reopened: `runtime::reopen` takes the lock, runs recovery (design rule 5: a call that was cut off is settled before anything else), and rebuilds the gateway from the log. Only if there is none does it call `runtime::create`, the same function `norn run` uses. Either way, `interrupt::serving` sets the message printed on a stop, and the session (client, revision, tool list) is written to the log.

**`runtime::finish_execution`.** `norn finish`: lock, recover, and if the execution is not finished, the same `finish_parts` as the end of a run: write the patch from the first checkpoint to the workspace, then the `execution.finished` record. It needs no sandbox, so it does not need a delegated cgroup.

**What happens on a stop.** Nothing new: the D-106 handler writes the `serving` message to standard error and exits at once. The execution is unfinished, exactly as after `kill -9`, which the kill suite tests 4,800 times. The next `norn mcp` recovers it.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-139 | v2 scope approved; 1.1.0 | M15–M16 only; 2.0.0 |
| D-140 | Serve both MCP revisions | Only the new one; only the old one |
| D-141 | A stop is only a stop; restart continues; `norn finish` | Finish on SIGINT; patch after every change; a finish tool; `--resume <id>`; a new execution per start |
| D-142 | Underscores in wire names | Dots on the wire; renaming the tools everywhere |
| D-143 | How it is built: own serve loop, open at first call, private protocol descriptor, error split, session in the log | An `Agent` driven by `drive`; open at start; relying on no prints; a new event kind |

## 8. Problems hit

1. The plan's `initialize` handshake is gone from the current MCP revision (E1). Found by reading the specification before coding.
2. The first probe's tool list was silently dropped by Claude Code (`cacheScope: "session"`). Found only in Claude Code's debug log.
3. Two plan items (finish on disconnect, resume by id) did not fit a real client (E3, E3b); replaced by D-141 before any code was written.
4. Three of the seven tests failed at first, all in the tests: a guessed line of the demo's fix, an `[exec]` replacement that matched a comment, and a write to a path the policy does not allow. The server was right each time.
5. The first Claude Code session failed at every call because `pytest` was not on the client's `PATH`; the message reached the model and the person. Not a bug; documented.

## 9. How to run it

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test mcp      # 7 tests
```

With Claude Code (inside the dev shell, so that `pytest` is on `PATH`):

```sh
cargo build
cp -a examples/demo-repo /tmp/demo
cat > /tmp/norn.json <<EOF
{"mcpServers": {"norn": {"command": "$PWD/target/debug/norn",
  "args": ["mcp", "--repo", "/tmp/demo", "--policy", "$PWD/policies/nix.toml"]}}}
EOF
claude -p 'The repository has a failing test. Fix it.' --mcp-config /tmp/norn.json --strict-mcp-config --tools "" --allowedTools mcp__norn
target/debug/norn status                 # open (`norn mcp` continues it, `norn finish` ends it)
target/debug/norn finish <id> && target/debug/norn apply <id>
```

## 10. What this milestone does not do

- No kill cells or security cases sent through MCP yet: M16.
- Only stdio; no MCP over HTTP (out of v2).
- The client's own tools are the client's business: norn cannot stop a client that keeps its shell from using it.
- A call is handled to its end once started; a cancellation from the client cannot stop a running command.

## 11. Check your understanding

1. What is the difference between a JSON-RPC request and a notification, and why does norn never answer a notification?
2. Why does `norn mcp` move standard output to another descriptor before doing anything else? What would a single `println!` do otherwise?
3. Why is a missing `path` argument a tool error, but an unknown tool name a protocol error?
4. Claude Code sends SIGINT and kills the server 500 ms later. Why does norn not try to finish the execution in that time?
5. After a crash, Claude Code starts `norn mcp` again with the same arguments. How does norn find the execution to continue, and what happens to the call that was cut off?
6. Why is the execution created at the first tool call, not when the server starts?
7. What does `--tools ""` do in Claude Code, and why can norn not enforce it itself?
8. Why do the tool names use `_` on the wire but `.` in the log?
9. Why is a line longer than 32 MiB skipped instead of read?
10. What does `norn finish` do that a session's end does not, and why does it need no sandbox?
