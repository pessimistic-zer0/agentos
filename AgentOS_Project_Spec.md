# AgentOS — Project Specification

**A secure, durable execution runtime for tool-using AI agents**

| | |
|---|---|
| Status | Living document — evolving spec, not a contract |
| Owner | Prince (`pessimistic-zer0`) |
| Last updated | 2026-11-10 |
| Working names | **AgentOS** (runtime), **TraceWeaver** (observability/forensics subsystem) |
| First workload | A coding agent that inspects a repo, edits files, runs tests, and proposes a patch |

> **Naming note.** "AgentOS" is a working name. It is not an operating system; it is an agent execution runtime plus, eventually, a control plane. Rename freely if a better name appears.

> **Status of this document: a "maybe someday" list, not the plan.** The plan is `AgentOS v1 — Scoped Design.md`. Nothing here adds work to v1, and where the two disagree the v1 document wins. Phases 2–6 may never happen, and that is fine.
>
> **v1 decisions that override this document:**
>
> - Checkpoints are directory copies (reflink where supported), not git commits (§9.6, §9.7, §15).
> - Files matching deny globs are never copied into the workspace, so deny rules also bind code run by `exec` (§9.3, §9.4).
> - After each `exec`, changes outside the write globs are reverted and logged (§9.3).
> - The workspace lives on disk with no hard size cap; disk fill is a known limit (§9.4, §9.5).
> - A call is COMPLETED only after its checkpoint exists (§9.6).
> - Read-only system folders come from policy presets (`fhs`, `nix`), not hardcoded paths (§9.4).
> - Dropped from v1 entirely: seccomp, hash-chained log, `SandboxBackend` trait, `QUEUED`/`PAUSED` states.
> - Out of scope, stated in the README: power loss, and anything that happens after the human applies the patch (§11.4).

---

## 0. How to read this document

This spec covers the whole project — from the first vertical slice to long-range research directions. It is deliberately **not** a fixed plan. Most decisions are marked with a status so it's clear what is settled and what is still a bet:

| Tag | Meaning |
|---|---|
| **[Decided]** | Settled for now. Change only with a written reason (an ADR). |
| **[Leaning]** | Current default. Expect to confirm or reverse after a prototype or measurement. |
| **[Open]** | Genuinely undecided. Options are listed; none is preferred yet. |
| **[Idea]** | Parking lot. Interesting, uncommitted, may never happen. |

Phases (§14) group work by dependency, not by calendar. Only Phase 1 (v1) has a time estimate.

### Contents

1. Vision and positioning
2. Problem statement
3. Users and use cases
4. Goals and non-goals
5. Design principles
6. Core concepts and glossary
7. Product surface
8. Architecture
9. Subsystems
10. Data model
11. Threat model
12. Reliability semantics
13. Evaluation and benchmarks
14. Roadmap by phase
15. Technology stack
16. Engineering practices
17. Landscape and positioning
18. Risks and mitigations
19. Success metrics
20. Portfolio and career framing
21. Open questions
22. Idea parking lot
23. Appendices

---

## 1. Vision and positioning

### 1.1 One-liner

AgentOS runs AI agents' tool calls inside enforced boundaries, survives crashes without losing track of what happened, and can explain afterwards exactly what the agent did.

### 1.2 Vision

A developer should be able to hand an agent real capabilities — a repository, a shell, an API token — and know three things with evidence rather than hope:

1. **Containment** — the agent could only do what the policy allowed.
2. **Continuity** — if anything crashed, nothing was silently lost or silently repeated.
3. **Accountability** — every action, decision, and denial is recorded and inspectable.

### 1.3 What AgentOS is

- An **execution layer** that sits underneath agents and mediates every side effect.
- A **policy enforcement point** that is outside the model and outside the agent's process.
- A **durable record** of execution that supports recovery, debugging, and audit.

### 1.4 What AgentOS is not

- Not an agent framework (no prompt chains, roles, or planning logic as the core product).
- Not a model-serving platform.
- Not a general workflow engine or a Kubernetes replacement.
- Not a claim of perfect isolation or exactly-once execution.

### 1.5 Core identity

| Layer | Question it answers |
|---|---|
| Runtime | *How* does an agent execute? |
| Security layer | *What* is the agent allowed to do? |
| Durability layer | *How* does execution survive failure? |
| TraceWeaver | *What happened*, and why? |

---

## 2. Problem statement

Agents increasingly call tools that have real effects: reading and writing files, running shell commands, calling APIs, modifying databases, opening pull requests. The common implementation — give the model a shell tool and run it on the developer's machine or in a loose container — leaves four classes of problems unsolved.

### 2.1 Security

- Agents can read or write outside their intended scope (path traversal, symlinks, absolute paths).
- Agents can read credentials from environment variables, dotfiles, or `/proc`.
- Agents can exfiltrate data over the network.
- Agents follow instructions embedded in untrusted content (prompt injection via repo files, web pages, tool output, tool descriptions).
- "Allowed" commands are often arbitrary code execution in disguise (`pytest` runs `conftest.py`; `npm test` runs package scripts; `cargo test` runs `build.rs`).
- Prompts asking the model to behave are not an enforcement mechanism.

### 2.2 Reliability

- Agent or runtime processes crash mid-task; in-memory state is lost.
- After restart, naive retry repeats actions that may already have succeeded.
- A timeout doesn't mean failure — the external operation may have completed.
- Partial writes leave files corrupted.
- Resumed execution may not match the real state of the environment.

### 2.3 Resource control

- Unbounded CPU, memory, disk, wall-clock time, process count.
- Unbounded token spend and API cost.
- Unbounded tool-call loops.

### 2.4 Debuggability and audit

- The agent's final answer rarely describes what it actually did.
- Developers need tool calls, arguments, results, policy decisions, file diffs, retries, resource use, and recovery events — in order, with causality.
- Security reviewers need to know what was attempted and denied, not just what succeeded.

---

## 3. Users and use cases

Personas are hypotheses; validate them by talking to real users once v1 exists.

### 3.1 Personas

| Persona | Wants | Cares most about |
|---|---|---|
| **Solo developer** running coding agents locally | Let an agent work on a repo without risking their machine | Containment, low friction, fast startup |
| **Agent builder** (framework or product author) | A safe execution backend for their agent's tools | Clean API, overhead, recovery semantics |
| **Platform / CI team** | Run agents unattended in pipelines | Resource limits, audit logs, reproducibility |
| **Security reviewer** | Understand and constrain what agents can do | Policy clarity, denial logs, threat model honesty |
| **Researcher / evaluator** | Compare agents, prompts, or models on the same tasks | Replay, run comparison, deterministic inputs |

### 3.2 Primary use case (Phase 1)

A coding agent is given a repository with a failing test. Inside AgentOS it reads files, edits source, runs tests, iterates, and produces a patch. It cannot touch anything outside the workspace, cannot reach the network, and if the runtime is killed at any moment, the execution can be recovered according to documented rules.

### 3.3 Later use cases

- Coding agent that opens a pull request (first external side effect).
- CI agent that triages failures and proposes fixes.
- Research/data agent with read-only access to datasets and a scratch area.
- Many agents working on independent tasks on one machine.
- Agents executing on remote workers with a central control plane.
- Post-incident investigation: "why did the agent delete that file?"
- Evaluation: replay 50 recorded tasks with a new model and diff behavior.

---

## 4. Goals and non-goals

### 4.1 Project-wide goals

1. Enforce permissions and resource limits outside the model. **[Decided]**
2. Make execution durable and recovery semantics explicit. **[Decided]**
3. Record a structured, causally linked history of every execution. **[Decided]**
4. Prove properties with adversarial and fault-injection tests, not claims. **[Decided]**
5. Keep the core small enough to reason about. **[Decided]**
6. Be usable by someone other than the author (docs, demo, sane defaults). **[Decided]**

### 4.2 Goals by phase (summary — detail in §14)

| Phase | Primary goal |
|---|---|
| 1 | Sandboxed single-agent execution with crash recovery, proven by tests |
| 2 | Hardening: network policy, secrets, external side effects, approvals |
| 3 | TraceWeaver: timelines, causal graph, inspection, replay, diffing |
| 4 | Integrations: MCP, SDKs, existing agents |
| 5 | Scale: concurrency, multi-agent, remote workers |
| 6 | Research directions: migration, information-flow control, policy synthesis |

### 4.3 Non-goals (project-wide)

- Building or fine-tuning models.
- Prompt engineering or agent "intelligence" as a product feature.
- Universal sandboxing across all operating systems (Linux first; others via VMs if ever).
- Exactly-once execution for arbitrary external systems.
- Replacing Kubernetes, Temporal, or general workflow engines.

### 4.4 Non-goals for v1 specifically

Dashboard, replay engine, MCP, SDKs, multi-agent, remote workers, network allowlisting, approvals, seccomp (stretch). See the v1 design doc for rationale.

---

## 5. Design principles

1. **The model is not the security boundary.** Trusted runtime code enforces every rule.
2. **Default deny.** No policy match means rejection.
3. **Least privilege.** Grant what the task needs, scoped as narrowly as practical.
4. **Assume code execution.** Any allowed command that runs project code is treated as arbitrary code execution inside the sandbox.
5. **Intent before effect.** Record what is about to happen, durably, before it happens.
6. **No false guarantees.** Uncertain outcomes are represented as uncertain. No exactly-once claims without downstream support.
7. **Enforcement close to the resource.** Kernel mechanisms over string checks.
8. **Observable by construction.** Every important operation emits a structured event; observability is not bolted on.
9. **Small, testable components.** Each subsystem has its own correctness tests.
10. **Measure before optimizing; publish how you measured.**
11. **Vertical slice first.** A working end-to-end path beats a set of unfinished abstractions.
12. **Flexible by default.** Prefer decisions that are cheap to reverse; write an ADR for those that aren't.

---

## 6. Core concepts and glossary

| Term | Definition |
|---|---|
| **Execution** | One run of one agent on one task, with an ID, a policy, a workspace, and a lifecycle. |
| **Agent** | The decision-making process (LLM loop or scripted) that proposes tool calls. Untrusted. |
| **Runtime** | The trusted process that mediates all tool calls for an execution. |
| **Tool** | A named operation with typed input/output (`fs.read`, `exec`, `git.commit`, `github.pr.create`…). |
| **Tool call** | One invocation of a tool, with its own state machine and record. |
| **Policy** | Declarative rules defining what an execution may do and its resource limits. |
| **Capability** | A specific, scoped permission granted by policy (e.g., write to `src/**`). |
| **Intent** | A durable record of a tool call written *before* its effect begins. |
| **Effect** | The change a tool call makes to the workspace or the outside world. |
| **Workspace** | The filesystem area an execution may operate on. |
| **Checkpoint** | A consistent snapshot of execution state + workspace at a log position. |
| **Event** | An immutable, structured record of something that happened. |
| **Event log** | The append-only, ordered sequence of events for an execution. |
| **Uncertain** | The state of a tool call whose effect may or may not have happened. |
| **Reconciliation** | Determining the true outcome of an uncertain call. |
| **Replay** | Re-running or re-presenting an execution using recorded inputs/outputs. |
| **Sandbox** | The OS-level isolation environment in which untrusted code runs. |
| **Control plane** | The component that accepts requests and manages executions (grows in later phases). |

---

## 7. Product surface

How people interact with AgentOS. Most of this is **[Open]** beyond the CLI.

### 7.1 CLI **[Decided for v1]**

Illustrative command set (names flexible):

```
agentos run    --policy policy.toml --repo ./demo --agent scripted|llm --task "..."
agentos status [<execution-id>]
agentos trace  <execution-id> [--json] [--calls-only] [--denied]
agentos recover [<execution-id>]
agentos diff   <execution-id>              # workspace changes (later)
agentos replay <execution-id> [--mode ...] # later (Phase 3)
agentos policy check policy.toml           # validate a policy (later)
agentos gc                                 # clean old executions (later)
```

Output should be human-readable by default and machine-readable with `--json`.

### 7.2 Policy file **[Leaning: TOML]**

Human-written, versioned, reviewable in a PR. See §9.3 and Appendix A.

### 7.3 Programmatic interfaces **[Open]**

| Option | Pros | Cons | When |
|---|---|---|---|
| Rust library API | No extra layer; used by the CLI anyway | Rust-only | Phase 1 (internal) |
| Local HTTP or gRPC API | Language-agnostic; enables SDKs and UI | Auth, versioning, another surface to secure | Phase 3–4 |
| Python SDK | Matches where agent builders live | Maintenance; must wrap a stable API | Phase 4 |
| TypeScript SDK | Large agent ecosystem | Same | Phase 4+ |
| AgentOS-as-MCP-server | Any MCP-capable agent can use AgentOS tools with no SDK | Tied to MCP semantics | Phase 4 — see §9.10 |

### 7.4 Visual interface **[Open, Phase 3+]**

Options: TUI (`ratatui`) for local inspection, or a web UI (React/Next.js + a graph library). Leaning: TUI or static HTML report first, full web UI only if there's demand. The data model must be stable before any UI work.

### 7.5 Illustrative developer experience (long-term, not committed)

```python
from agentos import Runtime, Policy

rt = Runtime()
policy = Policy.load("policies/coding-agent.toml")

execution = rt.start(
    repo="./my-project",
    policy=policy,
    agent="my_agent:run",          # or an MCP-speaking agent
    task="Fix the failing tests.",
)

for event in execution.events(follow=True):
    print(event.type, event.summary)

result = execution.wait()
print(result.status, result.patch_path)
```

---

## 8. Architecture

### 8.1 Logical layers

```
                 Developer · CLI · SDK · UI
                            │
                    Control plane            ← thin in v1; grows in Phase 5
                            │
        ┌───────────────────┼───────────────────┐
   Lifecycle mgr       Policy engine       Resource mgr
        └───────────────────┼───────────────────┘
                            │
                 Execution runtime (trusted)
        ┌───────────────────┼───────────────────┐
   Agent adapter      Tool gateway          Recovery
                            │
                 ┌──────────┴──────────┐
            In-process tools      Sandboxed executor
            (fs via openat2)      (namespaces, cgroups, Landlock)
                            │
                 Durable execution layer
        ┌───────────────────┼───────────────────┐
     Event log         Checkpoints         Blob store
                            │
                 TraceWeaver (Phase 3)
         timelines · causal graph · replay · diff · audit
```

### 8.2 Trust boundaries **[Decided]**

| Zone | Trusted? | Contents |
|---|---|---|
| Runtime process | Yes | Gateway, policy, log writer, recovery, checkpointer |
| State directory | Yes (host-only) | Event log, checkpoints, blobs — never mounted into the sandbox |
| Agent process | No | LLM loop or scripted agent; talks only to the gateway |
| Sandbox | No | Child processes from `exec`; sees only the workspace and read-only toolchain |
| LLM provider | Partially | Treated as a source of untrusted text |
| External services | Out of scope for trust | Reached only through brokered tools (Phase 2+) |

### 8.3 Where the agent runs **[Open]**

| Option | Description | Trade-off |
|---|---|---|
| A. In-runtime | Agent loop is a module in the runtime process | Simplest; agent bugs can crash the runtime; fine for v1 with a scripted/LLM loop written by us |
| B. Separate process, IPC to gateway | Agent is a child process speaking a JSON protocol over a socket/pipe | Clean boundary; enables third-party agents; small protocol to design |
| C. Agent inside the sandbox | Agent itself is sandboxed; only the gateway socket is exposed | Strongest; needed for running untrusted third-party agent code |

Leaning: A for v1, B in Phase 2, C when third-party agents are supported.

### 8.4 Deployment shapes over time

| Shape | Phase | Description |
|---|---|---|
| Single binary, local | 1–3 | One process per execution or a long-running local daemon |
| Local daemon + clients | 3–4 | Daemon owns state; CLI/SDK/UI talk to it |
| Control plane + workers | 5 | Central scheduler and storage; executions on remote workers |

---

## 9. Subsystems

Each subsystem lists responsibilities, the v1 approach, later options, and open questions.

### 9.1 Lifecycle manager

**Responsibilities:** create, start, pause, resume, cancel, time out, complete, fail executions; validate transitions; persist them.

**Execution states [Leaning]:**

```
CREATED → QUEUED → STARTING → RUNNING ─┬→ COMPLETED
                                       ├→ FAILED
                                       ├→ CANCELLED
                                       ├→ TIMED_OUT
                                       ├→ PAUSED ⇄ (RESUMING → RUNNING)
                                       └→ INTERRUPTED → RECOVERING → RUNNING | NEEDS_ATTENTION
```

- Separate **process state** (is the runtime/agent alive?) from **task state** (is the work recoverable?). A dead process can leave a recoverable task.
- `INTERRUPTED` is detected on startup (an execution marked running with no live owner).
- Invalid transitions are rejected and logged.
- Cancellation ≠ failure. Recovery never upgrades uncertainty to success.

**Later:** pause/resume via cgroup freezer; priorities and queueing (Phase 5); leases/heartbeats for remote ownership (Phase 5).

**Open:** Is `QUEUED` meaningful before there's a scheduler? (Probably skip until Phase 5.)

### 9.2 Tool gateway

**Responsibilities:** the single path from agent to effect. Parse → validate → identify execution → evaluate policy → check budget → write intent → dispatch → capture result → record completion → return.

**Tool set by phase:**

| Phase | Tools |
|---|---|
| 1 | `fs.read`, `fs.write`, `fs.list`, `exec` |
| 2 | `fs.patch` (apply diff), `fs.delete`, `fs.move`, `git.*` (status, diff, commit on a branch), `http.fetch` (allowlisted) |
| 2–4 | `github.pr.create` and similar external tools via brokered credentials |
| 4 | Arbitrary MCP tools via adapter |

**Tool definition shape [Leaning]:** name, version, input schema, output schema, *effect class*, *recovery class*, required capabilities, default limits.

**Effect classes [Leaning]:**

| Class | Example | Implication |
|---|---|---|
| `pure` | `fs.read` | Free to retry |
| `workspace` | `fs.write`, `exec` | Reversible via checkpoint |
| `external-idempotent` | API with idempotency keys | Retry with the same key |
| `external-reconcilable` | Create PR (can search for it afterwards) | Query to determine outcome |
| `external-irreversible` | Send email, payment | Needs approval; uncertain → human |

**Open:** Should tool schemas be JSON Schema (MCP-compatible) from day one? (Leaning: yes, cheap insurance.)

### 9.3 Policy and capabilities

**Responsibilities:** define and evaluate what an execution may do.

**v1 [Leaning]:** a TOML file with filesystem allow/deny globs (read, write), argv-prefix command allowlist, limits.

**Evolution options [Open]:**

| Option | Notes |
|---|---|
| Keep a small custom TOML schema | Easiest to explain; enough for coding agents |
| Adopt an existing policy language (e.g., Cedar, OPA/Rego) | Expressive and analyzable; more learning curve and dependency weight |
| Capability tokens | Unforgeable handles passed per call; elegant; more design work |
| Policy packs | Reusable templates per agent type (coding, CI, research) |

**Requirements regardless of format:**

- Versioned schema (`policy_version = 1`).
- Paths matched **after** kernel-confined resolution, never as raw strings.
- Commands as argv arrays, never shell strings.
- Environment variables allowlisted, not blocklisted.
- Every decision recorded with the rule that matched.
- `agentos policy check` validates and explains a policy.
- Later: per-tool argument constraints (e.g., `git push` only to `agent/*` branches).

**Open:** How are policy changes mid-execution handled? (Leaning: forbidden; a new policy = a new execution.)

### 9.4 Sandbox and isolation

**Responsibilities:** contain untrusted code even if it gains arbitrary execution inside the sandbox.

**v1 mechanisms [Leaning]:**

| Concern | Mechanism |
|---|---|
| Filesystem view | Mount namespace; workspace rw at `/work`; toolchain ro; tmpfs `/tmp`; no `/home` |
| Path confinement (in-runtime tools) | `openat2` with `RESOLVE_BENEATH` / `RESOLVE_NO_MAGICLINKS` |
| Defence in depth | Landlock filesystem rules |
| Privilege | User namespace, `PR_SET_NO_NEW_PRIVS`, capabilities dropped |
| Processes | PID namespace; kill the whole cgroup on timeout |
| Network | Empty network namespace |
| Resources | cgroup v2: `memory.max`, `pids.max`, `cpu.max`; size-capped workspace |
| Environment | Cleared; minimal allowlist |

**Later options [Open]:**

| Option | Isolation strength | Cost | Notes |
|---|---|---|---|
| DIY namespaces (v1) | Shared kernel | Very low startup | Best learning value; kernel bugs are out of scope |
| Wrap bubblewrap / nsjail | Shared kernel | Low | Battle-tested setup; less to get wrong |
| seccomp-bpf filter | Reduces kernel attack surface | Low | Stretch for v1; good Phase 2 item |
| gVisor (`runsc`) | User-space kernel | Moderate overhead | Stronger boundary without VMs |
| Firecracker / microVMs | Hardware virtualization | Higher startup, more ops | Strongest; standard for multi-tenant hosted sandboxes |
| Pluggable backend trait | — | Design effort | Lets backends be swapped and compared in benchmarks |

Leaning: define a `SandboxBackend` trait early, ship the DIY backend, and treat stronger backends as Phase 5–6 options benchmarked against it.

**Platform:** Linux only **[Decided]**. macOS/Windows users run AgentOS in a Linux VM if ever needed **[Idea]**.

### 9.5 Resource management and budgets

| Resource | Phase | Enforcement |
|---|---|---|
| Memory, PIDs, CPU | 1 | cgroup v2 |
| Wall-clock per call / per execution | 1 | Watchdog kills the cgroup |
| Disk | 1 | Size-capped workspace (tmpfs or quota) |
| Tool-call count | 1 | Gateway counter |
| Output size per call | 1 | Truncate and record truncation |
| LLM tokens / cost | 2 | Agent adapter counts usage from provider responses |
| API spend per external tool | 2–4 | Per-tool budgets in policy |
| Fair share across executions | 5 | Scheduler + cgroup weights |

Budget exhaustion is an explicit terminal reason, not a generic failure.

### 9.6 Durable execution

The technical heart of the project.

**Event log [Leaning: SQLite, WAL mode]:**

- Append-only from the runtime's point of view (INSERT only).
- Intents written with `synchronous=FULL` (or explicit checkpoint/fsync) before the effect begins.
- Lives in the host-only state directory.
- Optional hash chain (each event stores the previous event's hash) to detect truncation or corruption **[Idea → Phase 2]**.

**Alternatives [Open]:** a custom segmented append-only file (more control, more work, good learning), or a log-structured format later for high throughput. Leaning: SQLite until a benchmark says otherwise.

**Tool-call state machine [Leaning]:**

```
REQUESTED ─┬─ policy deny ─→ DENIED
           └─ allow ─→ INTENT_LOGGED ─(fsync)→ STARTED ─┬→ COMPLETED
                                                       └→ FAILED
crash while INTENT_LOGGED or STARTED → UNCERTAIN → APPLIED | NOT_APPLIED | NEEDS_HUMAN
```

**Recovery procedure [Leaning]:**

1. Open the log; verify integrity (no torn records; chain if enabled).
2. Mark executions with no live owner as `INTERRUPTED`.
3. For each tool call with an intent but no terminal record → `UNCERTAIN`.
4. Reconcile by effect class (§9.2, §9.11).
5. Restore the workspace to the right checkpoint if needed.
6. Rebuild agent context from logged messages; append synthetic results for reconciled calls.
7. Resume, or stop at `NEEDS_ATTENTION`.
8. Recovery itself is idempotent and re-entrant (it can crash and be re-run).

**Checkpoints:** see §9.7. A checkpoint records: log position, workspace snapshot ID, execution/task state, pending calls, resource counters, and (optionally) agent conversation state.

**Invariants (tested in §13.2):**

1. No effect without a prior durable intent.
2. No COMPLETED record without its effect.
3. After recovery, workspace = checkpoint + exactly the APPLIED calls after it.
4. No UNCERTAIN call re-executed before reconciliation.
5. Log is a gap-free prefix.

### 9.7 Workspace management

**Options [Open]:**

| Option | Snapshot cost | Restore cost | Notes |
|---|---|---|---|
| Copy repo to a scratch dir + git commits in a separate `GIT_DIR` | Proportional to changes | Fast (`git checkout`) | **[Leaning for v1]** Simple, inspectable, diffs for free |
| overlayfs (lower = original repo, upper = changes) | Near zero | Discard upper dir | Elegant; needs care with rootless overlay |
| btrfs/ZFS snapshots | Near zero | Near zero | Filesystem-dependent |
| Content-addressed tree in the blob store | Proportional to changes | Rebuild tree | Most portable; most code |

**Rules:** the agent's view never includes the snapshot store; the original repo is never modified in place; the final output is a patch or branch the human applies.

### 9.8 Network control

| Phase | Approach |
|---|---|
| 1 | No network (empty net namespace) |
| 2 | Allowlisted egress via a runtime-owned proxy (HTTP CONNECT/SNI host filtering), reachable through a socket mounted into the sandbox; DNS resolved by the proxy |
| 2+ | Package-registry access via a caching mirror (e.g., pre-fetched dependencies) rather than raw internet |
| Later [Idea] | Landlock network rules (newer kernels) as defence in depth; per-request logging into the event log |

**Open:** TLS interception for content inspection? (Leaning: no — log hosts and sizes, not contents.)

### 9.9 Secrets and credentials

**Principle [Decided]:** secrets never enter the sandbox or the agent's context.

**Approach [Leaning, Phase 2]:** credential brokering. The agent calls a tool like `github.pr.create`; the runtime attaches the token on the host side. Alternatively, the network proxy injects auth headers for specific hosts. Secrets are referenced by name in policy and redacted from logs by value matching as a backstop.

**Open:** where secrets come from (env of the runtime, OS keyring, a file, a vault).

### 9.10 Agent integration

How agents connect to AgentOS. **[Open]** beyond v1.

| Option | Description | Phase |
|---|---|---|
| Built-in agent loop | Minimal LLM loop against an OpenAI-compatible endpoint (local llama.cpp server or hosted) | 1 |
| Scripted agent | Deterministic sequence of tool calls for tests and fault injection | 1 |
| Agent protocol over IPC | Small JSON-RPC protocol; any language can implement an agent | 2–3 |
| AgentOS as an MCP server | Expose gateway tools over MCP so existing MCP-capable agents use AgentOS as their tool backend | 4 |
| MCP client adapter | AgentOS proxies third-party MCP servers, applying policy and logging to their calls | 4 |
| Wrapping existing CLI agents | Run an existing coding agent inside the sandbox, observing it externally | [Idea] — weaker mediation, but useful for audit |

**Rule [Decided]:** MCP is an integration, never a hard dependency of the core.

**MCP-specific concerns:** tool descriptions are untrusted input; tool schemas can change between sessions (record and diff them); third-party MCP servers are themselves potential attackers and may need their own sandbox.

### 9.11 External side effects and idempotency (Phase 2+)

- Every external tool declares an effect class (§9.2).
- Idempotency keys derived from `execution_id + call_id` when the service supports them.
- Reconcilable tools implement a `reconcile(intent) -> Applied | NotApplied | Unknown` function (e.g., search for a PR with a marker in its body).
- Irreversible tools require approval (§9.12) and go to `NEEDS_HUMAN` when uncertain.
- The project never claims exactly-once; it claims *effectively-once where the downstream allows it, and visible uncertainty otherwise*.

### 9.12 Approvals and governance (Phase 2+)

- Policy marks tools or argument patterns as `requires_approval`.
- The approval request shows the exact operation (tool, arguments, diff).
- Approvals are scoped to one operation (or a bounded capability), expire, and are recorded with approver identity.
- Permissions are revalidated at execution time, not just approval time.
- Channels: CLI prompt first; later webhook/Slack/GitHub comment **[Idea]**.
- Organization-wide policy layering (org defaults ∩ project policy) **[Idea, Phase 5]**.

### 9.13 TraceWeaver — observability and forensics (Phase 3)

Consumes AgentOS events; does not introduce separate instrumentation **[Decided]**.

**Features (pick and order freely):**

| Feature | Description |
|---|---|
| Timeline | Chronological events with durations, filters (denied, failed, by tool) |
| Causal graph | Explicit parent/child links: user request → agent message → tool call → result → next decision |
| Tool-call inspector | Inputs, outputs, policy decision + matched rule, resource usage, timing |
| File-change view | Per-call and cumulative diffs from checkpoints |
| Recorded-interaction replay | Re-present an execution using stored tool results; no side effects repeated |
| Comparative execution | Re-run with a different model/prompt/policy/tool impl while holding recorded inputs fixed |
| Run diff | Calls added/removed, argument changes, different decisions, different outcomes, latency/token deltas |
| Security view | Denials, attempts outside policy, sensitive-path access, schema changes, suspicious outputs |
| Search | Across historical executions (by tool, path, outcome) |
| Export | OpenTelemetry-compatible spans for existing observability stacks |

**Rules:** causality comes from recorded IDs, never inferred from timestamps; sensitive payloads are redacted by default and access to them is itself controlled; replay limitations (LLM nondeterminism, changed environment) are shown explicitly.

**Form factor [Open]:** in-process module, separate reader process over the same SQLite file, or a service. Leaning: a separate read-only consumer of the log.

### 9.14 Multi-agent execution (Phase 5)

Only after single-agent recovery is reliable **[Decided]**.

- Per-execution isolation (separate workspace, cgroup, namespaces).
- Admission control and queueing; priorities; fair scheduling.
- Shared read-only artifacts by content hash.
- Inter-agent communication only through the runtime, with its own policy and logging.
- Contention/deadlock detection for shared resources **[Idea]**.

### 9.15 Remote workers (Phase 5)

```
            Control plane (API · scheduler · metadata DB)
                              │
           ┌──────────────────┼──────────────────┐
        Worker 1           Worker 2           Worker 3
        sandbox(es)        sandbox(es)        sandbox(es)
           └──────────────────┼──────────────────┘
                  Shared storage (events · blobs · snapshots)
```

New problems: worker registration, leases and heartbeats, fencing tokens to stop zombie workers writing, artifact transfer, distributed recovery, metadata in PostgreSQL. This is a distributed-systems project in its own right — treat it as optional.

### 9.16 Snapshots and migration (Phase 6, research)

Moving a live execution between workers requires handling process state (CRIU-style checkpoint/restore), open file descriptors, pending calls, network connections, model conversation state, and external effects. Treat as research; a simpler, useful version is *task-level* migration: stop at a checkpoint, move workspace + log, restart the agent from logged context.

---

## 10. Data model

All schemas are **[Leaning]** and versioned.

### 10.1 Event envelope

| Field | Type | Notes |
|---|---|---|
| `schema_version` | int | Envelope version |
| `event_id` | ULID/UUIDv7 | Sortable unique ID |
| `execution_id` | string | |
| `seq` | int | Monotonic per execution; gap detection |
| `type` | string | e.g., `tool.invocation.started` |
| `ts` | RFC 3339 | Wall clock (for display only) |
| `mono_ns` | int | Monotonic clock (for durations) |
| `parent_event_id` | ULID? | Explicit causality |
| `call_id` | string? | Groups events of one tool call |
| `actor` | enum | `runtime`, `agent`, `sandbox`, `human` |
| `payload` | JSON | Type-specific, redacted |
| `blob_refs` | list | Content hashes for large payloads |
| `prev_hash` | hash? | Optional chain |

### 10.2 Event types (starting catalog)

- `execution.created / started / paused / resumed / interrupted / completed / failed / cancelled / timed_out`
- `agent.message` (LLM request/response metadata; content stored as blob)
- `tool.discovered`, `tool.schema.changed`
- `tool.invocation.requested / denied / intent_logged / started / completed / failed / uncertain / reconciled`
- `policy.evaluated` (decision + matched rule)
- `budget.exhausted`
- `checkpoint.created / restored`
- `recovery.started / completed`
- `approval.requested / granted / denied / expired` (Phase 2)
- `network.request` (Phase 2)
- `artifact.created`
- `resource.sampled`

### 10.3 Storage layout (local) [Leaning]

```
~/.local/share/agentos/
├── agentos.db            # SQLite: executions, tool_calls, events, checkpoints
├── blobs/ab/cd/<sha256>  # content-addressed payloads (file contents, outputs, messages)
├── snapshots/<exec>/     # GIT_DIR or overlay upper dirs
└── workspaces/<exec>/    # live workspaces mounted as /work
```

### 10.4 What is stored vs. hashed vs. dropped [Open]

| Data | Default |
|---|---|
| Tool arguments | Stored, redacted |
| File contents written | Blob (needed for reconciliation and diffs) |
| Command output | Blob, size-capped |
| LLM prompts/responses | Blob, configurable (privacy) |
| Secrets | Never stored; redacted by value |
| Environment variables | Names only |

Retention and garbage collection policies are **[Open]**.

---

## 11. Threat model

### 11.1 Assets

Host filesystem; credentials and environment; network; other executions' workspaces; the event log, checkpoints and blobs; host resources; external services reachable via tools; execution history contents (may contain sensitive data).

### 11.2 Adversaries and failure sources

- The agent itself (mistaken or manipulated).
- Prompt injection via repo files, tool outputs, web content, tool descriptions.
- Malicious or compromised MCP servers / tools (Phase 4).
- Malicious user-supplied tasks.
- Malicious repository contents (build scripts, test hooks).
- Runtime bugs; crashes; partial failures; disk-full conditions.

### 11.3 Security goals by phase

| Goal | Phase |
|---|---|
| No file access outside the workspace (incl. symlink/TOCTOU) | 1 |
| No host secrets reachable | 1 |
| No network egress | 1 |
| Resource exhaustion contained | 1 |
| Agent cannot tamper with its own history | 1 |
| Every decision recorded | 1 |
| Controlled egress to allowlisted hosts | 2 |
| Credentials usable without being readable | 2 |
| High-risk actions gated by approval | 2 |
| Third-party tools/MCP servers contained and audited | 4 |
| Executions isolated from each other | 5 |
| Tamper-evident logs | 2+ |

### 11.4 Explicit out-of-scope (stated in every release)

Kernel vulnerabilities (shared-kernel backends); a malicious host user or root; hardware side channels; a malicious LLM provider; data the user intentionally exposes via policy; guarantees from external services that don't support idempotency. A hash chain makes tampering *detectable*, not impossible, against an attacker who controls storage.

### 11.5 Prompt injection stance

AgentOS does not try to detect prompt injection in text. It limits the *damage* any injected instruction can cause by bounding capabilities, recording everything, and gating irreversible actions. Information-flow ideas (§22) are research directions.

---

## 12. Reliability semantics

What AgentOS promises — the table users should read.

| Situation | Guarantee |
|---|---|
| Runtime killed at any point | Execution can be recovered to a consistent, documented state |
| Pure tool call in flight | Safely retried |
| Workspace tool call in flight | Workspace restored to a known checkpoint; call marked NOT_APPLIED (or APPLIED if verifiable by hash) |
| Idempotent external call in flight | Retried with the same key |
| Reconcilable external call in flight | Outcome determined by query |
| Irreversible external call in flight | Marked NEEDS_HUMAN; never silently retried |
| Disk full / log write fails | Execution stops before the effect; no effect without an intent |
| LLM call in flight | Re-issued (no side effects); response may differ |
| Exactly-once for arbitrary external systems | **Not provided** |

---

## 13. Evaluation and benchmarks

Principle: define workload and baseline first, then measure, then publish scripts, raw data, hardware, kernel version, and limitations.

### 13.1 Security correctness

- Adversarial suite: path traversal variants, absolute paths, symlinks, hardlinks, symlink-swap races, `/proc` reads, env leaks, fork bombs, memory/disk exhaustion, infinite loops, network attempts, log tampering, setuid binaries, device nodes; later, malicious MCP servers and policy bypass attempts.
- Benign suite: normal operations that must succeed (false-denial rate).
- Metrics: escapes (target 0), false denials (target 0), containment of resource violations.
- Caveat stated with results: passing tests ≠ proven secure.

### 13.2 Recovery correctness

- Crash points: before intent; after intent before effect; mid-effect; after effect before completion record; after completion before agent sees result; during checkpoint; during recovery.
- Methods: deterministic `abort()` at named points + random `SIGKILL`; later, disk-full and fsync-failure injection.
- Metrics: invariant violations, duplicated side effects, lost state, uncertain calls correctly classified, recovery time p50/p99.

### 13.3 Performance and overhead

| Metric | Definition |
|---|---|
| Startup latency | Create request → ready |
| Sandbox spawn | `exec true` vs direct `fork/exec` |
| Tool-call overhead | Mediated latency − direct baseline |
| Event throughput | Events/s persisted (with and without fsync) |
| Checkpoint cost | Time and storage per checkpoint vs workspace size |
| End-to-end | Task wall time with vs without AgentOS |
| Concurrency (Phase 5) | Executions sustained under a fixed resource budget |

Report p50/p95/p99. Baselines: direct execution; plain container (`docker run`); AgentOS with recording/checkpointing toggled; later, alternative sandbox backends.

### 13.4 Replay correctness (Phase 3)

Percentage of recorded tool results replayed exactly; event-order consistency; workspace reconstruction accuracy; side effects avoided during replay. Never a single vague "determinism" score — specify what is reproduced.

### 13.5 Agent task benchmarks [Idea]

Run a small set of coding tasks (self-made, or a subset of a public benchmark) to show AgentOS doesn't break real agent workflows and to measure overhead on realistic workloads.

---

## 14. Roadmap by phase

Phases are ordered by dependency. Durations are rough and will move with coursework and interest.

### Phase 0 — Foundations (≈1 week)

- Rust ramp-up (ownership, error handling, modules, testing).
- Read: bubblewrap source, nsjail docs, Landlock docs, `openat2(2)`, cgroup v2 docs, SQLite WAL docs.
- Write: one-page threat model, the v1 design doc, first ADRs.

### Phase 1 — v1 core (≈10 weeks) — *see "AgentOS v1 — Scoped Design"*

Vertical slice → sandbox → durability and recovery → LLM agent loop → benchmarks → docs and release `v0.1`.

**Exit:** fault-injection suite green, 0 escapes on 30+ adversarial cases, published benchmarks, reproducible demo.

### Phase 2 — Hardening and real-world effects

Candidates (choose by interest):

- seccomp filter; hash-chained log; disk-full/fsync-failure injection.
- Network allowlist proxy; dependency mirror.
- Credential brokering; secret redaction.
- Git tools and the first external tool (open a PR) with reconciliation.
- Approval workflow (CLI).
- Agent as a separate process with an IPC protocol.
- Token and cost budgets.

### Phase 3 — TraceWeaver

- Stabilize and version the event schema.
- CLI/TUI timeline and inspector → optional web UI.
- Causal graph; file-change view; security view.
- Recorded-interaction replay; run diff; comparative execution.
- OpenTelemetry export.

### Phase 4 — Integrations and SDKs

- Local daemon + API.
- Python SDK (TypeScript later if useful).
- AgentOS as an MCP server; MCP client adapter with policy and sandboxing of third-party servers.
- Policy packs for common agent types.
- Example integrations with one or two popular agent frameworks.

### Phase 5 — Scale

- Concurrent executions; scheduler; admission control; fairness.
- Multi-agent coordination through the runtime.
- Remote workers, leases, fencing, PostgreSQL metadata, shared blob storage.
- Organization-level policies and governance.

### Phase 6 — Research directions (optional)

- Alternative sandbox backends (gVisor, microVMs) with comparative benchmarks.
- Live or task-level migration.
- Information-flow control for agent tool calls.
- Policy synthesis from observed traces.

### Dependency sketch

```
Phase 0 ─→ Phase 1 ─┬─→ Phase 2 ─┬─→ Phase 4 ─→ Phase 5
                    └─→ Phase 3 ─┘                 │
                                                   └─→ Phase 6
```

Phases 2 and 3 can interleave; TraceWeaver mainly needs a stable event schema from Phase 1.

---

## 15. Technology stack

| Area | Choice | Status | Alternatives |
|---|---|---|---|
| Core language | Rust | **[Decided]** | C++ (more familiar), Go (what many container tools use) |
| Concurrency | Synchronous + threads in v1 | **[Leaning]** | Tokio once there's a daemon/API (Phase 3–4) |
| Syscalls | `nix`, raw syscalls where needed | **[Leaning]** | `rustix` |
| Filesystem confinement | `landlock` crate + `openat2` | **[Leaning]** | — |
| Storage | SQLite (`rusqlite`, WAL) | **[Leaning]** | Custom append-only log; PostgreSQL in Phase 5 |
| Snapshots | git via CLI | **[Leaning]** | `gix`/`git2`, overlayfs, btrfs |
| Serialization | `serde`, JSON events, TOML policy | **[Leaning]** | MessagePack/CBOR for blobs |
| CLI | `clap` | **[Leaning]** | — |
| LLM client | Blocking HTTP to OpenAI-compatible APIs | **[Leaning]** | Provider SDKs |
| Testing | `cargo test`, custom fault-injection harness, `proptest` | **[Leaning]** | — |
| Benchmarks | `criterion`, `hyperfine` | **[Leaning]** | — |
| UI (Phase 3+) | TUI (`ratatui`) or web (React/Next.js) | **[Open]** | Static HTML report |
| Telemetry export | OpenTelemetry | **[Leaning]** | — |
| Dev environment | Nix flake | **[Idea]** | Reproducible kernels/tooling for CI |

Avoid Kafka, Redis, Kubernetes, or distributed databases until a measured need exists **[Decided]**.

---

## 16. Engineering practices

### 16.1 Repository structure (grows over time)

```
agentos/
├── src/ (single crate in v1; split when boundaries hurt)
├── tests/
│   ├── security/
│   ├── recovery/
│   └── integration/
├── benches/
├── examples/
│   └── demo-repo/
├── docs/
│   ├── architecture.md
│   ├── threat-model.md
│   ├── recovery-semantics.md
│   ├── event-schema.md
│   ├── benchmark-methodology.md
│   └── decisions/   (ADRs)
└── (later) sdk/, integrations/mcp/, ui/
```

### 16.2 Practices

- **ADRs** for every hard-to-reverse decision: context, options, decision, consequences.
- **Tests before claims:** no feature counts as done without tests; no number in the README without a script.
- **CI:** unit tests, security suite, a short fault-injection run on every push; the long run nightly or on demand. Sandbox tests need a runner with user namespaces enabled.
- **Versioning:** semver for the CLI; separate versions for the policy schema and the event schema.
- **Changelog** per release.
- **License [Open]:** MIT or Apache-2.0 (Apache-2.0 adds a patent grant).
- **Documentation:** README with a 5-minute quickstart, architecture diagram, guarantees table (§12), known limitations.
- **Security disclosures:** a `SECURITY.md` once others use it.

---

## 17. Landscape and positioning

Verify current details before quoting any of this publicly; this space changes quickly.

| Category | Examples | Relationship to AgentOS |
|---|---|---|
| Hosted agent sandboxes | E2B, Daytona, Modal sandboxes | Strong isolation as a service; AgentOS is local-first and focuses on recovery semantics and audit |
| Agent-platform runtimes | OpenHands' runtime, various coding-agent sandboxes | Usually container-based execution for one framework; AgentOS aims to be framework-agnostic |
| Sandboxing primitives | bubblewrap, nsjail, gVisor, Firecracker | Building blocks; AgentOS may use or wrap them |
| Durable execution engines | Temporal, Restate, DBOS, Inngest | General workflow durability; AgentOS specializes the idea for agent tool calls with sandboxing and uncertain-effect handling |
| Agent frameworks with checkpointing | LangGraph and similar | Checkpoint the agent graph's state, not OS-level effects |
| LLM observability | LangSmith, Langfuse, Arize Phoenix | Trace LLM calls; TraceWeaver focuses on effects, policy decisions and recovery |

**Positioning:** the intersection — enforced isolation + effect-aware durability + forensic history — for agents, built small enough to reason about and tested adversarially.

---

## 18. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Becomes an agent framework | Agent stays minimal; the runtime is the product |
| Scope explosion | Phases gated by exit criteria; v1 cut list enforced |
| Superficial security | Kernel mechanisms + adversarial suite; threat model in every release |
| Wrong recovery guarantees | State machine + invariants + fault injection; guarantees table |
| UI before data model | Event schema versioned and stable before UI work |
| Benchmark theater | Scripts, raw data, hardware, baselines published |
| Rust learning curve | Synchronous v1; small crate; Phase 0 ramp |
| Losing motivation mid-project | Every milestone ends in something runnable and demo-able |
| Kernel feature availability (Landlock/openat2/cgroup delegation) | Feature detection at startup with clear errors; document minimum kernel |
| Market already has similar products | Differentiate on recovery semantics and testing rigor; value as a portfolio piece doesn't depend on novelty |

---

## 19. Success metrics

### 19.1 Engineering

- 0 escapes on the adversarial suite at each release.
- 0 invariant violations across the full fault-injection matrix.
- Documented overhead (startup, per-call, end-to-end) with baselines.
- A stranger can run the demo from a clean clone in under 10 minutes.

### 19.2 Adoption [Open — only if it becomes a public project]

- External users who run it; issues and PRs from others; integrations built by others.

### 19.3 Personal

- Depth to discuss namespaces, cgroups, crash consistency, idempotency, and threat modelling in interviews.
- Resume bullets backed by measured numbers.

---

## 20. Portfolio and career framing

- Lists on the resume only once Phase 1 exit criteria are met; replaces Drishti, later PromptShield.
- Talking points: why the allowlist isn't the boundary; intent-before-effect; why exactly-once is impossible in general; how the fault-injection harness found real bugs (record them as they happen — they're the best interview stories).
- Bullet templates live in the v1 doc; fill only with measured numbers.
- Write-ups: a blog post per phase (e.g., "Crash-testing an agent runtime 1,000 times") **[Idea]**.

---

## 21. Open questions

Grouped; resolve via prototypes and ADRs.

**Architecture**
- Agent in-process, separate process, or sandboxed — and when to switch?
- Process per execution or long-running daemon?
- Should TraceWeaver read the SQLite file directly or consume an event stream?

**Durability**
- SQLite vs custom log — at what event rate does it matter?
- Checkpoint granularity: per mutating call, per `exec`, or time-based?
- How much LLM conversation state to persist, given privacy?

**Security**
- When to add seccomp, and with which syscall policy?
- Is a stronger sandbox backend worth the complexity, and for which threat?
- How to sandbox third-party MCP servers?
- How to handle hardlinks, device files, and FIFOs in the workspace?

**Policy**
- Custom TOML vs an established policy language?
- How to express argument-level constraints without a full DSL?
- Policy layering (org ∩ project ∩ execution)?

**Product**
- Who is the first real external user, and what do they need?
- CLI-only, TUI, or web UI for TraceWeaver?
- Python SDK or MCP server first?

**Process**
- License choice.
- Public from day one or after v0.1?

---

## 22. Idea parking lot

Uncommitted. Some are research projects on their own.

- **Information-flow control for tool calls:** tag data from untrusted sources (repo files, web, tool output) and restrict how tainted data can flow into sensitive tool arguments — in the spirit of recent "defend against prompt injection by design" research.
- **Policy synthesis:** record a successful run, propose the minimal policy that would have allowed it.
- **Anomaly flags:** compare a run's tool-call pattern against previous runs of the same task.
- **eBPF observation:** watch syscalls from sandboxed processes for richer forensic data.
- **WASM tools:** run some tools as WebAssembly modules with capability-based imports.
- **Deterministic test generation:** turn recorded executions into regression tests for agents.
- **"Flight recorder" mode:** low-overhead always-on recording with a rolling buffer.
- **Cost attribution:** per-task token, compute and API cost reports.
- **Editor/CI integrations:** VS Code panel showing live agent activity; GitHub App that runs agents on issues under AgentOS.
- **Kubernetes operator** for running AgentOS workers (only if Phase 5 happens).
- **Signed execution records:** attestations that a patch was produced under a given policy.
- **Multi-model comparison harness** built on comparative replay.
- **A patch/edit tool** (`fs.patch`: replace a range or apply a diff) so small models do not have to resend whole files. Came up in v1 M4 (D-060); not in v1 because the tool list is fixed at four.
- **Built in M10 (2026-11-15, D-108):** `agentos apply`.
- **Built in M9 (2026-11-15, D-098 to D-107)**, so no longer parked: a clean-up command (`agentos rm`), leaving folders out of the copy (`filesystem.skip`), starting itself under `systemd-run`, a message on Ctrl-C, a check command, reading part of a file, a hint for an inactive virtualenv, exceptions to deny rules, caches as non-violations (`filesystem.scratch`), a clearer end when the model gives up.
- **Still parked from the walk-through and the trial (2026-11-14):**
  - **Binary files and file modes through the file tools.** `agentos apply` exists now (M10, D-108) and carries them when a command made them; letting the agent's own `fs.read` and `fs.write` handle them is a separate question.
  - **`apply` on top of the user's own edits** (a merge). Today it refuses and points at the patch.
  - **A read-only folder inside the workspace** (for `node_modules`, which must be at its place in the project; `skip` only suits folders that work from their own path).
  - **A narrower way to allow one script** than a full argv rule per script.
- **From the M9 re-trial (2026-11-15):** an HTTP error from the model's server ended the run as `interrupted`. Built on 2026-11-15 (D-111): busy answers are retried, a wrong key is an error that names the settings, and a reply the server cannot use is told to the model, then ends the run as `stopped` with its patch.

---

## 23. Appendices

### Appendix A — Example policy (illustrative)

```toml
policy_version = 1
name = "coding-agent-default"

[filesystem]
read  = ["**"]
write = ["src/**", "tests/**"]
deny  = [".env", ".env.*", "secrets/**", ".git/**"]

[exec]
allow = [
  ["pytest"],
  ["python", "-m", "pytest"],
  ["cargo", "test", "--offline"],
]
env = ["PATH", "LANG"]

[network]
mode = "none"            # later: "allowlist"
# allow_hosts = ["pypi.org", "files.pythonhosted.org"]

[limits]
memory_mb = 1024
pids = 256
cpu_quota_percent = 200
call_timeout_s = 120
execution_timeout_s = 900
max_tool_calls = 100
max_output_kb = 256
workspace_mb = 512

# Phase 2+
# [approvals]
# require = ["github.pr.create"]
```

### Appendix B — Example event (illustrative)

```json
{
  "schema_version": 1,
  "event_id": "01JC8Z3V6Q8M2K4T7B1N9X5R2D",
  "execution_id": "exec_7f3a",
  "seq": 42,
  "type": "tool.invocation.intent_logged",
  "ts": "2026-11-10T08:00:00.123Z",
  "mono_ns": 1834567890123,
  "parent_event_id": "01JC8Z3V6Q8M2K4T7B1N9X5R2C",
  "call_id": "call_0019",
  "actor": "runtime",
  "payload": {
    "tool": "fs.write",
    "path": "src/auth.py",
    "pre_hash": "sha256:…",
    "new_hash": "sha256:…",
    "policy": { "decision": "allow", "rule": "filesystem.write[0]" }
  },
  "blob_refs": ["sha256:…"]
}
```

### Appendix C — Example CLI session (illustrative)

```
$ agentos run --policy policy.toml --repo examples/demo-repo --agent llm \
    --task "Make the failing test pass"
exec_7f3a  RUNNING
  fs.list   .                         allow
  fs.read   tests/test_auth.py        allow
  fs.read   src/auth.py               allow
  fs.write  src/auth.py               allow   (+3 −1)
  exec      pytest -q                 allow   exit 0  2.1s
  fs.read   .env                      DENY    filesystem.deny[0]
exec_7f3a  COMPLETED  patch: ~/.local/share/agentos/out/exec_7f3a.patch

$ agentos trace exec_7f3a --denied
#31  fs.read .env  DENY  rule=filesystem.deny[0]  parent=agent.message#30
```

### Appendix D — Demo script for the finished project (Phase 1 → 3)

1. Start an execution on the demo repo with the default policy.
2. Show the agent reading, editing, running tests.
3. Kill the runtime with `kill -9` mid-`exec`.
4. Run `agentos recover`; show the call marked UNCERTAIN → NOT_APPLIED and the workspace restored.
5. Agent resumes and finishes.
6. Show a blocked attempt to read `.env` and a blocked network call.
7. (Phase 3) Open TraceWeaver: timeline, causal graph, file diffs.
8. (Phase 3) Replay recorded interactions without repeating side effects; diff against a run with a different model.

### Appendix E — Reading list (starting points)

- Linux man pages: `namespaces(7)`, `user_namespaces(7)`, `mount_namespaces(7)`, `cgroups(7)`, `openat2(2)`, `landlock(7)`, `seccomp(2)`, `pidfd_open(2)`.
- Kernel docs: cgroup v2, Landlock.
- Source: bubblewrap, nsjail, youki (Rust container runtime).
- SQLite: WAL mode, `synchronous` pragma, atomic commit.
- Papers/posts on crash consistency in filesystems and applications (the "all file systems are not created equal" line of work).
- Idempotency keys in payment and API design.
- Material on durable execution engines and on prompt injection defences for agents.
