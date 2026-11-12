# AgentOS

A small Rust runtime that runs a coding agent's tool calls inside a rootless Linux sandbox, and survives `kill -9` at any moment without losing track of what happened.

The agent (a script, or an LLM) never touches anything itself. It asks; a gateway checks a policy, writes down what is about to happen, does it, and writes down that it is done. If the runtime dies in between, recovery can say for every call whether it happened, and it never silently runs an uncertain call again.

It is a learning and portfolio project, not a product. It does one thing: one agent, one copy of a repository, four tools, a command line.

## Status

**v0.1.** Everything below was built and tested on one machine: NixOS 26.05, Linux 7.1.8, btrfs, with the `nix` policy preset.

- **Not yet verified: any other Linux distribution.** The `fhs` preset (Ubuntu, Debian, Fedora, Arch) and the CI job for it (`.github/workflows/ci.yml`) exist but have never run. Treat the Ubuntu instructions as untested.
- No license has been chosen yet.
- Read [what it does not do](#what-it-does-not-do) before trusting it with anything.

## The demo

```sh
git clone <this repository> && cd agentos
nix develop -c make demo
```

Measured from a fresh clone on the development machine: **23 seconds** from that command to the end of the demo, including a full debug build. That machine already had the Rust crates and the Nix shell's packages on disk; a first run elsewhere also downloads those (about a hundred crates, plus pytest and hyperfine through Nix), which was not measured.

What you will see:

1. The demo repo (`examples/demo-repo`, a tiny Python project) has a failing test.
2. A scripted agent lists files, runs `pytest` (fails), reads two files, tries to read `.env` (**denied**), writes the fix, runs `pytest` (passes). Each line is one tool call that went through the policy, the log and the sandbox:

   ```text
     #1   fs.list  .                                        allow  4 entries
     #2   exec     pytest -q                                allow  exit 1, 0.2s
     #3   fs.read  tests/test_stats.py                      allow  304 bytes
     #4   fs.read  src/stats.py                             allow  554 bytes
     #5   fs.read  .env                                     DENY   filesystem.deny[0] ".env"
     #6   fs.write src/stats.py (548 bytes)                 allow  wrote 548 bytes
     #7   exec     pytest -q                                allow  exit 0, 0.2s
   ```

3. The result is a **patch**; the original repo is not modified.
4. The same task again, but the runtime kills itself with SIGKILL right after the file write and before its checkpoint.
5. `agentos recover` looks at the file, compares it with the hashes stored in the intent, and reports `#6 fs.write APPLIED`.
6. `agentos run --resume` continues with call #7. Nothing runs twice.

A full transcript is in [`docs/demo-transcript.md`](docs/demo-transcript.md).

### Without Nix (untested)

You need: Linux 5.14 or newer with cgroup v2, Landlock and unprivileged user namespaces; a systemd user session; Rust 1.89+; Python 3 with pytest; GNU `cp` and `diff`.

```sh
sudo apt install build-essential python3-pytest diffutils       # Ubuntu / Debian
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh  # Rust
sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0   # Ubuntu 23.10+ only
make demo                                                       # picks policies/fhs.toml
```

If something is missing, `agentos` stops at startup and says what (for example, how to get a delegated cgroup).

## What it shows

Three suites, one command (`nix develop -c make eval`). Results of record, with raw data, are in [`docs/results/`](docs/results/).

| Suite | Result (`nix` preset) |
|---|---|
| **Fault injection.** The runtime is killed at 8 points (before the intent, after it, during the effect, before, during and after the checkpoint, after completion, and during recovery itself) for each tool, 200 times each. After recovery, 5 invariants are checked against an independent oracle, then the run is resumed to the end. | 4,800 runs, 4,797 killed, **0 invariant violations**, 0 duplicated effects, 3,918 of 3,918 cut-off calls classified correctly. `agentos recover`: p50 20 ms, p99 55 ms |
| **Adversarial security.** 32 attacks: path traversal, symlinks, a symlink flipped ~6,000 times during 300 file calls (TOCTOU), reading denied and host files from code run by an allowed command, writing outside the allowed paths, network, fork bomb, memory hog, infinite loop, privilege tricks, the event log, the output patch. | 32 passed, **0 escapes** |
| **Benign.** 23 ordinary operations under the shipped policy. | **0 false denials** |
| **Overhead** (release build, i5-13420H, btrfs). | Sandboxed `true`: p50 7.8 ms, p99 24.6 ms (direct: 1.3 / 2.8 ms). `fs.write` 4 KB with fsynced log and checkpoint: p50 13.7 ms, p99 23.8 ms (durable direct write: 1.9 / 2.8 ms). Demo task end to end: p50 507 ms vs 376 ms direct (+35%) |

A real LLM also completed the demo task (a local 4B model under llama.cpp), including after being killed mid-run. Three earlier attempts failed; both the failures and the successful run are in `docs/results/`. This shows the loop works end to end with a real model, not that the agent is good.

The suites found five bugs in the runtime itself while it was being built; each has a short note in [`docs/bugs.md`](docs/bugs.md).

## How it works

Five parts:

| Part | Plain meaning | Where |
|---|---|---|
| Gateway | The agent cannot touch anything. It asks; the gateway does it. | `src/gateway.rs` |
| Policy | A TOML file: which paths may be read and written, which are denied, which commands may run, with what limits. No matching rule means no. | `src/policy.rs`, `policies/` |
| Sandbox | Commands run in a box that holds only a copy of the repo (`/work`) and read-only system folders. No network, no home folder, no privileges, limited memory, CPU and processes. | `src/sandbox/` |
| Event log | Before doing anything, write "about to do X" and fsync it. Afterwards, write "X is done". | `src/log.rs` |
| Checkpoints and recovery | After each change, copy the folder. After a crash, decide per call whether it happened; if that cannot be known, go back to the last copy. | `src/checkpoint.rs`, `src/recovery.rs` |

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

- **File tools** run in the runtime and open every path with `openat2(RESOLVE_BENEATH)`, so the kernel refuses anything that leaves the workspace; the opened handle is the one that is read or written, so a swapped symlink cannot redirect it.
- **Commands** are argv lists run with `execve`, never a shell. The allowlist is not the security boundary (an allowed `pytest` runs the repo's own code); the sandbox is.
- **Denied files** are not copied into the workspace at all, so no code can read them.
- **After each command**, changes outside the allowed write paths are put back from the checkpoint and reported.
- **After a crash**: an uncertain `fs.write` is settled by comparing the file with the before and after hashes stored in its intent; an uncertain command cannot be known, so the workspace is rolled back with one atomic folder swap. Recovery can itself be killed and run again.

More: [`docs/threat-model.md`](docs/threat-model.md), [`docs/recovery-semantics.md`](docs/recovery-semantics.md).

## Using it

Everything runs under a cgroup that systemd delegated to you (no root needed):

```sh
alias agentos='systemd-run --user --scope -p Delegate=yes -q target/debug/agentos'

agentos run --repo <repo> --policy policies/nix.toml --agent scripted --script <steps.toml>
agentos run --repo <repo> --policy policies/nix.toml --agent llm --task "Make the failing test pass"
agentos status                 # all executions; "interrupted" means it was killed
agentos trace [<id>]           # one line per call; --events for every record; --json for scripts
agentos recover <id>           # settle the call that was cut off, repair the workspace
agentos run --resume <id>      # recover, then let the agent continue
```

State lives in `--state-dir`, else `$AGENTOS_STATE_DIR`, else `~/.local/share/agentos`. Each execution has a folder with its log, policy copy, workspace, checkpoints, blobs and `result.patch`. Apply a patch with `patch -p1 < result.patch` in the original repo, after reading it.

The LLM agent talks to any OpenAI-compatible endpoint: `AGENTOS_LLM_BASE_URL` (default `http://127.0.0.1:8080/v1`, a local `llama-server --jinja`), `AGENTOS_LLM_MODEL`, and `AGENTOS_LLM_API_KEY` for hosted APIs. The model is called by the runtime, outside the sandbox; the key is never written to disk.

A policy, in full:

```toml
policy_version = 1
name = "default-nix"

[filesystem]
read  = ["**"]
write = ["src/**", "tests/**", "**/__pycache__/**"]
deny  = [".env", ".env.*", "**/.env", "**/.env.*", "secrets/**", ".git/**", "**/*.pem", "**/id_rsa*"]

[exec]
allow = [["pytest"], ["python3", "-m", "pytest"]]   # argv prefixes

[sandbox]
ro_mounts = ["/nix/store"]                          # fhs preset: /usr, /bin, /lib, /lib64

[limits]
memory_mb = 512
pids = 128
cpu_percent = 200
call_timeout_s = 60
max_output_kb = 64
max_tool_calls = 50
```

## What it does not do

- **It is not hardened like a production container runtime.** No seccomp filter; kernel bugs are out of scope; tested on one kernel and one distribution.
- **No power-loss guarantee.** The tests kill the process; they do not cut power. Checkpoints are not fsynced.
- **No exactly-once execution.** A command that was cut off is rolled back and may be issued again by the agent as a new call.
- **No disk limit.** A command can fill the disk. Checkpoints are never pruned.
- **No network, ever**, for commands. No web search, no package downloads, no external tools.
- **The sandbox can see** the read-only system folders in full (on NixOS, all of `/nix/store`) and some facts about the host (host name, kernel version, the workspace's path).
- **Prompt injection is contained, not prevented.** A file can talk the model into asking for something bad; the policy and sandbox then refuse or confine it.
- **Containment ends at the patch.** A patch can change test hooks or CI files that later run unsandboxed on your machine. Read it before applying.
- One agent, one workspace. No dashboard, no replay, no MCP, no SDK, no multi-agent, no approvals. Those were cut on purpose (see the plan).

## Repository

```text
src/
  main.rs          command line
  gateway.rs       requested → policy → intent → effect → checkpoint → completed
  policy.rs        the TOML policy and its glob rules
  tools/           fs.rs (openat2 file tools), exec.rs (program lookup, output capture)
  sandbox/         ns.rs (namespaces, new root), cgroup.rs, landlock.rs, beneath.rs (openat2)
  log.rs           event log and the call state machine
  workspace.rs     the execution folder; copying the repo without denied files
  checkpoint.rs    copies, rollback, revert of disallowed changes, the patch
  recovery.rs      verdicts after a crash
  faults.rs        crash points for the tests
  agent/           scripted.rs, llm.rs
policies/          fhs.toml, nix.toml
tests/             security/ (32 attacks), recovery/ (fault injection), benign.rs, llm_agent.rs, vertical_slice.rs
benches/, bench/   the three overhead benchmarks
examples/          demo-repo/, scripts/, sandbox_spawn.rs
experiments/       small programs that tried each mechanism alone first
docs/              see below
```

## Documents

This project was built to be studied afterwards, so the written record is as much a part of it as the code.

| Document | What it is |
|---|---|
| [`AgentOS v1 — Scoped Design.md`](AgentOS%20v1%20—%20Scoped%20Design.md) | The plan: scope, cut list, threat model, milestones |
| [`docs/decision-log.md`](docs/decision-log.md) | Every decision taken or discarded, with reasons and evidence; findings; glossary |
| [`docs/milestones/`](docs/milestones/) | One document per milestone: requirements with evidence, the code explained, problems hit, questions |
| [`docs/decisions/`](docs/decisions/) | 11 short ADRs for the hard-to-undo decisions |
| [`docs/threat-model.md`](docs/threat-model.md) | What is protected, each attack with its defence and test, what is out of scope |
| [`docs/recovery-semantics.md`](docs/recovery-semantics.md) | What a crash leaves and what recovery decides, per crash point and tool |
| [`docs/bugs.md`](docs/bugs.md) | Bugs the test suites found in the runtime |
| [`docs/results/`](docs/results/) | Results of record with raw data |

## Development

```sh
nix develop            # Rust 1.95, Python + pytest, hyperfine
make test              # everything, quick (3 kills per recovery cell): about 30 s
make eval              # the three suites at full size: about 13 min
make lint
```

The git history is written to be read from the first commit: small steps, each saying what changed and why.
