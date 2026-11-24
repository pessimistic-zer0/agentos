# M14 — The 1.0 release

| | |
|---|---|
| Weeks in the plan | v1.0 (D-114); no week plan |
| Started / finished | 2026-11-16 / 2026-11-25 |
| Commits | `3036816` … `d83aba5` (tag `v1.0` on `d83aba5`) |
| Status | Done. Everything below verified on both presets; the last part (the name norn, version 1.0.0) by CI run 45489487156 on `d83aba5` (D-136) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it. Fill it in while working.

## 1. Aim

Turn the code of M11 and M12 into something that can be called 1.0: say what stays compatible and test it, write down what changed for a user, build and measure the release the way a user would get it, and tag it once CI is green and the name is settled. No part of the runtime changes by plan.

## 2. Requirements

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| D10: a stated compatibility promise for the policy format and the event log | Yes, both presets (`fhs`: CI run 43903121563, D-129) | `docs/compatibility.md`; `tests/compat.rs` (4 tests) against files written by the `v0.1` binary |
| D9: a CHANGELOG | Yes | `CHANGELOG.md` |
| D9: a release build in CI | Yes (CI run 43903121563, job `release`, D-129) | `packaging/release.sh --check` on `nix`: packs a 3.7 MB archive and runs the demo task from it; CI job `release` (D-127) |
| D9: benchmarks run on Ubuntu | Yes, as a record (`docs/results/bench-fhs/`, D-129) | the same job runs `make bench` and uploads `docs/results/bench-fhs` |
| D9: install instructions | Yes | README, "Installing it" |
| D9: Cargo version 1.0.0 | Yes, both presets (CI run 45489487156, D-136) | `Cargo.toml` `version = "1.0.0"`; `norn --version` prints `norn 1.0.0` |
| D11: one more distribution in CI, or a written reason why not | Yes: Debian 12 in CI (run 43903121563, job `debian`, 167 passed, D-129) | `ci/debian-vm.sh`: Debian 12, kernel 6.1, in a VM on the development machine: `make test` 167 passed (`docs/results/debian-12.md`); CI job `debian` (D-128) |
| D8: the project name | Yes: norn, Prince's choice (D-133); verified on `fhs` by CI run 45489487156 (D-136) | Renamed in code, scripts, CI and current documents (D-134). `make test` 168 passed on `nix`; `make demo` and `packaging/release.sh --check` pass |
| Tag `v1.0` after a green CI run on that commit | Yes | CI run 45489487156 on `d83aba5`: `fhs`, `fhs-apparmor`, `release`, `debian` all success; `v1.0` tagged on `d83aba5` (D-136) |

## 3. What you need to know first

**Format version.** A number a file carries to say which rules it follows. The policy file has `policy_version = 1`; the event log stores `schema_version = 1`. A program that meets another number refuses the file instead of guessing.

**Backward compatible.** A newer program works with what an older one wrote. That is the direction users need: they upgrade the program, not their old files.

**Frozen fixture.** A file kept with the tests that was produced once, by an old version, and is never produced again. If today's code made it, the test would only show that today's code agrees with itself.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `ci/debian-vm.sh`, run locally | The whole suite on Debian 12 in a VM, before putting it in CI | 167 passed on the first attempt; Debian needed no extra system settings | The sandbox does not depend on Ubuntu or on a recent kernel: 6.1 is enough |
| (by hand; its output became the fixtures) | Built the tag `v0.1`, made a finished and a killed execution with it, then used today's binary on them | Policy accepted; `status`, `trace`, `apply`, `recover`, `run --resume` all worked | Every setting and log field added since v0.1 was optional, so the promise could be made for what already exists, and v0.1's files could serve as the test |

## 5. What was built

| File | Purpose |
|---|---|
| `docs/compatibility.md` | The promise, and what is not promised |
| `tests/compat.rs`, `tests/fixtures/compat/` | Today's code against two v0.1 presets and two v0.1 execution folders (in a tar file, so the trees are exact) |
| `CHANGELOG.md` | What changed for a user since 0.1 |
| `ci/debian-vm.sh` | Boots Debian from its cloud image under QEMU/KVM, installs the prerequisites, runs `make test` as an ordinary user |
| `packaging/release.sh` | Builds the release, packs the archive; `--check` runs the demo task from the unpacked archive |
| `.github/workflows/ci.yml` | Job `release`: the script above, then the benchmarks on Ubuntu; both uploaded |

## 6. The code, explained

**`tests/compat.rs`.** `unpack` extracts `v0.1-executions.tar` into a fresh state folder. The folders in it are called `finished` and `killed-p4` so the test does not depend on execution ids. Three tests use them through the real command line (`status`, `trace`, `apply --to`, `rm`, `recover`, `run --resume`); the policy test uses the library (`Policy::parse`) and checks both that the file is accepted and that it still decides the same five requests the same way. The last test changes the version number in a log and in a policy and requires a refusal that names the version.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-136 | Tag `v1.0` on `d83aba5`, the commit CI passed | Tagging the later commit that records the run |
| D-135 | Version 1.0.0 set in its own commit, right after the rename; the tag waits for CI | Tagging before CI; waiting for CI on the rename before setting the version |
| D-134 | The rename keeps the `ext.call` key `agentos-…`, reads `AGENTOS_` settings as a fallback, and leaves history and frozen fixtures as written | Renaming the key; new setting names only; looking in the old state folder too |
| D-133 | The name norn | enma, possum, sanzu, skuld, veles, keeping AgentOS |
| D-128 | Debian 12 in a VM, by a script shared by the development machine and CI | A container; root in a privileged container; Fedora too |
| D-127 | One archive made by a script, checked by running from it, in its own CI job; CI benchmarks are a record only | Publishing from CI; a static build; distribution packages |
| D-126 | Backward compatibility for format version 1 of policy files and execution folders, tested against frozen v0.1 files | Promising the other direction; promising screen output or the library |

## 8. Problems hit

1. A compat test failed although the product was right: a shared test helper finds an execution's folder by the id in its log, and the fixture's folders have fixed names. The test reads the output by folder name.
2. The mechanical rename also renamed the idempotency key the tests expect; three tests in `tests/external.rs` failed until the tests were set back to `agentos-…`. The key is a format, not a word (D-134).

## 9. How to run it

```sh
systemd-run --user --scope -p Delegate=yes cargo test --test compat
```

## 10. What this milestone does not do

- Nothing is published: no crates.io package (`publish = false`; the name `norn` is taken there by another crate, D-133), no GitHub release page, no distribution packages (D-127). The archive is built and checked in CI and kept as a CI artifact.
- The compatibility promise covers files, not the screen, the Rust library or an older program reading newer files (`docs/compatibility.md`).
- The name was checked quickly (crates.io and a web search), not as a trademark. Another AI coding agent called Norn exists (D-133).
- Three systems are tested: NixOS, Ubuntu 24.04 and Debian 12. Fedora and Arch use the same `fhs` preset and have not been run.
- The CI benchmarks are a record of one shared machine, not the project's numbers (D-127).

## 11. Check your understanding

1. Why does a compatibility test use files written by the old v0.1 binary, and not files made by today's code?
2. Which direction of compatibility does 1.0 promise, and why not the other one?
3. The rename changed `agentos` to `norn` everywhere except the `ext.call` idempotency key. What would go wrong if the key had changed too?
4. How did the tests catch the key being renamed?
5. Why are old `AGENTOS_` settings still read, and which name wins when both are set?
6. Why was the tag put on `d83aba5`, the commit CI ran, and not on the later commit that records the run?
7. What does running the demo from the unpacked archive prove that `make test` does not?

