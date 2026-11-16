# AgentOS

A small Rust runtime that runs a coding agent's tool calls inside a rootless Linux sandbox, and survives `kill -9` at any moment without losing track of what happened.

The agent (a script, or an LLM) never touches anything itself. It asks; a gateway checks a policy, writes down what is about to happen, does it, and writes down that it is done. If the runtime dies in between, recovery can say for every call whether it happened, and it never silently runs an uncertain call again.

It is a learning and portfolio project, not a product. It does one thing: one agent, one copy of a repository, five tools (plus one bonus tool, off by default), a command line.

## Status

**Usable on a real project, on two tested systems, with its limits written down.** The last tagged release is `v0.1`; the work since then is on `main`.

- **What it has been run on.** The demo; a Python project with real PyPI packages in a virtualenv; a Node project with npm packages; a Rust project. On each, tickets were fixed through the gateway and the result applied to a fresh clone, where the project's own tests passed ([`docs/results/real-repo-trial-nix.md`](docs/results/real-repo-trial-nix.md), [`second-stack-trial-nix.md`](docs/results/second-stack-trial-nix.md)). A local 4-billion-parameter model solved a one-line ticket through it and reached its own limit on a two-file one.
- **Where it is verified.** NixOS 26.05 on the development machine (`nix` preset), and Ubuntu 24.04 in CI (`fhs` preset), including a job that keeps Ubuntu's restriction on user namespaces switched on and uses the AppArmor profile that comes with agentos. Latest green run on everything but the newest change: [43733350171](https://github.com/pessimistic-zer0/agentos/actions/runs/43733350171). Debian, Fedora and Arch use the same `fhs` preset and have not been run.
- **What the suites say.** 4,800 kills at 8 points with 0 invariant violations, 32 attacks with 0 escapes, 130 tests in all. See [What it shows](#what-it-shows).
- **What it is not.** Not hardened like a container runtime (a seccomp deny list but no allow list, a measured disk limit but no hard cap), no network for commands, one agent at a time. Read [what it does not do](#what-it-does-not-do) before trusting it with anything.
- **How it got here.** Fourteen bugs were found on the way: eight by the suites and the first CI runs, and six after every suite had passed (four by using the program on things the suites did not try, one by the benchmarks, one by CI on the other system). Each has a note in [`docs/bugs.md`](docs/bugs.md), and the six are told as a story in [`docs/what-the-suites-missed.md`](docs/what-the-suites-missed.md). The history by milestone is in [`docs/milestones/`](docs/milestones/) (M0 to M10).
- Licensed under MIT or Apache-2.0, at your option (see [License](#license)).

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
| **Overhead** (release build, i5-13420H, btrfs). | Sandboxed `true`: p50 5.6 ms, p99 8.4 ms (direct: 1.1 / 1.5 ms). `fs.write` 4 KB with fsynced log and checkpoint: p50 13.3 ms, p99 21.3 ms (durable direct write: 1.9 / 2.7 ms). Demo task end to end: p50 481 ms vs 354 ms direct (+36%). Measured on 2026-11-14 at commit `15fab27` (`docs/results/bench-nix-m8/`); the v0.1 run is kept in `docs/results/bench-nix/`. The direct runs were faster too, so the difference between the two runs is the machine's state, not the code | Not run |

A real LLM also completed the demo task (a local 4B model under llama.cpp), including after being killed mid-run. Three earlier attempts failed; both the failures and the successful run are in `docs/results/`. This shows the loop works end to end with a real model, not that the agent is good.

The suites and the first CI runs found eight bugs in the runtime itself (two of them only on Ubuntu). Six more were found after v0.1, when every suite passed: by using the program by hand, by a project written by someone else, by the benchmarks, and by CI on Ubuntu. Each has a short note in [`docs/bugs.md`](docs/bugs.md).

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

Commands run in a cgroup that systemd delegated to you (no root needed). `agentos run` and `agentos check` move themselves into one, by starting themselves again through `systemd-run --user --scope -p Delegate=yes`; you do not type that.

```sh
alias agentos=target/debug/agentos

agentos check --policy policies/nix.toml --repo <repo>     # the machine, the policy, what would be copied; starts nothing
agentos run --repo <repo> --policy policies/nix.toml --agent scripted --script <steps.toml>
agentos run --repo <repo> --policy policies/nix.toml --agent llm --task "Make the failing test pass"
agentos status                 # all executions; "interrupted" means it was stopped or killed
agentos trace [<id>]           # one line per call; --events for every record; --json for scripts
agentos recover <id>           # settle the call that was cut off, repair the workspace
agentos run --resume <id>      # recover, then let the agent continue
agentos apply <id>             # copy the result into the repo (after reading the patch); --dry-run, --to <folder>
agentos rm <id>...             # delete executions; also --older-than <days>, --all, --dry-run
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
deny_except = [".env.example", ".env.sample", ".env.template", "**/.env.example", "**/.env.sample", "**/.env.template"]
skip    = []                       # not copied into the workspace, e.g. [".venv/**"]
scratch = [".pytest_cache/**"]     # caches: commands may write here; not reported, not in the patch or checkpoints

[exec]
allow = [["pytest"], ["python3", "-m", "pytest"]]   # argv prefixes
helpers = []                                        # programs those commands start: ["cc"], ["sh"]

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
workspace_mb   = 2048
```

### On your own project

The shipped policies fit the demo. For a real repository, copy one and change four things.

1. **Where the agent may write.** `[filesystem] write`: for example `["src/**", "tests/**", "pyproject.toml"]`. `fs.write` makes missing folders inside these paths. Anything else is refused, and what a command changes elsewhere is put back.
2. **Which commands it may run.** `[exec] allow`: argv prefixes, bare program names. Every allowed command is arbitrary code (your test suite runs your code); the sandbox is what confines it. There is no network in the sandbox, so a command that downloads something will fail: dependencies must already be on disk. If a command starts other programs by name, list them in `[exec] helpers`; the agent cannot run those itself. Two that were tried: `allow = [["cargo", "test"]]` needs `helpers = ["cc"]` and `scratch = ["target/**"]`; `allow = [["npm", "test"]]` needs `helpers = ["sh"]` (`[["node", "--test"]]` needs none).
3. **The packages your tests need.** The sandbox sees the copy of the repo and the folders in `sandbox.ro_mounts`, nothing else. Not your home folder, and not a virtualenv, even one inside the repo.
   - *A virtualenv:* add its folder to `ro_mounts`, start `agentos` with it activated, and if it lives inside the repo put it in `skip` so that it is not copied. A mount may not be under `/tmp`. If you forget to activate it, `agentos` prints a note.

     ```toml
     [filesystem]
     skip = [".venv/**"]
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
   - *A folder that must be at its place in the project* (`node_modules`): name it in `ro_dirs`. It is not copied; commands see the repository's own folder there, read-only, and `fs.read` and `fs.list` read it from the repository.

     ```toml
     [filesystem]
     ro_dirs = ["node_modules"]
     ```
   Everything under a mount or a read-only folder can be read by the code the agent runs (never written). Do not mount a folder that holds secrets. A read-only folder is the live original: it is not in checkpoints, and the run will not start if something in it matches a deny rule.
4. **The limits.** `call_timeout_s` (60) must be longer than your test suite takes; `max_tool_calls` (50) is the whole run's budget; `max_output_kb` (64) is also the largest file `fs.read` returns; `workspace_mb` (2048) is how large the workspace may get before a command is stopped and undone.

Then run `agentos check --policy my-policy.toml --repo /home/me/proj`. It says what `run` would refuse, and how many files and megabytes would be copied; if that number is large, add the big folders to `skip`.

Things to know before the first run:

- **The result is a patch**, printed at the end as `patch: <path>`. A run never changes your repository. Read the patch, then either apply it yourself (`git apply <path>` or `patch -p1 < <path>`) or run `agentos apply <id>`, which copies every change into the repository the run was started on. A patch is text: if the run added a binary file or an empty file, changed a file's mode or made an empty folder, the run ends with `NOT in the patch: …` and the patch lists them; `agentos apply` carries them across. It refuses, and changes nothing, if any file the run changed has changed in your repository since; then the patch is the way, because merging is yours to do. `agentos apply <id> --dry-run` shows the list first.
- **The file tools are made for text.** `fs.read` returns at most `max_output_kb` at a time; for a larger file the agent asks for lines (`start_line`, `max_lines`). `fs.write` replaces a whole file; `fs.edit` replaces one piece of text in an existing file, which must occur in it exactly once. A binary file is read and written whole, as base64 (`encoding = "base64"`), so only small ones fit: about three quarters of `max_output_kb`. `fs.write` can make a file executable or not (`executable`); other permission bits only a command can set.
- **`.git` is not in the workspace** (the shipped deny list leaves it out), so commands that ask git for something will not find a repository.
- **The whole workspace is copied** at the start and again after every change. On btrfs and XFS the copies share disk blocks. Elsewhere a repository with a large `node_modules` or `target/` folder is slow and takes space: 20,000 small files took 5.4 s for a 7-call run here, against 0.6 s for the demo. `skip` leaves a folder out, but then it is not in the workspace at all; that suits a virtualenv (it is used from its own path) and does not suit `node_modules`.
- **If you stop a run** (Ctrl-C, or the terminal closes), it says how to go on: `agentos run --resume <id>`. `agentos status` shows it as `interrupted`.
- **If the model gives up**, the run ends as `STOPPED`, not `COMPLETED`: a model that answers with words while its last command is still failing is reminded once, then the run is closed with its last words.
- **Nothing is deleted for you.** Each execution keeps its workspace and one checkpoint per change. `agentos rm --older-than 14` removes old ones; it keeps executions that are running or can still be resumed.

## What it does not do

- **It is not hardened like a production container runtime.** The seccomp filter is a short deny list, not an allow list; kernel bugs are out of scope; tested on one kernel and one distribution.
- **No power-loss guarantee.** The tests kill the process; they do not cut power. Checkpoints are not fsynced.
- **No exactly-once execution.** A command that was cut off is rolled back and may be issued again by the agent as a new call. An external call is applied once only if the service honours the idempotency key.
- **The disk limit is measured, not a hard cap.** `limits.workspace_mb` stops and undoes a command that takes the workspace past it, but the size is measured every 50 ms, so a fast writer can pass it for a moment. Checkpoints are never pruned, and each holds a copy of the workspace.
- **No network, ever**, for commands. No web search, no package downloads. The only thing that leaves the runtime is the bonus tool's request to a `127.0.0.1` service that a policy names, and the LLM request.
- **The sandbox can see** the read-only system folders in full (on NixOS, all of `/nix/store`) and some facts about the host (host name, kernel version, the workspace's path).
- **Prompt injection is contained, not prevented.** A file can talk the model into asking for something bad; the policy and sandbox then refuse or confine it.
- **Containment ends at the patch.** A patch can change test hooks or CI files that later run unsandboxed on your machine. Read it before applying. The same goes for `agentos apply`: it copies what the agent made, executable bits and symlinks included, and lists each one.
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
| [`docs/bugs.md`](docs/bugs.md) | Every bug found in the runtime: what was done, what went wrong, why, the fix, the lesson |
| [`docs/what-the-suites-missed.md`](docs/what-the-suites-missed.md) | A short write-up of the six bugs found after every suite had passed |
| [`docs/project-overview.md`](docs/project-overview.md) | One document that explains the whole project in plain words |
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
