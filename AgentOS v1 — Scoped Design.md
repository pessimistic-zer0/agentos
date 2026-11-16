# AgentOS v1 — Scoped Design

Nov 10, 2026 · @Prince

> **This document is the plan.** `AgentOS_Project_Spec.md` is a "maybe someday" list and does not add work to v1. If the two disagree, this document wins. The outline, the build order and the cut list below are fixed; changing any of them needs an ADR with a written reason.

## Summary

AgentOS v1 is a single-binary Rust runtime that runs a coding agent's tool calls inside a rootless Linux sandbox and survives `kill -9` at any point without losing track of what happened.

The core rule: every side effect is durably recorded as an *intent* before it runs. After a crash, the runtime can say which calls completed, which failed, and which are uncertain, and it never silently re-executes an uncertain one.

Sandboxing alone is commodity (E2B, Daytona, bubblewrap, nsjail). The contribution of v1 is **recovery semantics proven by a fault-injection harness**, plus an adversarial test suite for the sandbox. Target: about 12 weeks alongside coursework.

This is a learning and portfolio project. It is not a startup and has no users to support, so anything that does not teach something or prove a claim is cut.

## The five parts

| Part | Plain meaning | Real name |
| --- | --- | --- |
| Gate | The agent cannot touch anything itself. It asks; the gate does it. | Tool gateway |
| Rules | A small file that says which files and commands are allowed. No matching rule means no. | Policy (TOML) |
| Cage | Commands run in a box that holds only a copy of the code folder. No network, no home folder, limited memory and CPU. | Sandbox |
| Diary | Before doing anything, write "about to do X". Afterwards, write "X is done". | Event log (SQLite) |
| Save points | After each change, copy the folder. If we cannot tell what happened, go back to the last copy. | Checkpoints + recovery |

## Scope

v1 is one agent, one workspace, four tools, and a CLI.

**In v1**

- One agent per execution; the workspace is a copy of a target repo in a scratch directory on disk.
- Four tools: `fs.read`, `fs.write`, `fs.list`, `exec` (argv allowlist, no shell).
- A TOML policy file: read/write path globs, deny globs, allowed argv prefixes, resource limits, and which system folders the sandbox may see.
- Sandbox: user/mount/pid/net namespaces, cgroup v2, Landlock, no network.
- Tool-call state machine and append-only event log in SQLite.
- Workspace checkpoints as directory copies.
- Crash recovery on runtime restart.
- CLI: `run`, `status`, `trace`, `recover`.
- Two agents: a deterministic scripted agent for tests, and a minimal LLM loop for the demo.
- Demo target: a small Python repo with a failing `pytest` test, so no dependency downloads are needed.

**Cut from v1**

| Cut | Why |
| --- | --- |
| Web dashboard / TraceWeaver UI | `agentos trace` proves the data model; a React UI adds no systems signal |
| Replay engine | Needs a stable event schema first |
| MCP adapter | Integration breadth, not depth |
| Python SDK | CLI plus JSON event output is enough |
| Multi-agent, scheduler, remote workers | Single-agent recovery must work first |
| Network of any kind, including web search and an allowlist proxy | Network is fully off in v1; search can leak data out and bring hostile text in |
| External tools (open a PR, call an API) | See "Bonus" at the end; not part of done |
| Human approval workflows | Only matters once external side effects exist |
| seccomp filter | Namespaces + Landlock + no\_new\_privs cover the v1 threat model. (Built for v1.0 as a deny list: D-116) |
| Hash-chained event log | Detects corruption only; no test in v1 depends on it |
| Hard disk-size limit on the workspace | The only rootless way is a tmpfs, which vanishes when the runtime is killed and breaks recovery. Listed as a known limit instead |
| Git-based checkpoints | A test showed they miss ignored files, empty directories, file modes and a nested `.git` |
| Large benchmark set | Three numbers are enough (see Evaluation) |
| Tokio / async | One agent makes one call at a time; blocking code plus a watchdog thread is simpler |
| Pluggable sandbox backends | One backend does not need an abstraction |

## Threat model (v1)

v1 assumes the agent and everything it reads are hostile; the host kernel and the runtime binary are trusted.

The argv allowlist limits what the agent *asks for*, not what it can *do*. `pytest` runs the repo's `conftest.py` and `cargo test` runs `build.rs`, so any allowed test command means arbitrary code execution inside the sandbox. The sandbox, not the allowlist, is the security boundary.

**Assets**

- Host files outside the workspace, especially `~/.ssh`, `~/.config` and tokens.
- Files in the target repo that the policy denies (e.g. `.env`).
- Host environment variables and credentials.
- Network egress.
- The event log and checkpoints (the agent must not be able to rewrite its own history).
- Host CPU, memory and PIDs.

**Adversary:** the agent process and anything it spawns, possibly steered by prompt injection in repo files or tool output. It can issue any tool call and run allowed commands with arbitrary arguments and repo contents.

**In scope (each gets a test)**

- Path traversal (`../`), absolute paths, symlinks pointing outside the workspace.
- Symlink swapped between check and use (TOCTOU).
- Reading secrets via environment or `/proc`.
- Reading a denied file from code run by an allowed command (not only through `fs.read`).
- Writing outside the write globs from code run by an allowed command.
- Fork bombs, memory exhaustion, infinite loops.
- Network exfiltration from code run by an allowed command.
- Opening or modifying the event log or checkpoint store from inside the sandbox.

**Out of scope (stated in the README)**

- Kernel exploits, a malicious host user or root, side channels, a malicious LLM provider.
- Multiple mutually hostile agents on one host.
- Filling the disk from inside the workspace (known limit; no hard cap in v1).
- Power loss. The tests kill the process; they do not cut power, so no power-loss claim is made.
- What happens after the human applies the patch. A patch can change test hooks or CI files that later run unsandboxed. Containment ends when the patch leaves AgentOS.

## Architecture

One trusted process owns policy, logging and recovery. The sandbox only ever sees `/work`; the event log and checkpoints live where it cannot reach them.

&#91;embedded content: AgentOS v1 components · trust boundaries\]

The agent never touches the system directly. Every call goes through the gateway, which checks policy and fsyncs an intent before dispatching to the fs tools or a fresh sandboxed exec child.

The LLM is called by the runtime, outside the sandbox. Sandboxed commands never get a network.

## Workspace and checkpoints

**Creating the workspace.** The target repo is copied into a scratch directory. Files matching the policy's deny globs are **not copied**. This is what makes deny rules real: a denied file does not exist inside the sandbox, so neither `fs.read` nor code run by `exec` can read it. Symlinks are copied as symlinks, never followed. The original repo is never modified.

**Checkpoints.** A checkpoint is a full copy of the workspace directory, made with a reflink copy where the filesystem supports it (btrfs, XFS) and a plain copy elsewhere (ext4). The copy is written to a temporary name and then renamed, so a half-made checkpoint is never mistaken for a real one. A directory copy captures everything: ignored files, empty directories, file modes, special files.

**Ordering rule.** A mutating call follows this order: effect → checkpoint → COMPLETED record that names the checkpoint. A call is never COMPLETED without its checkpoint. The "last checkpoint" always means the last one named in the log; recovery deletes any checkpoint the log does not name.

**Output.** The result of an execution is a patch: the difference between the first checkpoint and the final workspace. The human applies it.

## Tool-call state machine and recovery

Every tool call moves through explicit states, and its intent is fsynced before the effect starts. A call caught mid-flight by a crash becomes UNCERTAIN and is reconciled per tool, never re-run blindly.

&#91;embedded content: tool-call lifecycle · crash paths dashed\]

On restart, recovery replays the log, finds calls with an intent but no terminal record, and resolves each with the rules below before the agent resumes.

| Tool | Effect | How an UNCERTAIN call is reconciled |
| --- | --- | --- |
| `fs.read`, `fs.list` | None | Mark NOT APPLIED; the agent can re-issue it |
| `fs.write` | One file in `/work` | The intent stores the path, pre-write hash and new-content hash. File matches the new hash: APPLIED (take the missing checkpoint now). Matches the old hash: NOT APPLIED (if the write was going to make folders, roll back first; D-084). Anything else: roll back to the last checkpoint |
| `exec` | Anything inside `/work` | Can't be known; roll `/work` back to the last checkpoint and mark NOT APPLIED |

**Invariants the fault-injection harness checks**

1. No effect happens without a durable intent recorded before it.
2. No COMPLETED record exists whose effect is missing.
3. After recovery, `/work` equals the last checkpoint plus exactly the APPLIED calls since it.
4. No UNCERTAIN call is re-executed before it is reconciled.
5. The event log is a gap-free prefix: no torn or out-of-order records.

The agent's conversation is in the log too. On resume it continues from its last recorded message and receives a synthetic tool result for each reconciled call.

## Sandbox design

Each `exec` runs in a fresh child with its own namespaces and cgroup. The filesystem tools run in the runtime process but resolve every path with kernel-enforced confinement, never string checks.

| Layer | Mechanism | Stops |
| --- | --- | --- |
| Filesystem view (exec) | Mount namespace: workspace bind-mounted rw at `/work`; system folders from the policy mounted read-only; tmpfs `/tmp`; no `/home` | Reading host files and secrets |
| Path access (fs tools) | `openat2` with `RESOLVE_BENEATH` and `RESOLVE_NO_MAGICLINKS` on a workspace dirfd | `..`, absolute paths, symlink escapes, TOCTOU swaps |
| Path access (exec) | Landlock ruleset: rw `/work`, ro system folders | Escapes through a bug in mount setup (defence in depth) |
| Process | User + PID namespace, `PR_SET_NO_NEW_PRIVS`, all capabilities dropped | Signalling host processes, setuid binaries |
| Network | Empty network namespace (loopback only) | Exfiltration, downloads |
| Resources | cgroup v2 `memory.max`, `pids.max`, `cpu.max`; watchdog kills the whole cgroup at wall-clock limit | Fork bombs, host OOM, runaway jobs |
| Environment | Environment cleared; only `PATH`, `HOME=/work`, `LANG` passed | Credential leakage |

**Not tied to one distro.** Where programs live differs between Linux systems, so the read-only system folders are policy, not code. Two presets ship:

| Preset | Read-only mounts | For |
| --- | --- | --- |
| `fhs` | `/usr`, `/bin`, `/lib`, `/lib64` | Ubuntu, Debian, Fedora, Arch |
| `nix` | `/nix/store` | NixOS |

The runtime resolves each allowed command to its real path on the host before the sandbox starts and builds `PATH` from that. Nothing else in the code may assume a distro. The security and recovery suites must pass on both presets before v0.1.

**Command policy.** Allowed commands are argv prefixes, executed with `execve` and never through a shell, e.g. `["pytest"]` or `["cargo", "test", "--offline"]`. Arguments after the prefix are allowed but logged.

**Deny paths.** Denied files are left out when the workspace is created (see above). `fs.*` calls also match the *resolved* path against the allow and deny globs after `openat2` succeeds, so a denied path created later is still refused.

**Write globs and `exec`.** Code run by `exec` can write anywhere in `/work`. After each `exec`, the runtime compares the workspace with the pre-call checkpoint. Any change outside the write globs is reverted from the checkpoint and logged as a policy violation. The call itself still completes.

**Atomic writes.** `fs.write` writes a temp file beneath the workspace, fsyncs it, then `renameat` over the target, so a crash never leaves a half-written file. Folders missing on the way to the file are made first, after the intent, if the policy allows the file (D-084).

**Rootless cgroups.** Run the runtime under a delegated cgroup subtree (`systemd-run --user --scope -p Delegate=yes`) so no root is needed. A cgroup cannot hold both processes and child groups with limits, so the runtime first moves every process of the delegated group (itself and, for example, the shell that started it) into a leaf group, then creates one child group per `exec` (D-049). Checked working on NixOS (kernel 7.1) and, in CI, on Ubuntu 24.04, which restricts unprivileged user namespaces by default and needs a documented one-line setting.

**Startup checks.** On start, the runtime checks for user namespaces, Landlock, `openat2` and cgroup delegation, and stops with a clear message naming what is missing.

**Read before writing code:** bubblewrap's source, nsjail's config docs, the Landlock kernel docs, and the Linux sandbox in OpenAI's Codex CLI (Rust, Landlock). Copy their setup order; most namespace bugs are ordering bugs.

## Evaluation

v1 is done only when three suites pass and are reproducible with one command (`make eval`).

**1. Fault-injection harness (the headline result).** The runtime reads `AGENTOS_CRASH_AT=<point>` and sends itself `SIGKILL` there (not `abort()`, which would write thousands of core dumps; D-052). Mid-effect points use `SIGKILL` after a random delay instead. For each point and each tool, run the scripted agent 200 times, restart, run `recover`, and check the invariants from the recovery section.

| Crash point | Where |
| --- | --- |
| P1 | Before the intent is written |
| P2 | After the intent is fsynced, before the effect starts |
| P3 | During the effect (temp-file write; running `exec`) |
| P4 | After the effect, before the checkpoint |
| P5 | During the checkpoint copy |
| P6 | After the checkpoint, before the completion record |
| P7 | After the completion record, before the agent sees the result |
| P8 | During recovery itself (recovery must be re-entrant) |

Report: runs per point, invariant violations, duplicated side effects, uncertain calls correctly classified, and recovery time p50/p99.

**2. Adversarial security suite.** At least 30 cases covering every in-scope attack, including a TOCTOU loop that swaps a symlink while the agent writes, a `conftest.py` that tries to read a denied file, and a test that writes outside the write globs. Pass criteria: 0 escapes. A benign suite of normal operations must show 0 false denials.

**3. Overhead benchmarks.** Three numbers, each against direct execution without AgentOS:

- Sandbox spawn cost: `exec true`, measured with hyperfine.
- `fs.write` 4 KB including intent fsync and checkpoint, measured with criterion.
- End-to-end demo task wall time.

Report p50/p95/p99 with CPU, RAM, kernel version and filesystem. Publish the raw results and scripts.

## Milestones

Twelve weeks, each block ending in something you can run. The order is fixed. If exams cause a slip, take the time from weeks 10–12, never from the tests.

Before each block, do the small throwaway experiments listed for it, so each mechanism is met alone before it is combined with others.

| Weeks | Milestone | Experiments first | Exit criterion |
| --- | --- | --- | --- |
| 0 | Rust ramp: the Book ch. 1–10 and 15–16; ownership, `Result`, `anyhow`/`thiserror` | — | A small CLI that reads a TOML file and writes rows to SQLite |
| 1–2 | Vertical slice, no sandbox: gateway, 4 tools, policy, scripted agent, event log, `agentos trace` | — | Scripted agent fixes the demo repo's failing test; `trace` prints every call |
| 3–5 | Sandbox: namespaces, cgroup v2, Landlock, `openat2`, env scrubbing, both presets | `unshare -Urmpn --fork --mount-proc sh`; a fork bomb under `pids.max`; a 30-line `openat2` program; the `landlock` crate example | 20 adversarial cases, 0 escapes |
| 6–8 | Durability: state machine, intent fsync, directory checkpoints, `agentos recover` | Write-temp-then-rename under `kill -9` in a loop; SQLite insert killed mid-transaction | P1–P8 pass all invariants over 200 runs each |
| 9 | LLM agent loop against an OpenAI-compatible endpoint (local llama.cpp server or hosted) | — | A real LLM completes the demo task inside the sandbox |
| 10 | Benchmarks; expand security suite to 30+ cases, including the TOCTOU race | — | Three numbers with p50/p95/p99 and hardware details |
| 11–12 | README, `threat-model.md`, `recovery-semantics.md`, ADRs, demo recording, tag v0.1 | — | A stranger can clone and run the demo in under 10 minutes on Ubuntu or NixOS |

Every milestone also ends with its document in `docs/milestones/` and a git tag; it is not finished without them.

The first two weeks deliberately skip the sandbox: get the event model and the trace output right while the code is still small.

## Stack and repo layout

One crate, synchronous Rust, and as few dependencies as possible. Split into a workspace only when a module boundary actually hurts.

| Need | Choice | Note |
| --- | --- | --- |
| Syscalls | `nix` | `unshare`, `mount`, `pivot_root`, `openat2` (raw syscall if `nix` lacks it) |
| Filesystem confinement | `landlock` crate |  |
| cgroups | Write to cgroupfs directly | A crate adds little here |
| Storage | `rusqlite` (bundled), WAL mode | `synchronous=FULL` for intent records |
| Checkpoints | Recursive directory copy, reflink where supported | Start by shelling out to `cp -a --reflink=auto` |
| Patch output | Shell out to `diff -ruN` |  |
| Config / events | `serde`, `serde_json`, `toml` |  |
| Hashing | `sha2` | Content hashes for write reconciliation |
| CLI | `clap` |  |
| Errors / logs | `anyhow`, `thiserror`, `tracing` |  |
| LLM client | `ureq` (blocking HTTP) | OpenAI-compatible API |
| Benchmarks | `criterion`, `hyperfine` |  |
| Dev environment | `flake.nix` dev shell (Rust, Python, pytest) | Optional convenience; the README also lists plain `apt` packages |

```text
agentos/
├── src/
│   ├── main.rs          # CLI
│   ├── gateway.rs       # validate → policy → intent → dispatch
│   ├── policy.rs
│   ├── tools/           # fs.rs, exec.rs
│   ├── sandbox/         # ns.rs, cgroup.rs, landlock.rs
│   ├── log.rs           # event log + state machine
│   ├── workspace.rs     # create the copy, leave out denied files
│   ├── checkpoint.rs
│   ├── recovery.rs
│   └── agent/           # scripted.rs, llm.rs
├── policies/            # default policy for each preset (fhs, nix)
├── tests/
│   ├── security/
│   └── recovery/        # fault-injection harness
├── benches/
├── examples/demo-repo/  # small Python project with a failing test
└── docs/
    ├── decision-log.md  # every decision taken or discarded, in plain words
    ├── milestones/      # one document per milestone: aim, requirements, code explained
    ├── threat-model.md
    ├── recovery-semantics.md
    └── decisions/
```

## Definition of done

AgentOS replaces Drishti on the resume only when all of these are true:

Status (boxes as of 2026-11-13; at tag `v0.1` on 2026-11-12 the third box was open; evidence in `docs/results/` and `docs/milestones/M6-release.md`):

- [x] P1–P8 fault-injection suite passes with 0 invariant violations. *(`nix` preset: 4,800 runs.)*
- [x] Security suite: 30+ cases, 0 escapes, 0 false denials on the benign suite. *(`nix` preset: 32 cases, 23 benign operations.)*
- [x] Both suites pass with the `fhs` and the `nix` preset. *(`fhs`: CI run <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> on 2026-11-13, commit `c3c9502`, Ubuntu 24.04: 4,800 runs, 0 violations; 32 cases, 0 escapes; 0 false denials. It was the third run; the first two failed and found bugs B-007 and B-008. D-072.)*
- [x] Three benchmarks published with scripts, raw data and hardware details.
- [x] Demo runs from a fresh clone in under 10 minutes. *(NixOS: 23 s with dependencies already downloaded. Ubuntu: not run.)*
- [x] Threat model, recovery semantics and at least 3 ADRs written. *(11 ADRs.)*

All six boxes are ticked as of 2026-11-13. Two things were not run on Ubuntu and are not part of any box as written: the benchmarks, and `make demo`.

**Resume bullet templates.** Replace each bracketed placeholder only with numbers you measured.

- Built AgentOS, a Rust runtime that executes LLM coding-agent tool calls in a rootless Linux sandbox (namespaces, cgroup v2, Landlock, `openat2`) under a default-deny policy; 0 escapes across \[N\] adversarial tests including symlink TOCTOU races and fork bombs.
- Designed intent-before-effect durable execution on SQLite WAL with copy-on-write workspace checkpoints; recovered correctly at all 8 injected crash points across \[N\] `kill -9` runs with 0 invariant violations.
- Measured \[X\] ms p50 / \[Y\] ms p99 sandbox spawn overhead and \[Z\]% end-to-end slowdown versus unsandboxed execution.

## Decisions (ADRs)

Each gets a short ADR in `docs/decisions/`, revisited only if a measurement says so.

| ADR | Question | Decision |
| --- | --- | --- |
| 0001 | Run fs tools in the runtime (with `openat2`) or in a helper inside the sandbox? | Runtime with `openat2`; simpler, and the security suite tests it |
| 0002 | Checkpoint after every mutating call, or only after `exec`? | Every mutating call; measure the cost in week 10 |
| 0003 | Recover an uncertain `exec` by rolling back to the checkpoint, or by pausing for a human? | Roll back and tell the agent |
| 0004 | Store file contents in the log, or only hashes? | Hashes in the log, contents in a content-addressed blob dir; needed to reconcile `fs.write` |
| 0005 | How does the agent's context survive a crash? | Every LLM message is an event; resume rebuilds the conversation from the log |
| 0006 | Rootless cgroup delegation vs requiring root | Rootless via `systemd-run --user -p Delegate=yes` |
| 0007 | Git commits or directory copies for checkpoints? | Directory copies; git misses ignored files, empty directories, modes and a nested `.git` |
| 0008 | How do deny globs bind code run by `exec`? | Denied files are never copied into the workspace |
| 0009 | How do write globs bind code run by `exec`? | Compare with the checkpoint after each `exec`; revert and log changes outside the globs |
| 0010 | Workspace on tmpfs (hard size cap) or on disk? | On disk; a tmpfs is lost when the runtime is killed. Disk fill is a known limit |
| 0011 | Which system folders does the sandbox see? | Set in policy; `fhs` and `nix` presets |

## After the bonus: what a first user needs (2026-11-14)

Decided by Prince after a walk-through as a new user (D-088). The five parts and the tool list do not change.

1. **Defects and wording inside v1** are fixed: clearer first messages (D-089); the patch says what it cannot carry (D-090).
2. **Dependencies in the sandbox** (new): a command can use packages installed outside the repository, through a read-only mount named in the policy and, where needed, environment variables named in the policy (D-091, D-092).
3. **An AppArmor profile for Ubuntu** (new): so that user namespaces need not be allowed for the whole system (D-093).

4. **The parked conveniences** (M9, opened by Prince on 2026-11-14, D-098), none of them a tool: `agentos rm` and `agentos check`; starting itself in a delegated scope; a message when a run is stopped; `fs.read` with a line range; policy lists `skip`, `scratch` and `deny_except`; a note for a virtualenv that is mounted but not active; a run whose model stops while its last command fails ends as `stopped` (D-099 to D-107).

5. **`agentos apply`** (M10, approved by Prince on 2026-11-15, D-108): a command the user runs after a run, which copies the result into the repository and refuses if the repository changed since the copy. A run itself still never modifies the original repo.

Held at the time, in the parking lot of `AgentOS_Project_Spec.md` (§22): binary files or file modes through the file tools. (Built for v1.0, D-123.)

## v1.0 scope (agreed 2026-11-16)

Naming: this document is the plan for what was tagged `v0.1`. "v1.0" is the next release, beyond this plan. Prince approved its contents on 2026-11-16 (D-114). Only these items are added; everything else stays in the parking lot of `AgentOS_Project_Spec.md` (§22).

| Milestone | Items |
|---|---|
| M11 hardening | A seccomp filter for sandboxed commands. A disk limit for the workspace: a measured limit, not a hard cap, unless an experiment finds a rootless hard cap (ADR 0010). Scratch folders kept out of checkpoints |
| M12 real projects | `fs.edit {path, old_str, new_str}`, a new agent tool (`old_str` must occur exactly once). A read-only folder inside the workspace (`node_modules`). Binary files and file modes through the file tools |
| M13 evidence | One run of a capable hosted model through the six tickets of the inventory project |
| M14 release | The project name (Prince decides). Version 1.0.0, a CHANGELOG, a release build in CI, install instructions, benchmarks on Ubuntu. A compatibility promise for the policy format and the event log schema. One more distribution in CI (Debian; dropped with a written reason if CI cannot run it). Tag `v1.0` after a green CI run |

The five parts and the design rules do not change. The tool list becomes: the four v1 tools, the bonus `ext.call`, and `fs.edit`.

Not in v1.0: network or downloads in the sandbox, multi-agent, a daemon or SDK, a merge for `agentos apply`.

## Bonus, only after everything above is done

One fake external tool: a small local service with injectable timeouts, called through the gateway with an idempotency key. It gives the "uncertain call that cannot be rolled back" path something real to do. It is not part of the definition of done and must not start before v0.1 is tagged.

**Status (2026-11-13): built**, after `v0.1` was tagged and CI was green, at Prince's request. It is a fifth gateway tool, `ext.call {service, payload}`; the policy names the services and only allows `http://127.0.0.1:<port>` addresses; the shipped policies do not allow it and the LLM agent is not offered it. An uncertain call is settled by sending the same request again with the same idempotency key; if the service does not answer, the call stays uncertain and the run stays stopped. Nothing above this section changed: the five parts, the four v1 tools, the cut list and the definition of done are as they were. It passes on both presets (`fhs`: CI run <https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321>, D-082). Record: D-073 to D-082, ADR 0012, `docs/milestones/M7-bonus-external-tool.md`, `docs/recovery-semantics.md` §9, `docs/threat-model.md` §7.
