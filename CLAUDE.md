# AgentOS

A Rust runtime that runs a coding agent's tool calls inside a rootless Linux sandbox and survives `kill -9` without losing track of what happened. It is a learning and resume project, not a product.

## Documents

- `AgentOS v1 — Scoped Design.md` is **the plan**. Read it before any design or code work.
- `AgentOS_Project_Spec.md` is a "maybe someday" list. It never adds work to v1. If the two disagree, the v1 document wins.
- `docs/decision-log.md` records every decision taken or discarded, with reasons. Prince will study from it later, so it must stay complete.
- `docs/decisions/` holds the ADRs. `docs/threat-model.md`, `docs/recovery-semantics.md`, `docs/bugs.md` and `docs/results/` are the other documents of record.

## Keep the decision log up to date

This is required, not optional. In the same turn as the work, append to `docs/decision-log.md` whenever:

- a design or tooling choice is made, even a small one (a crate, a file format, an error-handling approach);
- an option is considered and rejected, including ideas Prince or Claude raised and dropped;
- an earlier decision changes (add a new entry and mark the old one `Replaced by D-0xx`; never rewrite history);
- an experiment, test or bug teaches something (add a row to "Findings");
- a new technical term appears (add it to the glossary).

Follow the entry template in that file. Write "In plain words" for a beginner. Under "Evidence", state only what was actually run or read, with the result; otherwise write "none; reasoning only". Do not invent reasons for past decisions: if the reason is unknown, say so.

## Scope is fixed

The five parts (gateway, policy, sandbox, event log, checkpoints + recovery), the build order, and the cut list in the v1 document do not change during normal work. The "Bonus" of the v1 document (one fake external tool) is built (M7, D-073); it is finished. On 2026-11-14 Prince approved exactly two more additions (D-088): packages from outside the repo for sandboxed commands, and an AppArmor profile for Ubuntu. Neither is a tool. Later the same day he opened scope for the parked conveniences (M9, D-098), except binary file tools, which stay parked. An `apply` command was held at first and approved on 2026-11-15 (D-108). On 2026-11-16 Prince approved the v1.0 scope (D-114; section "v1.0 scope" of the v1 document): seccomp, a measured disk limit, scratch folders out of checkpoints, a read-only folder in the workspace, `fs.edit`, binary files and modes in the file tools, a hosted-model trial, and release engineering (name, 1.0.0, CHANGELOG, compatibility promise, one more distribution in CI). The tool list is four plus the bonus plus `fs.edit`.

- Do not add features, tools, dependencies or abstractions that the v1 document does not list.
- If an idea comes up that is outside v1, add one line to the parking lot (§22 of the spec) and continue with the current milestone.
- If a v1 decision turns out not to work in practice, make the smallest change that fixes it, record it as a new decision-log entry that replaces the old one, update the v1 document to match, and continue. Do not use this to add scope.

## How to work: build without stopping, document everything

Prince wants the project built end to end without being asked questions along the way. He will learn afterwards from the written record, with the help of an AI. So the record is as important as the code.

- Work through the milestones in order. Do not pause to ask for approval or to check in.
- When a choice comes up, make it, log it, and move on.
- Stop only when something is needed that only Prince can give (a password, an API key, `sudo`, an account setting), or when continuing would mean faking a result. Say exactly what is needed.
- Never weaken, skip or delete a test to make a milestone pass. If an exit criterion cannot be met, say so in the milestone document and stop.
- Prince is new to Rust and Linux internals (namespaces, cgroups, Landlock, `openat2`, fsync). Write every document for that reader: plain words first, the real term next to it.
- Run the small experiments listed in the v1 milestone table before building each mechanism. Keep them in `experiments/` with a short note on what each showed.

## The storyline: commits (never push)

The git history is part of the documentation. Someone should be able to read it top to bottom and follow how the project was built.

- Commit in small steps: one idea per commit, and the code builds at each one.
- The commit subject says what changed. The body says why, in plain words, and names the decision-log entry if there is one (for example "See D-031").
- Commit code together with its tests and its log entries, not in separate later commits.
- **Never run `git push`, in any form, to any remote.** This includes pushing tags, and any other command that uploads the repository (for example `gh repo sync` or `gh pr create`). Prince pushes himself. All work stays in the local repository.
- Always commit when a milestone is achieved: the code, the tests, the log entries and the milestone document together, so the tag points at a complete milestone. Committing in between is allowed and expected.
- Never rewrite existing history (no amend of earlier commits, no rebase), and never commit secrets (API keys, tokens, `.env` files).
- In commit messages and pull request descriptions, do not add Co-Authored-By lines or any signature or attribution to Claude.
- Tag the end of each milestone locally: `m0`, `m1`, … and `v0.1` at the end. Since D-113: tag only after the CI run on that commit is green (`m9` and `m10` were tagged before, and sit on commits that fail on Ubuntu; M9 and M10 first pass there at `d98eab1`).

## One document per milestone

At the end of each milestone, write `docs/milestones/M<n>-<name>.md` from `docs/milestones/TEMPLATE.md`. It must cover the aim, the requirements and whether each was met, what was built, the code explained piece by piece, the decisions made, the problems hit, and how to run it. Fill it in while working, not from memory at the end. A milestone is not finished until its document is.

| Doc | Weeks | Milestone |
|---|---|---|
| `M0-rust-ramp.md` | 0 | Rust ramp |
| `M1-vertical-slice.md` | 1–2 | Gateway, tools, policy, scripted agent, event log, `trace` |
| `M2-sandbox.md` | 3–5 | Namespaces, cgroups, Landlock, `openat2`, presets |
| `M3-durability.md` | 6–8 | State machine, checkpoints, recovery, fault injection |
| `M4-llm-agent.md` | 9 | LLM agent loop |
| `M5-benchmarks.md` | 10 | Benchmarks, full security suite |
| `M6-release.md` | 11–12 | README, threat model, recovery semantics, v0.1 |
| `M7-bonus-external-tool.md` | after v0.1 | Bonus: `ext.call`, the fake service, its reconciliation rule and fault cells |
| `M10-apply.md` | after M9 | `agentos apply`: the result copied into the repository by a command the user runs (D-108) |
| `M9-conveniences.md` | after M8 | `agentos rm` and `check`, self-start in a scope, a message when stopped, `fs.read` with a line range, policy lists `skip`, `scratch`, `deny_except`, a note for an inactive virtualenv, `stopped` when the model gives up |
| `M11-hardening.md` | v1.0 | seccomp, measured disk limit, scratch out of checkpoints (D-114) |
| `M12-real-projects.md` | v1.0 | `fs.edit`, read-only folder in the workspace, binary files and modes |
| `M13-hosted-model.md` | v1.0 | A capable hosted model through the six inventory tickets |
| `M14-release.md` | v1.0 | Name, 1.0.0, CHANGELOG, compatibility promise, another distribution, tag `v1.0` |
| `M8-first-user.md` | after M7 | Bugs found by hand (B-009 to B-013), folder-making writes, first messages, the patch's notes, packages from outside the repo, the AppArmor profile, the first trial on a real project |

## Design rules that must hold

1. The agent never touches the system directly. Every effect goes through the gateway.
2. No policy match means deny.
3. An intent is written and fsynced before its effect starts.
4. Order for a mutating call: effect → checkpoint → COMPLETED record that names the checkpoint.
5. An UNCERTAIN call is never re-executed before it is reconciled. For `ext.call`, reconciling *is* sending the same request with the same idempotency key; no answer means it stays uncertain and the agent does not continue (D-078).
6. Files matching deny globs are never copied into the workspace.
7. Paths are confined by the kernel (`openat2`, mount namespace, Landlock), never by string checks.
8. Commands are argv arrays run with `execve`. Never build a shell string.
9. The event log and checkpoints are never visible inside the sandbox.
10. A run never modifies the original repo. Its output is a patch. Only `agentos apply`, a separate command the user runs afterwards, writes into a repository, and only if it is unchanged since the copy (D-108).

## Portability

- Linux only. Development is on NixOS, but nothing in `src/` may assume NixOS.
- Distro differences live in policy presets (`fhs`, `nix`), not in code.
- Both test suites must pass on both presets. The `nix` preset is tested on the development machine. The `fhs` preset is tested in CI on an Ubuntu runner, which only runs after Prince pushes. A change is "not yet verified" on `fhs` until a CI run on that commit is green; never record it as passing before that.
- The runtime checks for user namespaces, Landlock, `openat2` and cgroup delegation at startup and fails with a clear message.

## Claims and tests

- A feature is done only when it has tests.
- No number goes in the README or a resume bullet without a script that produces it.
- Do not claim power-loss safety, exactly-once execution, or perfect isolation. The tests kill the process; they do not prove more than that.
- Bugs found by the fault-injection harness or the security suite get a short note in `docs/` when they are found. They are the best interview material.

## Code style

- One crate, synchronous Rust, threads where needed. No async runtime.
- `anyhow` at the top level, `thiserror` for errors that callers match on.
- Few dependencies. Adding one that the v1 stack table does not list needs a decision-log entry saying why.
- Comments say why, not what. Every `unsafe` block and raw syscall gets a comment on what it relies on.

## Commands

Run everything inside the dev shell (`flake.nix`): `nix develop -c <command>`, or `nix develop` once for an interactive shell.

- Since M9, `agentos run` and `agentos check` move themselves into a delegated scope (D-105); the `systemd-run` prefix in the commands below is still correct and still what `make` and CI use. `AGENTOS_IN_SCOPE=1` switches the self-start off.
- Before a first run on a repo: `agentos check --policy <file> --repo <folder>`. After a run: read `result.patch`, then `agentos apply <id>` (`--dry-run` first; `--to <folder>` for another copy). Tests: `tests/apply.rs`. Clean up: `agentos rm <id>`, `--older-than <days>`, `--all`, `--dry-run`.
- Build and test: `nix develop -c make test` (= `systemd-run --user --scope -p Delegate=yes cargo test`: unit tests, `tests/vertical_slice.rs`, `tests/security/`, and `tests/recovery/` with 3 kills per cell). Without the delegated cgroup the sandbox tests fail at startup with a message saying so. `AGENTOS_TEST_PRESET=fhs` switches the policy preset.
- Fault-injection suite at full size: `nix develop -c make eval-recovery` (200 kills per cell, about 8 minutes; `RUNS=20` for a quick look). Results of record go in `docs/results/`.
- Crash by hand: `AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=6 agentos run ...`, then `agentos recover <id>` and `agentos run --resume <id>` (see `src/faults.rs`).
- LLM agent: start any OpenAI-compatible server (for llama.cpp: `llama-server -m <model>.gguf --jinja --port 8080`), set `AGENTOS_LLM_BASE_URL` / `AGENTOS_LLM_MODEL` (and `AGENTOS_LLM_API_KEY` for a hosted API), then `agentos run --agent llm --task "..." --repo ... --policy ...`. `tests/llm_agent.rs` uses a mock endpoint and needs no model. Never use a credential found in the environment that Prince did not give for this project (D-058).
- The three suites of the v1 design: `nix develop -c make eval` (= `eval-recovery`, `eval-security`, `bench`). Benchmarks alone: `make bench`; raw data and `summary.md` land in `docs/results/bench-<preset>/`. No number goes into a document unless it is in `docs/results/`.
- Security suite only: `make eval-security` (32 adversarial cases plus the benign suite).
- Lint: `nix develop -c cargo clippy --all-targets`.
- Demo: `nix develop -c make demo` (the fix, then a deliberate kill, recover and resume). By hand: `systemd-run --user --scope -p Delegate=yes cargo run -- run --repo examples/demo-repo --policy policies/nix.toml --agent scripted --script examples/scripts/fix-demo.toml`, then `cargo run -- trace`.
- M0 practice program: `cd experiments/m0-ramp && cargo test`. Other experiments: see each `experiments/*/NOTES.md`.
- Bonus tool by hand: `cargo run --example fake_service` (port 8099; `FAKE_SERVICE_FAULTS='order-1=drop_after'` loses an answer), add the `[external]` section it prints to a copy of a policy, and use a script step `tool = "ext.call"`. Tests: `tests/external.rs`; the external fault cells are the second test in `tests/recovery/main.rs` (report: `target/recovery-ext-report.json`).
- The compatibility promise (`docs/compatibility.md`, D-126): `tests/compat.rs`, with frozen v0.1 files in `tests/fixtures/compat/` (never regenerate them). User-facing changes go in `CHANGELOG.md`.
- Binary files (base64) and the executable bit through the file tools (D-123): `tests/binary.rs`.
- A repository folder shown read-only at its place (`filesystem.ro_dirs`, D-122): `tests/rodirs.rs`.
- `fs.edit` (D-120): `tests/edit.rs`; its kill cells are the test `an_edit_is_recovered_like_a_write` in `tests/recovery/main.rs`.
- The disk limit `limits.workspace_mb` (D-117): the last four tests of `tests/hardening.rs`.
- Hostile leftovers and folder-making writes: `tests/hardening.rs` (part of `make test`; alone: `systemd-run --user --scope -p Delegate=yes cargo test --test hardening`).
- Packages from outside the repo (a mounted virtualenv, `[sandbox.env]`): `tests/deps.rs`. First messages, the patch's notes, the startup probe: `tests/hardening.rs`.
- Ubuntu without the system-wide sysctl: `packaging/apparmor/agentos` (install steps in its header and in the README); CI job `fhs-apparmor`.
- Bugs the suites find in the runtime get a note in `docs/bugs.md`.
- CI: `gh run list --limit 5`, `gh run view <id> --log-failed`, `gh run download <id> -n recovery-report-fhs`.

## Current status

v0.1 tagged locally (tags `m0`–`m6`, `v0.1`). All seven milestones are done on the `nix` preset. The `fhs` preset is verified by CI run 43385661105 (2026-11-13, commit `c3c9502`, Ubuntu 24.04; D-072): lint, all test suites and the 200-kill suite pass. Two earlier runs failed and found B-007 and B-008. Results of record: `docs/results/recovery-200-fhs.md`, `security-fhs.md`. Not run on Ubuntu: benchmarks, `make demo`.

The bonus (M7, tag `m7` locally) is built on top of v0.1: the tool `ext.call`, the fake service, its recovery rule (D-073 to D-081, ADR 0012). On the `nix` preset the external cells give 1,600 runs with 0 violations and 0 duplicated effects (`docs/results/recovery-ext-200-nix.md`). On `fhs` the same, in CI run 43436681321 on `1bc27da` (D-082, `docs/results/recovery-ext-200-fhs.md`).

`main` and the tags `m0`–`m7`, `v0.1` are on GitHub (`pessimistic-zer0/agentos`). They were pushed once by the agent at Prince's direct request (D-083); the rule above is unchanged: the agent does not push.

M8 (tag `m8` locally; `docs/milestones/M8-first-user.md`) is the work after the bonus, asked for by Prince on 2026-11-13 and 2026-11-14:

- Bugs found by using the program by hand, all fixed: B-009 to B-013 (`docs/bugs.md`). `fs.write` makes missing folders (D-084).
- What a first user needs (D-088): clearer first messages and an early check for the LLM server (D-089); a patch that lists what it cannot carry (D-090, D-095); packages from outside the repo (D-091, D-092; `tests/deps.rs`); an AppArmor profile and a startup probe that tries (D-093, D-096; `packaging/apparmor/agentos`).
- First trial on a project that is not the demo: `docs/results/real-repo-trial-nix.md` (the test project is in `~/Temp/AgentOS_temp/`, outside this repository). Ideas it raised are parked in the spec, §22.
- On `nix`: `make test` 105 passed; 200-kill run 4,800 + 1,600 runs with 0 violations (`docs/results/recovery-200-nix-m8.md`); benchmarks re-run (`docs/results/bench-nix-m8/`).
- On `fhs`: CI run 43612319559 on `69cdbf0` (D-097), both jobs green: `fhs` (105 tests, `make demo`, the 200-kill run) and `fhs-apparmor` (restriction on; refuses without the profile; runs with the profile installed at `/usr/local/bin/agentos` as the README says, and four suites). Earlier M8 runs: 43573592353 (D-087), 43590153676 (D-094). Not run on Ubuntu: benchmarks.

M9 (tag `m9` locally; `docs/milestones/M9-conveniences.md`), 2026-11-15: ten of the parked ideas (D-098 to D-107). On `nix`: `make test` 118 passed; the real-project session repeated (`docs/results/real-repo-trial-nix.md`, second part); 200-kill run in `docs/results/recovery-200-nix-m9.md`. Verified on `fhs` together with M10 (D-110). Held at the time: `agentos apply` (built next, M10) and binary file tools. The model-server error met in the re-trial is handled since D-111.

`agentos apply` (M10, tag `m10` locally; `docs/milestones/M10-apply.md`; D-108), 2026-11-15: copies a finished execution's result into the repository, including what a patch cannot carry; refuses if the repository changed since the copy. `tests/apply.rs` (6 tests). On `nix`: `make test` 125 passed. On `fhs`: CI run 43691133260 on `d98eab1` (D-110), both jobs green: 125 tests, `make demo`, the 200-kill run, and the AppArmor job. The run before it (43690755406) failed on a false note that only Ubuntu could show (B-014, D-109). Still parked: binary files and file modes through the file tools.

After M10, 2026-11-15, no new tag yet: the model server's answers are sorted into wait, error and tell-the-model (D-111; verified on `fhs` by CI run 43733350171 on `2a6a3d1`, D-113), and helper programs for commands (`exec.helpers`, D-112), found in a trial on a Node and a Rust project (`docs/results/second-stack-trial-nix.md`; the two projects are in `~/Temp/AgentOS_temp/`). On `nix`: `make test` 130 passed. D-112 is verified on `fhs` by CI run 43844369387 (D-119). `main`, and the tags up to `m10`, are on GitHub, pushed by Prince.

v1.0 is in progress (D-114), milestones M11 to M14.

M11 (tag `m11` locally, on `fc31fbb`; `docs/milestones/M11-hardening.md`), 2026-11-16: the TOCTOU test repeats its race until it happened often enough (D-115, after CI run 43742105191 failed on 925 swaps); the seccomp filter (D-116); the measured disk limit `limits.workspace_mb` (D-117); scratch paths left out of checkpoints (D-118). On `nix`: `make test` 138 passed; 200-kill run in `docs/results/recovery-200-nix-m11.md`. On `fhs`: CI runs 43844369387 on `caeb948` (D-119) and 43849008533 on `fc31fbb` (D-121), both jobs green. Commit `048e7e5` has four failing tests, fixed in `b340a51`.

M12 (`docs/milestones/M12-real-projects.md`), no tag yet: `fs.edit` (D-120), a read-only repository folder inside the workspace (`filesystem.ro_dirs`, D-122), binary files and the executable bit through the file tools (D-123). On `nix`: `make test` 163 passed. On `fhs`: D-120 and D-122 are verified by CI run 43894443267 on `b66298a` (D-125); D-123, and D-124 (a busy model server that names a wait is given it), are **not yet verified on `fhs`**. With `fs.edit` the local 4B model solved the two-module ticket it had failed three times (`docs/results/real-repo-trial-nix.md`, last part).

M14 (`docs/milestones/M14-release.md`) is started: the compatibility promise with tests against frozen v0.1 files (D-126) and `CHANGELOG.md`; `make test` 167 passed on `nix`, not yet verified on `fhs`. Open in M14: a release build and benchmarks in CI, install instructions, version 1.0.0, another distribution in CI, the name.

M13 (`docs/milestones/M13-hosted-model.md`) is **blocked** after one of six tickets: the Groq key's tier allows 8,000 tokens per minute, which is also the largest request, and the conversations need more (`docs/results/hosted-model-trial-nix.md`). INV-101 was solved and checked. The five other tickets were not run.

Open, and only Prince can close them:

- A key for M13 with a much higher tokens-per-minute limit (a higher Groq tier, or another OpenAI-compatible provider).
- Decide the project name (D-023, D-068), before the `v1.0` tag (D-114). The license is settled: dual MIT OR Apache-2.0 (D-069).

Update this section when any of these changes.
