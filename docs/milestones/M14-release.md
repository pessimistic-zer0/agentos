# M14 — The 1.0 release

| | |
|---|---|
| Weeks in the plan | v1.0 (D-114); no week plan |
| Started / finished | 2026-11-16 / in progress |
| Commits | `3036816` … |
| Status | In progress. Done and verified in CI: the compatibility promise, the CHANGELOG, the release archive, benchmarks on Ubuntu, install instructions, Debian 12. Open: the name (Prince), M13 (blocked on an API key), then version 1.0.0 and the tag |

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
| D9: Cargo version 1.0.0 | Not started (last step) | |
| D11: one more distribution in CI, or a written reason why not | Yes: Debian 12 in CI (run 43903121563, job `debian`, 167 passed, D-129) | `ci/debian-vm.sh`: Debian 12, kernel 6.1, in a VM on the development machine: `make test` 167 passed (`docs/results/debian-12.md`); CI job `debian` (D-128) |
| D8: the project name | Open; only Prince decides | |
| Tag `v1.0` after a green CI run on that commit | Not yet | |

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
| D-128 | Debian 12 in a VM, by a script shared by the development machine and CI | A container; root in a privileged container; Fedora too |
| D-127 | One archive made by a script, checked by running from it, in its own CI job; CI benchmarks are a record only | Publishing from CI; a static build; distribution packages |
| D-126 | Backward compatibility for format version 1 of policy files and execution folders, tested against frozen v0.1 files | Promising the other direction; promising screen output or the library |

## 8. Problems hit

1. A compat test failed although the product was right: a shared test helper finds an execution's folder by the id in its log, and the fixture's folders have fixed names. The test reads the output by folder name.

## 9. How to run it

```sh
systemd-run --user --scope -p Delegate=yes cargo test --test compat
```

## 10. What this milestone does not do

(Filled in at the end.)

## 11. Check your understanding

(Filled in when the milestone is finished.)
