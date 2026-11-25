# norn v2 — Scope

*Drafted 2026-11-26 (D-138); approved by Prince the same day, as drafted (D-139). Road map it belongs to: D-137.*

**Status: built (M15 to M18, 2026-11-26), verified on `nix`; `fhs` waits for CI. Approved; this is the plan for v2.** All four milestones are in. The M18 trial uses Prince's Claude Code account. The release is numbered 1.1.0. Anything not listed here stays parked (spec §22), and the rule in CLAUDE.md holds: no feature that this plan does not list.

## 1. In one paragraph

Today norn runs its *own* agent: a scripted one, or a small LLM loop that talks to a model server. v2 lets **other agents** use norn. Claude Code, Cursor and most coding agents can call tools from a separate program through **MCP** (the Model Context Protocol: a standard way for an AI app to find and call tools offered by another program). norn will be such a program: an *MCP server*. An agent connected to it reads, writes and runs commands through norn's gateway, with the same policy, sandbox, event log, checkpoints and recovery as today. v2 also fixes the friction that the real-project trials found (spec §22), because agents connected through MCP will meet it first.

This is the first part of Phase 4 of the spec. The order of the later phases is in D-137: v3 hardening (approvals, secrets, network), v4 TraceWeaver, v5 scale and the rest of Phase 4.

## 2. Words used here

| Term | Plain meaning |
|---|---|
| MCP | Model Context Protocol. A standard for an AI application (the *client*, e.g. Claude Code) to list and call tools offered by another program (the *server*) |
| MCP server | The program that offers the tools. In v2, `norn mcp` |
| stdio transport | The client starts the server as a child process and they talk through its standard input and output, one JSON message per line. No network |
| JSON-RPC 2.0 | The message format MCP uses: a request has a method name, parameters and an id; the answer carries the same id |
| `tools/list`, `tools/call` | The two MCP methods that matter here: "what tools do you offer?" and "run this tool with these arguments" |
| Session | One connection, from the client starting `norn mcp` until it closes it. In v2 one session is one execution |

## 3. What is in v2

### M15 — norn as an MCP server

| # | Item | Notes |
|---|---|---|
| A1 | `norn mcp --repo <folder> --policy <file>`: an MCP server over stdio | Creates an execution exactly like `norn run` (the copy of the repository, the first checkpoint), then answers MCP messages. Standard output carries only protocol messages; everything meant for a person goes to standard error |
| A2 | The tools offered are the ones the policy allows: `fs.read`, `fs.list`, `fs.write`, `fs.edit`, `exec`, and `ext.call` when the policy has an `[external]` section | Each `tools/call` goes through the gateway unchanged: policy check, intent written and fsynced, effect, checkpoint, completion record. A denial is returned as a tool error that says why. On the wire the tools are called `fs_read`, `fs_list`, … (D-142) |
| A3 | ~~The end of a session finishes the execution~~ Replaced by D-141: **`norn finish <id>`** finishes an MCP execution (settles a cut-off call, writes `result.patch`); the end of a session only stops the server | E3b: a client stops the server with SIGINT and kills it half a second later, too soon to write a patch. The person then reads the patch and runs `norn apply`, as today |
| A4 | The tool list and the description of each tool, as sent to the client, are recorded in the event log at the start of a session | Spec §9.10: tool descriptions are part of what the model sees; the log should show what it was told |
| A5 | Setup instructions for one client (Claude Code), with its own file and shell tools switched off | norn can only govern calls that go through it. If the client keeps its own Bash or file tools, the agent can go around norn. The README and the threat model say so, and experiment E2 checks what switching them off really leaves |

### M16 — a killed MCP session

| # | Item | Notes |
|---|---|---|
| B1 | ~~`norn mcp --resume <id>`~~ Replaced by D-141: **`norn mcp` continues the unfinished MCP execution of the same repository** by itself, so a server the client restarts carries on where it stopped | Recovery first settles the call that was cut off (design rule 5), then the server answers again. A call the client sends again after the restart is a *new* call with a new id; norn never treats it as the old one |
| B2 | Kill cells through MCP | The fault-injection harness drives the existing crash points through a small scripted MCP client in the tests, and checks the same invariants as `tests/recovery/` |
| B3 | Security cases through MCP | The 32 adversarial cases sent as `tools/call` messages, plus protocol cases: broken JSON, a message too large, an unknown method, a call before the start of the session, a repeated request id. Expected: an error answer, never a crash that loses the log, never an effect outside the policy |

### M17 — what agents ran into (spec §22, from the trials of 2026-11-15 to 2026-11-17)

| # | Item | Notes |
|---|---|---|
| C1 | A **test baseline**: the test command is run once on the untouched copy, and its failures are recorded | The LLM agent's stop rule then refuses to finish only for failures that are *new*. Four of six agents in the relay trial had a finished answer refused because of failures in other tickets (`docs/results/relay-model-trial-nix.md`). MCP clients see the baseline in the result of `exec` |
| C2 | A **denial says what is allowed** | A refused command lists the allowed command prefixes; a refused write names the write globs. Reported independently by the six agents of the relay trial |
| C3 | **`fs.search {pattern, path?}`**: search the workspace for text | A new tool. It reads through `openat2` like `fs.read`, skips files matching deny globs, and has an output limit. Today an agent has to read files one by one, or `grep` has to be allowed as a command |
| C4 | **`fs.delete {path}`**: delete a file | A new mutating tool with the same order as `fs.write` (intent, effect, checkpoint, completion) and its own kill cells. Today an agent cannot remove a file |
| C5 | **`fs.edit` with `replace_all`** | Parked when `fs.edit` was built (D-120) |
| C6 | The LLM agent ends a run at once on `413 Request too large` | Today it tells the model three times first (hosted-model trial, 2026-11-16) |

The tool list becomes: `fs.read`, `fs.list`, `fs.write`, `fs.edit`, `fs.search`, `fs.delete`, `exec`, and the bonus `ext.call`.

### M18 — evidence and release

| # | Item | Notes |
|---|---|---|
| D1 | Trial: **Claude Code as an MCP client** on the six tickets of the inventory project, with its own tools switched off | Each ticket checked on a clone against the solution notes, as in M13. Uses Prince's Claude Code account, so it needs his yes (question 2 below) |
| D2 | The documents: README ("using norn from an MCP client"), threat model (a new section for the MCP front door), recovery semantics (a killed session), CHANGELOG, compatibility page | |
| D3 | Release: CI green on both presets and Debian, then the tag | Version number: question 3 below |

## 4. Experiments before building (as in v1, kept in `experiments/`)

| # | Question | How |
|---|---|---|
| E1 | What does the current MCP specification require from a stdio server, and which revision do clients speak? Are dots allowed in tool names? How does a client show a tool error? | Read the specification; write a server of about 30 lines that offers one tool, connect Claude Code to it, record every message |
| E2 | Can Claude Code be set up so that norn's tools are the *only* way to read, write or run anything? What is left (web fetch, its own memory files)? | Configure it with its built-in tools off; ask it to read a file outside the repository and to run a shell command; record what happens |
| E3 | What does a client do when the server dies in the middle of a call: restart it, call again, give up? | Kill the E1 server during a call |

If E2 finds that a client cannot be kept from going around norn, the scope does not grow to fix that: the limitation is written into the threat model and the README, and the trial reports it.

## 5. What is not in v2

- **Network, secrets and approvals.** That is v3 (D-137). In v2 the sandbox still has no network.
- **TraceWeaver** (timeline, causal graph, replay, run diff). v4.
- **A long-running service, an HTTP API, SDKs, a queue, fair share, remote workers.** v5.
- **MCP over HTTP** (the network transport). stdio only: no listening socket, nothing to authenticate.
- **norn as an MCP client** (passing calls on to other people's MCP servers with policy applied). It needs network and a sandbox for those servers; later.
- **The other parts of MCP** (resources, prompts, sampling). Only tools.
- **Several sessions on one execution**, or one session over several repositories.
- From the parking lot, still parked: shortening a long conversation for the model, pacing to a tokens-per-minute limit, a merge for `apply`, reading part of a binary file, a narrower rule for one script.

## 6. What does not change

- The five parts and the ten design rules. Rule 1 ("the agent never touches the system directly; every effect goes through the gateway") stays true *for every call that reaches norn*; for an MCP client it also depends on the client's setup (A5, E2), and the documents say so.
- One crate, synchronous Rust, no async runtime. stdio needs none.
- No new dependency is expected: the messages are JSON, and `serde_json` is already used. If E1 shows otherwise, a decision entry says why.
- The compatibility promise (`docs/compatibility.md`): everything above is an addition (a command, tools, optional arguments), so policy files and execution folders of 1.x keep working.

## 7. How v2 counts as done

- An MCP client can do a ticket from start to patch through norn, and the patch applies with `norn apply`.
- Kill cells through MCP: 200 kills per cell, 0 invariant violations, 0 duplicated effects, on `nix` and in CI on `fhs`.
- The security cases and the protocol cases pass through MCP, 0 escapes.
- The six-ticket trial with Claude Code is recorded, solved or not.
- Every item has tests; `make test` passes on both presets and on Debian in CI; each milestone has its document.

## 8. Questions only Prince can answer (answered 2026-11-26, D-139: all four milestones; yes, his account; 1.1.0)

1. **Approve the scope**: all of it, or which items to strike. (A smaller v2: M15 and M16 only, with M17 left for later.)
2. **The trial in M18 uses your Claude Code account.** Is that all right, or should the trial use the local model through an MCP-capable client instead?
3. **The version number.** "v2" is the name of the plan. If nothing incompatible is needed, semantic versioning says the release is **1.1.0**, not 2.0.0. Recommended: number by the rules (1.1.0) and keep "v2" as the plan's name; the alternative is 2.0.0 for the size of the step, with no break behind it.
