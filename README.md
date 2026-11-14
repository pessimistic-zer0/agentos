# AgentOS

A small Rust runtime that runs a coding agent's tool calls inside a rootless Linux sandbox, and survives `kill -9` at any moment without losing track of what happened.

The agent (a script, or an LLM) never touches anything itself. It asks; a gateway checks a policy, writes down what is about to happen, does it, and writes down that it is done. If the runtime dies in between, recovery can say for every call whether it happened, and it never silently runs an uncertain call again.

It is a learning and portfolio project, not a product. It does one thing: one agent, one copy of a repository, four tools (plus one bonus tool, off by default), a command line.

## Status

**v0.1.** Built on one machine (NixOS 26.05, Linux 7.1.8, btrfs, `nix` policy preset). The test suites also pass in CI on Ubuntu 24.04 with the `fhs` preset.

- **Verified on two systems, no more.** NixOS on the development machine, and Ubuntu 24.04 in CI ([run 43385661105](https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105), 2026-11-13: lint, all test suites, and 4,800 kills with 0 violations). The first two CI runs failed and found two real bugs (B-007, B-008 in [`docs/bugs.md`](docs/bugs.md)). Debian, Fedora and Arch use the same `fhs` preset and have not been run. On Ubuntu the benchmarks and `make demo` have not been run either.
- Licensed under MIT or Apache-2.0, at your option (see [License](#license)).
- **After v0.1: one bonus tool.** `ext.call` sends a request to a (fake) outside service, something a checkpoint cannot undo. It is off unless a policy allows it. See [the bonus](#the-bonus-a-call-that-cannot-be-rolled-back).
- **After that: what a first user needs.** Clearer first messages, a patch that says what it leaves out, packages from outside the repository, and an AppArmor profile for Ubuntu. See [on your own project](#on-your-own-project). Verified on NixOS and, in [CI run 43590153676](https://github.com/pessimistic-zer0/agentos/actions/runs/43590153676), on Ubuntu 24.04, where a second job runs the sandbox through the profile with Ubuntu's restriction left on.
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

### Without Nix (the suites pass in CI on Ubuntu 24.04; the demo script itself was not run there)

You need: Linux 5.14 or newer with cgroup v2, Landlock and unprivileged user namespaces; a systemd user session; Rust 1.89+; Python 3 with pytest; GNU `cp` and `diff`.

```sh
sudo apt install build-essential python3-pytest diffutils       # Ubuntu / Debian
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh  # Rust
make demo                                                       # picks policies/fhs.toml
```

**Ubuntu 23.10 and newer** allow user namespaces only to programs that have an AppArmor profile. Pick one:

```sh
# (a) For agentos alone: install the program and the profile that comes with it.
#     Tried by the CI job `fhs-apparmor` on Ubuntu 24.04 (with the path changed to the
#     test binary; the install to /usr/local/bin below was not itself run in CI).
cargo build --release
sudo install -m 755 target/release/agentos /usr/local/bin/agentos
sudo install -m 644 packaging/apparmor/agentos /etc/apparmor.d/agentos
sudo apparmor_parser -r /etc/apparmor.d/agentos

# (b) For every program on the machine (what `make demo` and the main CI job use,
#     because they run the binary from target/, where the profile does not apply):
sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0
```

If something is missing, `agentos` stops at startup and says what (for example, how to get a delegated cgroup).

## What it shows

Three suites, one command (`nix develop -c make eval`). Results of record, with raw data, are in [`docs/results/`](docs/results/). The `fhs` column is from one CI run, after two that failed.

| Suite | Result (`nix` preset, development machine) | Result (`fhs` preset, Ubuntu 24.04 in CI) |
|---|---|---|
| **Fault injection.** The runtime is killed at 8 points (before the intent, after it, during the effect, before, during and after the checkpoint, after completion, and during recovery itself) for each tool, 200 times each. After recovery, 5 invariants are checked against an independent oracle, then the run is resumed to the end. | 4,800 runs, 4,797 killed, **0 invariant violations**, 0 duplicated effects, 3,918 of 3,918 cut-off calls classified correctly. `agentos recover`: p50 20 ms, p99 55 ms | 4,800 runs, 4,800 killed, **0 invariant violations**, 0 duplicated effects, 3,756 of 3,756 classified correctly. `agentos recover` on a shared runner: p50 34 ms, p99 315 ms |
| **Adversarial security.** 32 attacks: path traversal, symlinks, a symlink flipped ~6,000 times during 300 file calls (TOCTOU), reading denied and host files from code run by an allowed command, writing outside the allowed paths, network, fork bomb, memory hog, infinite loop, privilege tricks, the event log, the output patch. | 32 passed, **0 escapes** | 32 passed, **0 escapes** |
| **Benign.** 23 ordinary operations under the shipped policy. | **0 false denials** | **0 false denials** |
| **Overhead** (release build, i5-13420H, btrfs). | Sandboxed `true`: p50 7.8 ms, p99 24.6 ms (direct: 1.3 / 2.8 ms). `fs.write` 4 KB with fsynced log and checkpoint: p50 13.7 ms, p99 23.8 ms (durable direct write: 1.9 / 2.8 ms). Demo task end to end: p50 507 ms vs 376 ms direct (+35%) | Not run |

A real LLM also completed the demo task (a local 4B model under llama.cpp), including after being killed mid-run. Three earlier attempts failed; both the failures and the successful run are in `docs/results/`. This shows the loop works end to end with a real model, not that the agent is good.

The suites found seven bugs in the runtime itself (two of them only on the Ubuntu CI runs); each has a short note in [`docs/bugs.md`](docs/bugs.md). Three more (B-009 to B-011) were found after v0.1 by using the program by hand, in places no suite looked; they are in the same file.

## The bonus: a call that cannot be rolled back

v0.1 can always fall back on "put the folder back as it was". Built after v0.1, the bonus adds the case where that is impossible: a fifth tool, `ext.call`, asks a service outside the runtime to do something.

```toml
# policy: the agent names a service, never a URL; only this machine is allowed
[external]
timeout_ms = 2000
attempts   = 3
[[external.service]]
name = "ledger"
url  = "http://127.0.0.1:8099/apply"
```

```toml
# script step
[[step]]
tool    = "ext.call"
service = "ledger"
payload = "order-1"
```

- Before the request leaves, the intent records an **idempotency key** (`agentos-<execution id>-c<call id>`). The service applies a key at most once.
- A timeout does not say whether the service did the work. The gateway sends the *same* request with the *same* key again; if no attempt is answered, the run **stops** with the call uncertain instead of recording a guess.
- After a crash or such a stop, `agentos recover` sends the same request again. Answered: APPLIED, exactly once. Not answered: still uncertain, and the agent does not continue.

The service is a small fake (`tests/common/fake_service.rs`; run it with `cargo run --example fake_service`) that can lose a request or lose an answer on demand. Eight extra fault-injection cells kill the runtime around the request or make the service go silent, and check the service's own record:

| | `nix` preset, development machine | `fhs` preset, Ubuntu 24.04 in CI |
|---|---|---|
| 8 cells × 200 runs: killed before the intent, after it, in mid-request, after completion, during recovery; request lost; answer lost; no answer at all | 1,600 runs, **0 invariant violations**, **0 orders applied twice or lost**, 932 of 932 verdicts matching the service's record | 1,600 runs, **0 invariant violations**, **0 orders applied twice or lost**, 942 of 942 verdicts matching |
| The same harness against two seeded bugs ("say it failed", "retry with a new key"), 160 runs each | 48 and 70 doubled effects: it can fail | Not run |

Raw data: [`docs/results/recovery-ext-200-nix.md`](docs/results/recovery-ext-200-nix.md), [`recovery-ext-200-fhs.md`](docs/results/recovery-ext-200-fhs.md) (one CI run, [43436681321](https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321)), [`recovery-ext-mutations.md`](docs/results/recovery-ext-mutations.md).

It is off by default: the shipped policies have no `[external]` section, and the LLM agent is not offered the tool. Allowing it gives the agent a way to send data out; read [`docs/threat-model.md`](docs/threat-model.md) §7 first. Details: [`docs/recovery-semantics.md`](docs/recovery-semantics.md) §9.

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

[sandbox.env]                                       # extra variables for commands, fixed text
PYTHONDONTWRITEBYTECODE = "1"

[limits]
memory_mb = 512
pids = 128
cpu_percent = 200
call_timeout_s = 60
max_output_kb = 64
max_tool_calls = 50
```

### On your own project

The shipped policies fit the demo. For a real repository, copy one and change four things.

1. **Where the agent may write.** `[filesystem] write`: for example `["src/**", "tests/**", "pyproject.toml"]`. `fs.write` makes missing folders inside these paths. Anything else is refused, and what a command changes elsewhere is put back.
2. **Which commands it may run.** `[exec] allow`: argv prefixes, bare program names. Every allowed command is arbitrary code (your test suite runs your code); the sandbox is what confines it. There is no network in the sandbox, so a command that downloads something will fail.
3. **The packages your tests need.** The sandbox sees the copy of the repo and the folders in `sandbox.ro_mounts`, nothing else. Not your home folder, and not a virtualenv, even one inside the repo.
   - *A virtualenv:* add its folder to `ro_mounts` and start `agentos` with it activated. A mount may not be under `/tmp`.

     ```toml
     [sandbox]
     ro_mounts = ["/usr", "/bin", "/lib", "/lib64", "/home/me/proj/.venv"]
     ```
     ```sh
     source /home/me/proj/.venv/bin/activate
     agentos run --repo /home/me/proj --policy my-policy.toml --agent llm --task "..."
     ```
   - *Any other folder of packages:* mount it the same way and point at it with a variable:

     ```toml
     [sandbox.env]
     PYTHONPATH = "/home/me/deps"
     ```
   Everything under a mount can be read by the code the agent runs (never written). Do not mount a folder that holds secrets.
4. **The limits.** `call_timeout_s` (60) must be longer than your test suite takes; `max_tool_calls` (50) is the whole run's budget; `max_output_kb` (64) is also the largest file `fs.read` returns.

Things to know before the first run:

- **The result is a patch**, printed at the end as `patch: <path>`. Read it, then apply it in your repository with `git apply <path>` or `patch -p1 < <path>`. A patch is text: if the run added a binary file, changed a file's mode or made an empty folder, the run ends with `NOT in the patch: …` and the patch starts with one `# agentos:` line per such change, saying where to copy the file from.
- **`.git` is not in the workspace** (the shipped deny list leaves it out), so commands that ask git for something will not find a repository.
- **The whole repository is copied** at the start and again after every change. On btrfs and XFS the copies share disk blocks. Elsewhere, a repository with a large `node_modules` or `target/` folder is slow and takes space: 20,000 small files took 5.4 s for a 7-call run here, against 0.6 s for the demo.
- **If you stop a run** (Ctrl-C, or the terminal closes), nothing is printed. `agentos status` shows it as `interrupted`; `agentos run --resume <id>` settles the call that was cut off and goes on.
- **Nothing is deleted for you.** Each execution keeps its workspace and one checkpoint per change. Remove old ones with `rm -rf ~/.local/share/agentos/executions/<id>`.

## What it does not do

- **It is not hardened like a production container runtime.** No seccomp filter; kernel bugs are out of scope; tested on one kernel and one distribution.
- **No power-loss guarantee.** The tests kill the process; they do not cut power. Checkpoints are not fsynced.
- **No exactly-once execution.** A command that was cut off is rolled back and may be issued again by the agent as a new call. An external call is applied once only if the service honours the idempotency key.
- **No disk limit.** A command can fill the disk. Checkpoints are never pruned.
- **No network, ever**, for commands. No web search, no package downloads. The only thing that leaves the runtime is the bonus tool's request to a `127.0.0.1` service that a policy names, and the LLM request.
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
  tools/           fs.rs (openat2 file tools), exec.rs (program lookup, output capture), ext.rs (bonus: one HTTP request)
  sandbox/         ns.rs (namespaces, new root), cgroup.rs, landlock.rs, beneath.rs (openat2)
  log.rs           event log and the call state machine
  workspace.rs     the execution folder; copying the repo without denied files
  checkpoint.rs    copies, rollback, revert of disallowed changes, the patch
  recovery.rs      verdicts after a crash
  faults.rs        crash points for the tests
  agent/           scripted.rs, llm.rs
policies/          fhs.toml, nix.toml
tests/             security/ (32 attacks), recovery/ (fault injection), benign.rs, llm_agent.rs, vertical_slice.rs,
                   external.rs and common/fake_service.rs (bonus)
benches/, bench/   the three overhead benchmarks
examples/          demo-repo/, scripts/, sandbox_spawn.rs, fake_service.rs (bonus)
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
| [`docs/decisions/`](docs/decisions/) | 12 short ADRs for the hard-to-undo decisions |
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

## License

Licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](LICENSE-APACHE))
- MIT license ([`LICENSE-MIT`](LICENSE-MIT))

at your option. This is the usual choice for Rust projects (D-069).
