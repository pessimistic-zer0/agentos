# M25 — Token counts, replay and run diff

| | |
|---|---|
| Weeks in the plan | v4 (D-169); no week plan |
| Started / finished | 2026-11-28 / 2026-11-28 on `nix`; `fhs` waits for a CI run |
| Commits | the M25 commit (tag `m25` after CI is green, D-113) |
| Status | Done. Verified on both presets and Debian 12 by CI run 46166530040 on `d68aed0`; tagged `m25` there (D-177) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

M23 and M24 read one run. M25 compares. **Token counts** say how much of the model each run used. **Replay** asks: if the same agent (or another model) were given the same task and the same answers, would it make the same calls? **Run diff** puts two runs side by side. Replay is built so that it cannot have an effect: it never reaches the gateway. It touches the **agent** (it reports token counts) and the **event log** (one optional field), and adds two commands that only read.

## 2. Requirements

From `docs/v4-scope.md`, M25 (R1 to R4), with D-171.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| R1 the LLM agent stores each reply's token counts, with the model's name; MCP runs have none, and the trace says so | Yes, `nix` | `tokens_are_recorded_and_a_replay_follows_…` (`tests/llm_agent.rs`): six assistant messages with `usage`, `model` = the server's own name, no `usage` on other messages; the replay report says "tokens not recorded" for runs without counts |
| R2 `norn replay <id> [--model M] [--base-url U]`: calls answered from the recording, nothing run; stops at the first divergence and names it; a scripted run replays exactly | Yes, `nix`, and `--script` for a scripted run | same LLM test (no divergence; the model asked six times; another model diverges; work, checkpoints, log unchanged); `a_scripted_replay_matches_its_own_script_and_stops_where_another_differs` (`tests/trace.rs`): itself, another path, another content, shorter, longer; nothing changed, no new execution |
| R3 `norn diff <a> <b>`: calls added or removed, changed arguments, other decisions and outcomes, time and token differences, same patch or not | Yes, `nix` | `a_run_diff_lines_up_the_calls_and_finds_nothing_against_itself`: no differences against itself; against another run, each kind of line; `cargo test --lib trace::rundiff` |
| R3 "including a replay against its original" | Changed | A replay is a report, not an execution; it prints that comparison itself (D-174) |
| R4 the limits printed with every replay; an MCP run cannot be replayed | Yes, `nix` | the line "Nothing is run: …" is checked in the LLM test; `an_mcp_execution_cannot_be_replayed_and_says_why` |
| Every existing test passes unchanged | Yes, `nix` | `nix develop -c make test`: 288 passed, 0 failed |
| Both presets | Yes | CI run 46166530040 on `d68aed0`: all four jobs green (D-177) |

## 3. What you need to know first

**Tokens.** A model reads and writes text in pieces called tokens (a word is one to a few). Servers that speak the OpenAI API add a `usage` object to each answer: `prompt_tokens` (what it read: the whole conversation so far) and `completion_tokens` (what it wrote). Hosted models are paid by these numbers; local ones are slower the more there are.

**Determinism.** A program is deterministic if the same input always gives the same output. norn's scripted agent is. A model at temperature 0 often is, but not always (E3), and a different model is not expected to be. Replay therefore promises nothing about how far it will get; it reports where the two first part.

**Longest common subsequence.** Given two lists, the longest list of items that appear in both, in the same order, though not always side by side. For `[a, b, c, d]` and `[a, x, c, d]` it is `[a, c, d]`. It is found with a table: the cell for "the rest of A from i, the rest of B from j" is one more than the cell diagonally after it if the two items are equal, else the larger of its two neighbours.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m25-determinism` (E3) | The same request to a local model at temperature 0, six times | The same call six times; a whole run was too slow to try on this machine | Replay reports the divergence and promises nothing (D-171) |
| `m24-trial-logs` (E2) | Lining up two runs of one ticket by their calls | 7 of 19 and 22 calls in common; readable | The LCS for `norn diff` |

## 5. What was built

| File | Purpose |
|---|---|
| `src/log.rs` | `Usage`, `Usage::from_reply`; `usage` on `AgentMessage` |
| `src/agent/mod.rs`, `llm.rs` | `take_messages` gives each message with its counts; the LLM agent keeps a reply's counts until it stores the assistant message |
| `src/runtime.rs`, `src/mcp.rs` | Store `usage` with the message |
| `src/replay.rs` (new) | `replay`: a fresh agent against the recording; `End`, the ways a replay stops; `render`; `usage_of` |
| `src/trace/rundiff.rs` (new) | `render_run_diff`: `lcs`, pairing by tool and path, totals |
| `src/main.rs` | `norn replay`, `norn diff` |
| `docs/event-schema.md` | The `usage` field |
| `tests/llm_agent.rs` | The mock server reports token counts; the R1/R2 test |
| `tests/trace.rs` | Scripted replays, the MCP refusal, run diffs |

```text
norn replay <id>
  log.db ──calls + results (blob store)──┐
  fresh agent ──next_action(last)──▶ call? ── same as recorded call i? ──yes──▶ last = recorded result i
                                                 └──no──▶ DIVERGENCE (report)          (gateway never used)
```

## 6. The code, explained

**`Usage::from_reply` (`src/log.rs`).** Reads `usage.prompt_tokens` and `usage.completion_tokens` from the server's JSON, and the server's `model` field, falling back to the name norn asked for. No `usage` object: `None`, and nothing is stored.

**`LlmAgent::push` (`src/agent/llm.rs`).** Every message goes through `push`. When the reply comes in, its counts wait in `reply_usage`; the next assistant message pushed takes them (`take()` leaves `None` behind), so a correction message norn adds afterwards never gets them. `take_messages` hands `(message, usage)` pairs to the runtime, which stores both in one event.

**`replay::replay` (`src/replay.rs`).** Reads the log read-only, refuses an MCP run, and walks the agent forward. For each call the agent asks for, it turns it into its logged form (`call.logged()`, the same form the gateway writes) and compares it, as JSON, with the recorded call at the same position. Equal: the recorded result (outcome from the log, output from the blob store) is handed back as `last`, exactly what the agent saw the first time. Not equal, or past the end, or the agent ends early: the loop stops with an `End` value saying which. The gateway, the sandbox and the state folder are never used, so there is nothing a replay could change; the tests check the folder anyway.

**`replay::render`.** The limits first (R4), then the token totals of the recording and of the replay, then how many calls matched and how it ended. For a divergence it names the call, both versions, and which fields differ (`path`; `bytes, content_hash`).

**`rundiff::lcs`.** The table described in §3, filled from the end, then walked from the start to collect the matching pairs.

**`rundiff::render_run_diff`.** The common calls are anchors. Between two anchors, a call of A and a call of B with the same tool and path become one `~` line ("other arguments"); the rest are `-` (only in A) or `+` (only in B). At each anchor the two results are compared: the same is `=`, different is `!`. A command's own time is left out of that comparison (`summary_kernel`). Then the totals: agent and tool time from M23's timing, tokens, how each run ended, and whether the two patches have the same changed lines.

**Rust features met here.** `Option::take`, to move a value out and leave `None`. A trait method whose return type changed for every implementer (`take_messages`), and the compiler finding each place that had to follow. A two-dimensional `Vec<Vec<u32>>` as the LCS table.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-171 | Counts stored with the model's name; replay promises nothing | Model name in `execution.created` |
| D-174 | M25 as built: `usage` beside the message; replay as a report outside the gateway; run diff by LCS with pairing by tool and path; R3's "replay against its original" dropped | Counts inside the message; replay as a new execution |

## 8. Problems hit

- **A wrong reason in D-171** ("the model can change at a resume"). Building R1 showed that a resume reads the model's name from `llm.json`, which has existed since v0.1. Recorded in Findings; the decision itself stands for another reason (the server's own name for the model is the fact).
- **A wrong count in a new test**: it expected six common calls between two runs; the two runs share five (calls 1, 2, 3, 5 and 7). The test's number was corrected; the program was right.
- **A moved value** in the mock server (`req` used after being stored): the count of messages is now taken before.

## 9. How to run it

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test trace --test llm_agent
nix develop -c make test
norn replay <id>                                   # the same agent again
norn replay <id> --script other.toml               # a scripted run, with another script
norn replay <id> --model other --base-url http://… # an LLM run, with another model
norn diff <id-a> <id-b>
```

## 10. What this milestone does not do

- Go on after the divergence with real tools in a new workspace (left out, D-169).
- Replay an MCP client's run.
- Count tokens for MCP clients: they do not tell the server.
- Put a price on tokens.

## 11. Check your understanding

1. Why are the token counts beside the message in the event, rather than inside it?
2. After a reply arrives, norn may add a correction message before the model's next turn. Why does that message not get the reply's counts?
3. What exactly does replay compare to decide that a call is "the same"? Why does that make two writes with different content different calls?
4. Why can a replay never change a file, even if the replayed agent asks for `exec rm -rf src`?
5. A replay of a scripted run with its own script diverges. What would that tell you?
6. Why can an MCP run not be replayed?
7. In `norn diff`, when is a pair of calls shown as `~` rather than as one `-` and one `+`?
8. Why is `exit 0, 0.2s` compared as `exit 0`?
9. Two runs end with the same changed lines but different file headers in their patches. Does `norn diff` call them the same? Why is that right?
