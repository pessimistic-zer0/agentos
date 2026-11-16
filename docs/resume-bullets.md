# Resume bullets, with where each number comes from

Rule of the project: no number without a script or a record that produces it. Each bullet names its source. Counts were taken on 2026-11-15 at commit `58d2028`; re-count before using them.

## Short form (three lines)

- Built **AgentOS**, a Rust runtime that runs an AI coding agent's tool calls in a rootless Linux sandbox (user, mount, PID and network namespaces, cgroup v2, Landlock, `openat2`) behind a default-deny policy, with a write-ahead event log and checkpoints.
- Survives `kill -9` at any point: **4,800 injected kills at 8 crash points, 0 invariant violations, 0 duplicated effects**, on NixOS and on Ubuntu 24.04 in CI; **32 adversarial tests, 0 sandbox escapes**.
- Used it on Python, Node and Rust projects; found and fixed **14 bugs**, 6 of them after every test suite had passed, and wrote up why the suites missed them.

## Longer form (pick what fits the role)

| Bullet | Source |
|---|---|
| Designed and built a 7,000-line Rust runtime (one crate, no async runtime, 11 dependencies) that mediates every file and command request of an AI coding agent through a gateway: policy check, fsynced intent record, effect, checkpoint, completion record | `cat src/*.rs src/*/*.rs \| wc -l` = 7,047; `Cargo.toml`; `src/gateway.rs` |
| Rootless sandbox from Linux primitives: user, mount, PID and network namespaces, `pivot_root` to a root that holds only a copy of the repo, cgroup v2 limits, Landlock, no capabilities, `no_new_privs`; file tools confined by `openat2(RESOLVE_BENEATH)`, immune to symlink races | `src/sandbox/`; `docs/threat-model.md` §3 |
| Fault-injection harness that kills the runtime at 8 points in a call's life for each tool, 200 times each, then checks 5 invariants: 4,800 runs, 0 violations, 0 effects applied twice; recovery p50 about 20 ms | `make eval-recovery`; `docs/results/recovery-200-nix-m9.md`; Ubuntu: CI run 43691133260 |
| Adversarial security suite: 32 attacks (path traversal, a symlink flipped about 6,000 times during 300 file calls, fork bomb, memory exhaustion, network, privilege gain, escaping through the revert code), 0 escapes; 0 false denials on 23 ordinary operations | `make eval-security`; `docs/results/security-nix.md`, `security-fhs.md` |
| Recovery for an effect that cannot be rolled back (a call to an outside service): idempotency key written before the request, uncertain calls settled by sending the same request again; 1,600 runs with kills and lost answers, 0 duplicated or lost effects; two seeded bugs produced 48 and 70 duplicates, showing that the harness can fail | `docs/results/recovery-ext-200-nix-m9.json`, `recovery-ext-mutations.md`; ADR 0012 |
| Measured overhead: 4.6 ms added per sandboxed command (p50), 13.3 ms for a durable 4 KB write with log and checkpoint, +36% end to end on the demo task | `make bench`; `docs/results/bench-nix-m8/summary.md` |
| Portable across distributions without code changes: distro differences live in policy presets; verified on NixOS and on Ubuntu 24.04 in CI, including a job that keeps Ubuntu's user-namespace restriction on and uses a shipped AppArmor profile | `policies/`; `.github/workflows/ci.yml`; D-094, D-110 |
| Ran it on real-shaped projects in three ecosystems (Python with PyPI packages in a virtualenv, Node with npm packages, Rust): tickets fixed through the gateway, results applied to fresh clones, the projects' own tests passing there | `docs/results/real-repo-trial-nix.md`, `second-stack-trial-nix.md` |
| Found 14 bugs in my own runtime: 3 only on the Ubuntu CI runs, and 6 after all suites had passed (by hands-on use, by a project someone else wrote, by the benchmarks, by CI); documented each with cause, fix and lesson | `docs/bugs.md` (B-001 to B-014); `docs/what-the-suites-missed.md` |
| Kept a decision log of 113 entries (options considered, what was discarded and why, evidence actually run), 12 ADRs and a document per milestone | `grep -c '^### D-' docs/decision-log.md`; `docs/decisions/`; `docs/milestones/` |

## What not to claim

- Not "production-ready" or "secure": a seccomp deny list rather than an allow list, a measured disk limit rather than a hard cap, kernel bugs out of scope (`docs/threat-model.md` §4).
- Not "exactly-once": a cut-off command is rolled back and may be issued again; an external call is applied once only if the service honours the key.
- Not "power-loss safe": the tests kill the process, they do not cut power.
- Not "works with LLMs" as a general claim: one local 4B model, one-line tickets (`docs/results/`).
- Not other distributions: two were tested.

## Interview stories, one line each

- Why an intent is fsynced *before* the effect, and what recovery does for each tool when the completion record is missing (`docs/recovery-semantics.md`).
- B-007 and B-008: a fix for a child process outliving its killed parent introduced a lock that outlived the parent for a few milliseconds; found only on a slower CI machine.
- B-010: a `chmod 000` folder made recovery fail for ever; the security suite and the recovery suite each assumed the other's world.
- B-013: a library function that re-ran "the program I am".
- Why the argv allowlist is not the security boundary, and the sandbox is.
