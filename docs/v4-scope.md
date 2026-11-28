# norn v4 — Scope (draft)

*Drafted 2026-11-28 (D-168). Road map: D-137 (v4 is Phase 3 of the spec: TraceWeaver).*

**Status: draft, waiting for Prince's approval.** Nothing in it is built until he approves it. The questions he needs to answer are in §8.

## 1. In one paragraph

Every execution already leaves a complete record: an append-only event log, the full output of every call in the blob store, and a checkpoint of the workspace after every call that changed it. Today `norn trace` reads that record in only two ways: one line per call, or every event raw. You can see *what* happened. You cannot easily ask *why* the agent did something, which file a call changed, where two runs went different ways, or how long each step took. v4 is TraceWeaver (spec §9.13): new ways to **read** the record that is already there. It runs no tools and changes no workspace. It adds very little to the log: one optional field that links a call to the message that asked for it, and the model's token counts. Every 1.x log stays readable.

## 2. Words used here

| Term | Plain meaning |
|---|---|
| Timeline | The events of one run in order, with how long each call took |
| Inspector | Everything about one call on one screen: what was asked, the policy rule that decided, the output, the files it changed, its network traffic, its approval |
| Causal link | A recorded id that says "this happened because of that" (this call came from that message). Spec rule: causes come from ids, never from guessing by time |
| Causal graph | All the causal links of a run drawn as a tree or graph |
| Replay | Re-running the *agent* against the recorded tool results, with no tool really run. Nothing outside changes |
| Divergence | The first point where a replayed agent asks for a call different from the recorded one |
| Run diff | Two runs side by side: calls added or removed, different arguments, different decisions, different outcomes, time and tokens |
| OpenTelemetry (OTel) | A common format for traces. Tools such as Jaeger and Grafana Tempo can show it. A *span* is one timed step with a parent |
| DOT | The text format of Graphviz. `dot -Tsvg` turns it into a picture |

## 3. What is in v4

### M23 — Timeline, inspector and security view

| # | Item | Notes |
|---|---|---|
| T1 | A written, versioned event schema: `docs/event-schema.md`, one section per event kind, with the version that added it | "Stabilize and version the event schema" (spec Phase 3). Tested against the frozen v0.1 logs and a new frozen 1.2 log (`tests/compat.rs`, D-126) |
| T2 | `norn trace <id>` gets filters: `--denied`, `--failed`, `--tool fs.write`, `--path 'src/**'`, `--uncertain` | The example in the spec's appendix (`agentos trace exec_7f3a --denied`) |
| T3 | Durations: each call's time from `tool.requested` to its ending, and the time the agent took before it | From `ts`, which is wall time for people. Order still comes from `seq` |
| T4 | `norn trace <id> --call N`: the inspector | Arguments, policy rule, intent, output (from the blob store), approval, network connections, the reconciliation if any |
| T5 | `norn trace <id> --security`: denials, attempts outside the policy that the runtime undid (`policy.violation`), refused connections, approvals asked and given | Spec §9.13 "security view" |
| T6 | `norn trace` opens the log read-only, so it can read a run that is still going | Spec §9.13 form factor: "a separate read-only consumer of the log". Experiment E1 |

### M24 — Causal links and the file-change view

| # | Item | Notes |
|---|---|---|
| C1 | `tool.requested` gets an optional `cause`: the `seq` of the `agent.message` that asked for the call. Through MCP, where norn never sees the model's messages, the cause is the client's request id | An optional field is allowed by the compatibility promise. Old logs have no `cause`. The trace then says "cause not recorded", and never guesses by order |
| C2 | The other links that ids already give, shown together: approval asked → approval used (approval id), uncertain call → its reconciliation (`call_id`), crash → recovery → resume | No new fields |
| C3 | `norn trace <id> --graph`: the causal graph as an indented tree; `--graph=dot` writes DOT for Graphviz | No new dependency. norn writes text; the person runs `dot` if they want a picture |
| C4 | `norn trace <id> --diff N`: the files that call N changed, as a diff between the checkpoint before it and the one after it. `--diff` alone gives the whole run, the same as `result.patch` | Checkpoints are kept until `norn rm`, so this works for every past run. It reuses the patch code (`checkpoint::write_patch`) |

### M25 — Replay and run diff

| # | Item | Notes |
|---|---|---|
| R1 | Token counts: the LLM agent stores each reply's `usage` (prompt and answer tokens) in the log | A new optional field. MCP clients do not tell norn their tokens, so MCP runs have none, and the trace says so |
| R2 | `norn replay <id> [--model M] [--base-url U]`: the LLM agent is run again on the same task. Every call it asks for is answered from the recording, not run. Replay stops at the first divergence and reports it: call number, recorded call, new call | Nothing is run, written or sent. The result is a report, not a new execution. A scripted run replays exactly, which tests the replay itself |
| R3 | `norn diff <id-a> <id-b>`: two runs lined up call by call: calls added or removed, changed arguments, different policy decisions and outcomes, time and token differences, and whether the two patches are the same | Works on any two runs of the same repository and task, including a replay against its original |
| R4 | The limits are printed with every replay: the model may answer differently with the same input; the environment may have changed; a replay of an MCP run is not possible, because the client's reasoning is not in the log | Spec §9.13: "replay limitations are shown explicitly" |

### M26 — Export, evidence and release

| # | Item | Notes |
|---|---|---|
| X1 | `norn trace <id> --otel > run.json`: the run as OpenTelemetry spans in the OTLP JSON format. One span for the execution, one for each call, with the parent taken from the causal links | Written to a file with `serde_json`. Nothing is sent anywhere. Experiment E4 checks that a real collector accepts it |
| X2 | `norn search --tool T --path P --outcome O [--since D]` across all executions in the state folder | Reads each log read-only. No index: the number of runs on one machine is small |
| X3 | Trial: use TraceWeaver on the recorded trials of v1 to v3 to answer three questions written down before looking ("why did the agent read X before it changed Y", "where did run A and run B go different ways", "what did this run send over the network"). Record what it could and could not answer | `docs/results/v4-trial-nix.md`. This is a test of whether the views help, not a benchmark |
| X4 | Documents (README, the event schema, threat model: the record now holds token counts), CHANGELOG, release | Version: question 5 |

## 4. Experiments before building

| # | Question | How |
|---|---|---|
| E1 | Can a second process read the SQLite log, read-only, while a run is writing it, without blocking the run or reading half an event? | `experiments/m23-readonly-log`: a writer that appends fast, plus a reader in a loop. The log is in WAL mode, which allows readers beside one writer; check that a read-only open keeps that true |
| E2 | Which questions can the logs of the earlier trials (v1 to v3) answer today, and which need C1 or R1? | Go through three recorded runs by hand. List each missing link. `experiments/m24-trial-logs` |
| E3 | With the same input, how often does the local model (llama.cpp, temperature 0) give the same tool call again? This tells if replay is useful or diverges at once | Replay one recorded run ten times by hand, through the mock or the real server. `experiments/m25-determinism` |
| E4 | Does a real OpenTelemetry collector (or Jaeger) accept the OTLP JSON that norn writes, and show the parent links? | Run one in the dev shell if `nixpkgs` has it, and send it the file with `curl`. If none can run, check against the OTLP JSON schema only, and say so. `experiments/m26-otlp` |
| E5 | How long does a per-call diff take on the largest workspace from the trials? | Time `--diff N` with the patch code on two checkpoints. `experiments/m24-call-diff` |

## 5. What is not in v4

- **A web interface or a TUI.** v4 adds text, JSON, DOT and OTLP output (question 2).
- **Comparative execution that goes on after the divergence** (a replay that switches to running tools for real in a new workspace): question 3. The draft stops at the divergence.
- **Sending traces to a collector over the network**: question 4. The draft writes a file.
- **A search index or a database across runs**: v5, with the daemon.
- **Cost in money**: token counts only. Prices change and depend on the provider (parked: cost attribution, spec §22).
- **Showing a secret's value or reading payloads in a special mode**: the log already holds no secret values (v3). There is nothing more to hide or show.

## 6. What changes, and what does not

- **Changes:** two optional fields in the log (`cause` on `tool.requested`, `usage` on `agent.message`), allowed by the compatibility promise. Three commands (`replay`, `diff`, `search`) and new options of `trace`.
- **Unchanged:** the five parts and the ten design rules. Replay runs no tool, so rules 1, 3, 4 and 5 are not touched: there is no gateway effect to guard. One crate, synchronous Rust. Every log from 1.0 onwards stays readable, and the frozen v0.1 files stay frozen.
- **Dependencies:** none expected. SQLite, JSON and diffs are already there; DOT and OTLP are text the program writes.

## 7. How v4 counts as done

- Every existing test passes unchanged; the trace of a frozen v0.1 log reads as before.
- Each view has tests on recorded logs, including logs with crashes, recovery, denials, approvals and network traffic.
- A scripted run replays with no divergence, and nothing outside the state folder changes during a replay (checked as the security suite checks it).
- A replay with a changed script stops at the first changed call, and names it.
- `norn diff` of a run against itself shows no differences.
- The OTLP file is accepted by a collector (or, if E4 finds none can run, by the schema, and the document says which).
- The trial is recorded; CI is green on both presets and Debian.

## 8. Questions only Prince can answer

1. **Approve the scope:** all four milestones, or a smaller v4. For example M23 and M24 only: reading and links, with no replay.
2. **The form:** CLI only, with text, JSON, DOT and OTLP output (recommended: no new dependency, and OTLP gives a graphical view in tools that exist already). Or also one self-contained HTML report per run (`norn trace --html`: one file, no server). Or a TUI, which needs a new dependency.
3. **Comparative execution:** stop at the divergence (recommended for v4), or go on after it and run tools for real in a new execution that starts from the checkpoint at that point. The second shows how a different model would have *finished*. It is a new way to start an execution, and it is about as large as one more milestone.
4. **OpenTelemetry:** write a file only (recommended), or also send it to a collector over HTTP with the `ureq` that norn already has.
5. **Version:** everything is optional and the log only gains optional fields, so semantic versioning gives **1.3.0** (recommended).
