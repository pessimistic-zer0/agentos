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

The five parts (gateway, policy, sandbox, event log, checkpoints + recovery), the build order, and the cut list in the v1 document do not change during normal work. The "Bonus" of the v1 document (one fake external tool) is built (M7, D-073); it is the only addition and it is finished. It does not open the door to more tools.

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
- Tag the end of each milestone locally: `m0`, `m1`, … and `v0.1` at the end.

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
10. The original repo is never modified. The output is a patch.

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
- Bugs the suites find in the runtime get a note in `docs/bugs.md`.
- CI: `gh run list --limit 5`, `gh run view <id> --log-failed`, `gh run download <id> -n recovery-report-fhs`.

## Current status

v0.1 tagged locally (tags `m0`–`m6`, `v0.1`). All seven milestones are done on the `nix` preset. The `fhs` preset is verified by CI run 43385661105 (2026-11-13, commit `c3c9502`, Ubuntu 24.04; D-072): lint, all test suites and the 200-kill suite pass. Two earlier runs failed and found B-007 and B-008. Results of record: `docs/results/recovery-200-fhs.md`, `security-fhs.md`. Not run on Ubuntu: benchmarks, `make demo`.

The bonus (M7, tag `m7` locally) is built on top of v0.1: the tool `ext.call`, the fake service, its recovery rule (D-073 to D-081, ADR 0012). On the `nix` preset the external cells give 1,600 runs with 0 violations and 0 duplicated effects (`docs/results/recovery-ext-200-nix.md`).

Open, and only Prince can close them:

- Push the tags (`m0`–`m6`, `v0.1`); `main` is on GitHub (`pessimistic-zer0/agentos`), the tags are not.
- Decide the project name (D-023, D-068). The license is settled: dual MIT OR Apache-2.0 (D-069).
- Push the bonus commits (tag `m7`). The bonus is built and passes on the `nix` preset; it is "not yet verified" on `fhs` until a CI run on those commits is green.

Update this section when any of these changes.
