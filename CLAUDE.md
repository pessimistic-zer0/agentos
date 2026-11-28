# Norn

A Rust runtime that runs a coding agent's tool calls inside a rootless Linux sandbox and survives `kill -9` without losing track of what happened. It is a learning and resume project, not a product.

## Documents

- `AgentOS v1 — Scoped Design.md` is **the plan**. Read it before any design or code work.
- `AgentOS_Project_Spec.md` is a "maybe someday" list. It never adds work to v1. If the two disagree, the v1 document wins.
- `docs/decision-log.md` records every decision taken or discarded, with reasons. Prince will study from it later, so it must stay complete.
- `docs/v4-scope.md` is **the plan for v4** (approved 2026-11-28, D-169): TraceWeaver, new ways to read a run's record: timeline, inspector, causal links, per-call diffs, replay up to the first divergence, run diff, an OpenTelemetry file, search. Milestones M23 to M26, released as 1.3.0.
- `docs/v3-scope.md` is **the plan for v3** (approved 2026-11-26, D-153): network allow-list, approvals, secrets used by the runtime, released as 1.2.0. Milestones M19 to M22.
- `docs/v2-scope.md` is **the plan for v2** (approved 2026-11-26, D-139): norn as an MCP server, M15 to M18, released as 1.1.0. The order of the versions after it is D-137; each needs its own approval.
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

The five parts (gateway, policy, sandbox, event log, checkpoints + recovery), the build order, and the cut list in the v1 document do not change during normal work. The "Bonus" of the v1 document (one fake external tool) is built (M7, D-073); it is finished. On 2026-11-14 Prince approved exactly two more additions (D-088): packages from outside the repo for sandboxed commands, and an AppArmor profile for Ubuntu. Neither is a tool. Later the same day he opened scope for the parked conveniences (M9, D-098), except binary file tools, which stay parked. An `apply` command was held at first and approved on 2026-11-15 (D-108). On 2026-11-16 Prince approved the v1.0 scope (D-114; section "v1.0 scope" of the v1 document): seccomp, a measured disk limit, scratch folders out of checkpoints, a read-only folder in the workspace, `fs.edit`, binary files and modes in the file tools, a hosted-model trial, and release engineering (name, 1.0.0, CHANGELOG, compatibility promise, one more distribution in CI). The tool list is four plus the bonus plus `fs.edit`. On 2026-11-26 Prince approved the v2 scope (D-139; `docs/v2-scope.md`): `norn mcp` over stdio, resuming a killed session, the trial fixes (test baseline, denials that say what is allowed, `fs.search`, `fs.delete`, `replace_all`, ending at once on `413`), and a trial with Claude Code. The tool list becomes eight with `fs.search` and `fs.delete`.

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
| `M10-apply.md` | after M9 | `norn apply`: the result copied into the repository by a command the user runs (D-108) |
| `M9-conveniences.md` | after M8 | `norn rm` and `check`, self-start in a scope, a message when stopped, `fs.read` with a line range, policy lists `skip`, `scratch`, `deny_except`, a note for an inactive virtualenv, `stopped` when the model gives up |
| `M11-hardening.md` | v1.0 | seccomp, measured disk limit, scratch out of checkpoints (D-114) |
| `M12-real-projects.md` | v1.0 | `fs.edit`, read-only folder in the workspace, binary files and modes |
| `M13-hosted-model.md` | v1.0 | A capable hosted model through the six inventory tickets |
| `M14-release.md` | v1.0 | Name, 1.0.0, CHANGELOG, compatibility promise, another distribution, tag `v1.0` |
| `M15-mcp-server.md` | v2 | `norn mcp`: MCP over stdio, one session is one execution (D-139) |
| `M16-mcp-recovery.md` | v2 | `norn mcp --resume`, kill cells and security cases through MCP |
| `M17-agent-friction.md` | v2 | Test baseline, denials that say what is allowed, `fs.search`, `fs.delete`, `replace_all`, end on `413` |
| `M18-mcp-trial-release.md` | v2 | Claude Code on the six tickets through norn; documents; 1.1.0, tag `v1.1` |
| `M19-network.md` | v3 | `[network] allow`: a proxy in the runtime, a forwarder in the sandbox, connections in the log (D-153) |
| `M20-approvals.md` | v3 | `[approvals] require`, `norn approve` / `norn deny`, exact and expiring |
| `M21-secrets.md` | v3 | `[secrets]`, `ext.call` to HTTPS services with a secret header, redaction |
| `M22-v3-trial-release.md` | v3 | Security cases, a trial with a local fake HTTPS service, documents, 1.2.0, tag `v1.2` |
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
10. A run never modifies the original repo. Its output is a patch. Only `norn apply`, a separate command the user runs afterwards, writes into a repository, and only if it is unchanged since the copy (D-108).

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

- Since M9, `norn run` and `norn check` move themselves into a delegated scope (D-105); the `systemd-run` prefix in the commands below is still correct and still what `make` and CI use. `NORN_IN_SCOPE=1` switches the self-start off.
- Before a first run on a repo: `norn check --policy <file> --repo <folder>`. After a run: read `result.patch`, then `norn apply <id>` (`--dry-run` first; `--to <folder>` for another copy). Tests: `tests/apply.rs`. Clean up: `norn rm <id>`, `--older-than <days>`, `--all`, `--dry-run`.
- Build and test: `nix develop -c make test` (= `systemd-run --user --scope -p Delegate=yes cargo test`: unit tests, `tests/vertical_slice.rs`, `tests/security/`, and `tests/recovery/` with 3 kills per cell). Without the delegated cgroup the sandbox tests fail at startup with a message saying so. `NORN_TEST_PRESET=fhs` switches the policy preset.
- Fault-injection suite at full size: `nix develop -c make eval-recovery` (200 kills per cell, about 8 minutes; `RUNS=20` for a quick look). Results of record go in `docs/results/`.
- Crash by hand: `NORN_CRASH_AT=P4 NORN_CRASH_CALL=6 norn run ...`, then `norn recover <id>` and `norn run --resume <id>` (see `src/faults.rs`).
- LLM agent: start any OpenAI-compatible server (for llama.cpp: `llama-server -m <model>.gguf --jinja --port 8080`), set `NORN_LLM_BASE_URL` / `NORN_LLM_MODEL` (and `NORN_LLM_API_KEY` for a hosted API), then `norn run --agent llm --task "..." --repo ... --policy ...`. `tests/llm_agent.rs` uses a mock endpoint and needs no model. Never use a credential found in the environment that Prince did not give for this project (D-058).
- The three suites of the v1 design: `nix develop -c make eval` (= `eval-recovery`, `eval-security`, `bench`). Benchmarks alone: `make bench`; raw data and `summary.md` land in `docs/results/bench-<preset>/`. No number goes into a document unless it is in `docs/results/`.
- Security suite only: `make eval-security` (32 adversarial cases plus the benign suite).
- Lint: `nix develop -c cargo clippy --all-targets`.
- Demo: `nix develop -c make demo` (the fix, then a deliberate kill, recover and resume). By hand: `systemd-run --user --scope -p Delegate=yes cargo run -- run --repo examples/demo-repo --policy policies/nix.toml --agent scripted --script examples/scripts/fix-demo.toml`, then `cargo run -- trace`.
- M0 practice program: `cd experiments/m0-ramp && cargo test`. Other experiments: see each `experiments/*/NOTES.md`.
- MCP server (v2, M15): `norn mcp --repo <folder> --policy <file>` over stdio; with Claude Code: `claude --mcp-config <file> --strict-mcp-config --tools "" --allowedTools mcp__norn` (inside the dev shell, so the policy's programs are on `PATH`). `norn finish <id>`, then `norn apply <id>`. Tests: `tests/mcp.rs`; client in `tests/common/mcp_client.rs`; experiments: `experiments/m15-mcp/NOTES.md`. Kill cells through MCP: `recovery_holds_through_mcp` in `tests/recovery/main.rs` (report `target/recovery-mcp-report.json`). `make test` runs the security suite a second time through MCP (`NORN_SECURITY_FRONT=mcp`, D-144).
- Bonus tool by hand: `cargo run --example fake_service` (port 8099; `FAKE_SERVICE_FAULTS='order-1=drop_after'` loses an answer), add the `[external]` section it prints to a copy of a policy, and use a script step `tool = "ext.call"`. Tests: `tests/external.rs`; the external fault cells are the second test in `tests/recovery/main.rs` (report: `target/recovery-ext-report.json`).
- Another distribution: `ci/debian-vm.sh` runs `make test` on Debian 12 in a QEMU/KVM virtual machine (needs `qemu-system-x86_64` and `qemu-img` on `PATH`; D-128). CI job `debian` does the same.
- Release archive: `nix develop -c packaging/release.sh --check` (builds, packs into `target/dist/`, runs the demo task from the unpacked archive; D-127). CI job `release` does the same on Ubuntu and then runs the benchmarks.
- The compatibility promise (`docs/compatibility.md`, D-126): `tests/compat.rs`, with frozen v0.1 files in `tests/fixtures/compat/` (never regenerate them). User-facing changes go in `CHANGELOG.md`.
- `fs.delete`, `fs.search`, `replace_all` (D-147): `tests/agent_tools.rs`; delete kill cells: `a_delete_is_recovered_like_a_write` in `tests/recovery/main.rs`. Test baseline (D-145): `src/baseline.rs`. Refusal hints (D-146): `Policy::allowed_hint`.
- Secrets and HTTPS `ext.call` (v3, M21, D-162): `[secrets]`, `[[external.service]]` with `https://`, `ca_file`, `headers`, `approval`; code in `src/secrets.rs`, `ext::Target`. Tests: `tests/secrets.rs` (fake HTTPS service: `FakeService::start_tls`, certificates in `tests/fixtures/tls/`). Experiment: `experiments/m21-https/NOTES.md`.
- Approvals (v3, M20, D-161): `[approvals] require` in a policy; `norn approve <id> [approval] [--minutes N]`, `norn deny <id> <approval>`; code in `src/approval.rs` and `Gateway::approval_gate`. Tests: `tests/approvals.rs`. Experiment: `experiments/m20-approval-wait/NOTES.md`.
- Network allow-list (v3, M19, D-160): `[network] allow` in a policy; the proxy is `src/sandbox/proxy.rs`. Tests: `tests/network.rs`, `cargo test --lib sandbox::proxy`; kill cells with the network on: `NORN_RECOVERY_NETWORK=1` (part of `make test` and `make eval-recovery`; report `target/recovery-network-report.json`). Experiments: `experiments/m19-*/NOTES.md`.
- Binary files (base64) and the executable bit through the file tools (D-123): `tests/binary.rs`.
- A repository folder shown read-only at its place (`filesystem.ro_dirs`, D-122): `tests/rodirs.rs`.
- `fs.edit` (D-120): `tests/edit.rs`; its kill cells are the test `an_edit_is_recovered_like_a_write` in `tests/recovery/main.rs`.
- The disk limit `limits.workspace_mb` (D-117): the last four tests of `tests/hardening.rs`.
- Hostile leftovers and folder-making writes: `tests/hardening.rs` (part of `make test`; alone: `systemd-run --user --scope -p Delegate=yes cargo test --test hardening`).
- Packages from outside the repo (a mounted virtualenv, `[sandbox.env]`): `tests/deps.rs`. First messages, the patch's notes, the startup probe: `tests/hardening.rs`.
- Ubuntu without the system-wide sysctl: `packaging/apparmor/norn` (install steps in its header and in the README); CI job `fhs-apparmor`.
- Bugs the suites find in the runtime get a note in `docs/bugs.md`.
- CI: `gh run list --limit 5`, `gh run view <id> --log-failed`, `gh run download <id> -n recovery-report-fhs`.

## Current status

v0.1 tagged locally (tags `m0`–`m6`, `v0.1`). All seven milestones are done on the `nix` preset. The `fhs` preset is verified by CI run 43385661105 (2026-11-13, commit `c3c9502`, Ubuntu 24.04; D-072): lint, all test suites and the 200-kill suite pass. Two earlier runs failed and found B-007 and B-008. Results of record: `docs/results/recovery-200-fhs.md`, `security-fhs.md`. Not run on Ubuntu: benchmarks, `make demo`.

The bonus (M7, tag `m7` locally) is built on top of v0.1: the tool `ext.call`, the fake service, its recovery rule (D-073 to D-081, ADR 0012). On the `nix` preset the external cells give 1,600 runs with 0 violations and 0 duplicated effects (`docs/results/recovery-ext-200-nix.md`). On `fhs` the same, in CI run 43436681321 on `1bc27da` (D-082, `docs/results/recovery-ext-200-fhs.md`).

`main` and the tags `m0`–`m7`, `v0.1` are on GitHub (`pessimistic-zer0/agentos`). They were pushed once by the agent at Prince's direct request (D-083); the rule above is unchanged: the agent does not push.

M8 (tag `m8` locally; `docs/milestones/M8-first-user.md`) is the work after the bonus, asked for by Prince on 2026-11-13 and 2026-11-14:

- Bugs found by using the program by hand, all fixed: B-009 to B-013 (`docs/bugs.md`). `fs.write` makes missing folders (D-084).
- What a first user needs (D-088): clearer first messages and an early check for the LLM server (D-089); a patch that lists what it cannot carry (D-090, D-095); packages from outside the repo (D-091, D-092; `tests/deps.rs`); an AppArmor profile and a startup probe that tries (D-093, D-096; `packaging/apparmor/norn`).
- First trial on a project that is not the demo: `docs/results/real-repo-trial-nix.md` (the test project is in `~/Temp/Norn_temp/`, outside this repository). Ideas it raised are parked in the spec, §22.
- On `nix`: `make test` 105 passed; 200-kill run 4,800 + 1,600 runs with 0 violations (`docs/results/recovery-200-nix-m8.md`); benchmarks re-run (`docs/results/bench-nix-m8/`).
- On `fhs`: CI run 43612319559 on `69cdbf0` (D-097), both jobs green: `fhs` (105 tests, `make demo`, the 200-kill run) and `fhs-apparmor` (restriction on; refuses without the profile; runs with the profile installed at `/usr/local/bin/norn` as the README says, and four suites). Earlier M8 runs: 43573592353 (D-087), 43590153676 (D-094). Not run on Ubuntu: benchmarks.

M9 (tag `m9` locally; `docs/milestones/M9-conveniences.md`), 2026-11-15: ten of the parked ideas (D-098 to D-107). On `nix`: `make test` 118 passed; the real-project session repeated (`docs/results/real-repo-trial-nix.md`, second part); 200-kill run in `docs/results/recovery-200-nix-m9.md`. Verified on `fhs` together with M10 (D-110). Held at the time: `norn apply` (built next, M10) and binary file tools. The model-server error met in the re-trial is handled since D-111.

`norn apply` (M10, tag `m10` locally; `docs/milestones/M10-apply.md`; D-108), 2026-11-15: copies a finished execution's result into the repository, including what a patch cannot carry; refuses if the repository changed since the copy. `tests/apply.rs` (6 tests). On `nix`: `make test` 125 passed. On `fhs`: CI run 43691133260 on `d98eab1` (D-110), both jobs green: 125 tests, `make demo`, the 200-kill run, and the AppArmor job. The run before it (43690755406) failed on a false note that only Ubuntu could show (B-014, D-109). Still parked: binary files and file modes through the file tools.

After M10, 2026-11-15, no new tag yet: the model server's answers are sorted into wait, error and tell-the-model (D-111; verified on `fhs` by CI run 43733350171 on `2a6a3d1`, D-113), and helper programs for commands (`exec.helpers`, D-112), found in a trial on a Node and a Rust project (`docs/results/second-stack-trial-nix.md`; the two projects are in `~/Temp/Norn_temp/`). On `nix`: `make test` 130 passed. D-112 is verified on `fhs` by CI run 43844369387 (D-119). `main`, and the tags up to `m10`, are on GitHub, pushed by Prince.

v1.0 is done (D-114, D-136): milestones M11 to M14, tag `v1.0` locally on `d83aba5`.

M11 (tag `m11` locally, on `fc31fbb`; `docs/milestones/M11-hardening.md`), 2026-11-16: the TOCTOU test repeats its race until it happened often enough (D-115, after CI run 43742105191 failed on 925 swaps); the seccomp filter (D-116); the measured disk limit `limits.workspace_mb` (D-117); scratch paths left out of checkpoints (D-118). On `nix`: `make test` 138 passed; 200-kill run in `docs/results/recovery-200-nix-m11.md`. On `fhs`: CI runs 43844369387 on `caeb948` (D-119) and 43849008533 on `fc31fbb` (D-121), both jobs green. Commit `048e7e5` has four failing tests, fixed in `b340a51`.

M12 (tag `m12` locally, on `f38fff9`; `docs/milestones/M12-real-projects.md`), 2026-11-16: `fs.edit` (D-120), a read-only repository folder inside the workspace (`filesystem.ro_dirs`, D-122), binary files and the executable bit through the file tools (D-123). With `fs.edit` the local 4B model solved the two-module ticket it had failed three times (`docs/results/real-repo-trial-nix.md`, last part). Verified on `fhs` by CI runs 43894443267 (D-125) and 43903121563 (D-129).

M14 (tag `v1.0` locally, on `d83aba5`; `docs/milestones/M14-release.md`), done 2026-11-25: the compatibility promise with tests against frozen v0.1 files (D-126), `CHANGELOG.md`, the release archive and install instructions (D-127), Debian 12 in a virtual machine (D-128), a busy model server given the wait it asks for (D-124). All verified by CI run 43903121563 on `f38fff9` (D-129): four jobs green, `fhs` (167 tests, `make demo`, the 200-kill run), `fhs-apparmor`, `release` (archive built and run from; benchmarks, kept in `docs/results/bench-fhs/` as a record), `debian` (167 tests on Debian 12, kernel 6.1). The Ubuntu runners' kernel is 7.0. The name is **norn** (D-133, 2026-11-25; the rename, and what keeps the old name on purpose, such as the `ext.call` idempotency key and the `AGENTOS_` settings as a fallback: D-134). Version 1.0.0 (D-135). On `nix`: `make test` 168 passed. On `fhs`: CI run 45489487156 on `d83aba5` (D-136), all four jobs green: 168 tests, `make demo`, the 200-kill run, the AppArmor job, the archive `norn-1.0.0`, and 168 tests on Debian 12. The GitHub repository is `pessimistic-zer0/norn` since Prince renamed it; the local folder is still `agentos`.

M13 (tag `m13` locally, on `5e17acb`; `docs/milestones/M13-hosted-model.md`), 2026-11-17: the six inventory tickets with Claude as the model, through a relay that looks like a model server and is answered by a fresh subagent per ticket (D-130; Prince's choice after the Groq key's limit stopped the hosted run after one ticket). Six of six solved, each checked on a clone against the solution notes (`docs/results/relay-model-trial-nix.md`). The friction it found (the stop rule in a project with other failing tests, denials that do not say what is allowed) is parked in the spec §22. No code change; CI run 44093789877 on `5e17acb` is green on all four jobs (D-132). Scheduling is parked as a v2 sketch (D-131).

v2 (D-139, `docs/v2-scope.md`), done. M15 (`docs/milestones/M15-mcp-server.md`), 2026-11-26: `norn mcp` (D-140 to D-143) and `norn finish`. The experiments with Claude Code changed two plan items (D-141). Claude Code fixed the demo through it (`docs/results/mcp-claude-code-demo-nix.md`). M16 (`docs/milestones/M16-mcp-recovery.md`): the kill cells and the security suite through MCP, hostile protocol messages (D-144); no change in `src/`. M17 (`docs/milestones/M17-agent-friction.md`): the test baseline (D-145), refusals that say what is allowed (D-146), `fs.delete`, `fs.search`, `replace_all` (D-147), ending at once on HTTP 413 (D-148). On `nix`: `make test` 223 passed; delete cells at 200 per point in `docs/results/recovery-delete-200-nix.md`. M18 (`docs/milestones/M18-mcp-trial-release.md`): Claude Code as an MCP client, own tools off, solved the six inventory tickets, each checked on a clone (D-149, `docs/results/mcp-claude-code-trial-nix.md`); version 1.1.0 (D-150). Verified on `fhs` by CI run 45728656310 on `1571d09` (D-158): 224 tests, the MCP kill cells (4,800 runs, 0 violations), Debian 12, the AppArmor job, the 1.1.0 archive. Tags `m15` to `m18` and `v1.1` on `1571d09`.

Open, and only Prince can close them:

- The tags `v1.0` and `m14` are on GitHub, pushed by Prince on 2026-11-25.
- Nothing. v2 is released: CI run 45728656310 on `1571d09` is green on all four jobs, tags `m15` to `m18` and `v1.1` there (D-158). On the way, CI found a test-client bug and B-015 (`openat2` gave `EAGAIN` under concurrent renames and norn never retried), both fixed. v3 is released (D-153): M19 to M22 (`docs/milestones/M19-network.md` to `M22-v3-trial-release.md`, D-154 to D-167), version 1.2.0. Tags `m19` (CI run 45816245961, D-166), `m20` (45817518838, D-165), and `m21`, `m22`, `v1.2` on `01edb37` (45920326159, D-167), all on GitHub. On the way, CI found B-016 (a run stopped in setup could not be resumed, and said only "No such file"), fixed. v4 and v5 are ordered (D-137) but not scoped; each needs Prince's approval.

Update this section when any of these changes.
