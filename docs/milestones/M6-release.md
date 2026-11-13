# M6 — Release (v0.1)

| | |
|---|---|
| Weeks in the plan | 11–12 |
| Started / finished | 2026-11-12 / 2026-11-12 |
| Commits | `1757fed` … the commit that adds this file (tags `m6` and `v0.1`) |
| Status | Done with gaps (the `fhs` preset is not yet verified; no license chosen) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Make the project usable and checkable by someone who was not there. That means: a README a stranger can follow, one command that shows the whole story, a threat model that says what is and is not protected, a document that says exactly what "recovery" promises, short records of the decisions that are hard to undo, and a final run of every suite on the code that gets the tag. No part of the system changes, with one exception: a bug found by running the demo was fixed.

## 2. Requirements

Exit criterion from the v1 table: *"A stranger can clone and run the demo in under 10 minutes on Ubuntu or NixOS."* The row also lists: README, `threat-model.md`, `recovery-semantics.md`, ADRs, demo recording, tag v0.1.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| Clone and run the demo in under 10 minutes on NixOS | Yes, with a caveat | `git clone` into an empty folder, then `nix develop -c make demo`: **23 s**, exit 0, including a full debug build (the clone's own `target/`, 428 MB). Caveat: crate sources and the Nix shell's packages were already on this machine; a first run elsewhere also downloads them. Not measured |
| …on Ubuntu | **Not verified** | Never run. Instructions are in the README, marked untested |
| README | Yes | `README.md`: status, demo, results, how it works, usage, limits, layout |
| `docs/threat-model.md` | Yes | 32 attacks, each with defence and test; out-of-scope list; known weaknesses |
| `docs/recovery-semantics.md` | Yes | What each crash point leaves, the verdict per tool, the invariants and how they are checked, what is not promised |
| ADRs (at least 3) | Yes | 11 in `docs/decisions/` |
| Demo recording | Yes, as text | `docs/demo-transcript.md`: the unedited output of `make demo`. Not a video (D-068) |
| Tag v0.1 | Yes | `git tag` lists `m0`…`m6`, `v0.1`. Local only; nothing was pushed |
| Final suites on the tagged code (`nix` preset) | Yes | Fault injection: 4,800 runs, 4,797 killed, 0 violations, 0 duplicated effects, 3,918/3,918 verdicts (`docs/results/recovery-200-nix.md`). Security: 32 passed; benign: 0 false denials (`docs/results/security-nix.md`). Benchmarks: `docs/results/bench-nix/`. `make test` and `make lint` pass on the final commit |

**Definition of done** (v1 document), box by box:

| Box | State |
|---|---|
| P1–P8 fault injection, 0 invariant violations | Met on `nix` |
| Security suite 30+, 0 escapes, 0 false denials | Met on `nix` (32 cases, 23 benign) |
| Both suites pass with `fhs` **and** `nix` | **Open.** `fhs` has never been run |
| Three benchmarks published with scripts, raw data, hardware | Met |
| Demo from a fresh clone in under 10 minutes | Met on NixOS (23 s, dependencies cached); Ubuntu not run |
| Threat model, recovery semantics, at least 3 ADRs | Met |

One box is open. By the v1 document's own rule, AgentOS should not go on the resume until the Ubuntu CI run is green.

One detail about the final fault-injection run: it was made before the execution-id change of this milestone (D-067). That change touches only how ids are formed; `make test`, which includes 72 kills, passed after it. The 4,800-run figure is therefore for the code one small commit earlier, and says so in the results file.

## 3. What you need to know first

**Release tag.** A name (`v0.1`) attached to one commit, saying "this exact state is the release". Tags here are local until pushed.

**Threat model.** A written answer to: what are we protecting, from whom, what can they do, and what do we not defend against. Its most useful part is usually the "not" list.

**Semantics.** The exact meaning of a promise. "It recovers" is a slogan; "after a kill at point X, the log says Y and the workspace is Z" is semantics.

**ADR (Architecture Decision Record).** One page per decision that would be expensive to reverse: the question, the answer, why, the cost, and what would make you reconsider.

**Fresh clone test.** Building and running from a new copy of the repository, to catch anything that only works because of files lying around in the developer's folder.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| What the sandbox reveals | Printed `/proc/self/mountinfo`, `uname`, `/proc/cpuinfo`, `/proc/self/cgroup` from inside | The workspace's host path (with the user name), host name, kernel version, CPU model and cgroup path are visible | Written into the threat model as a known information leak |
| First full demo run | `make demo` end to end | Steps 5–7 acted on the wrong execution | Bug B-006; ids now carry milliseconds |
| Fresh clone | Clone to an empty folder, `nix develop -c make demo` | 23 s, exit 0 | Nothing depended on the working folder; the number excludes downloads |

## 5. What was built

| File | Purpose |
|---|---|
| `README.md` | The front page |
| `demo.sh`, `Makefile` (`demo`) | One command for the demo |
| `docs/threat-model.md` | Threat model |
| `docs/recovery-semantics.md` | Recovery semantics |
| `docs/decisions/0001…0011`, `README.md` | ADRs and their index |
| `docs/demo-transcript.md` | The demo's output |
| `docs/results/recovery-200-nix.*` | Final fault-injection run (the M3 run is kept as `recovery-200-nix-m3.*`) |
| `src/workspace.rs` | Execution ids with milliseconds |
| `docs/bugs.md` | B-006 |
| `Cargo.toml` | Version 0.1.0 |
| `AgentOS v1 — Scoped Design.md` | Definition-of-done boxes marked with their state |
| `CLAUDE.md` | Status: what is done, what only Prince can close |

## 6. The code, explained

### 6.1 `demo.sh`

```sh
if [ -z "${AGENTOS_IN_SCOPE:-}" ]; then
  exec env AGENTOS_IN_SCOPE=1 systemd-run --user --scope -p Delegate=yes -q "$0" "$@"
fi
```

The sandbox needs a delegated cgroup. If the script is not yet inside one, it starts itself again under `systemd-run`. `exec` replaces the current shell with that command, so there is no leftover parent process; the environment variable stops it from doing this twice.

```sh
elif [ -d /nix/store ] && [ ! -e /usr/bin/python3 ]; then preset=nix
else preset=fhs; fi
```

The only place that guesses the distribution. It is in the demo script, outside `src/`, which must not know about distros (CLAUDE.md, "Portability").

```sh
AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=6 $A run ... || echo "   -> exit code $? (137 = killed by signal 9)"
id2=$(ls "$state/executions" | grep -v "^$id1\$")
```

The second run is told to kill itself at P4 (after the effect, before the checkpoint) while handling call 6, the `fs.write`. `|| echo` keeps `set -e` from stopping the script when the killed process returns 137. The second execution is found as "the one that is not the first", not by sorting.

### 6.2 Execution ids (`workspace::new_execution_id`)

```rust
let ts = crate::log::now_rfc3339(); // 2026-11-11T19:02:03.456Z
let digits: String = ts.chars().filter(|c| c.is_ascii_digit()).collect(); // 20261111190203456
...
format!("{}-{}-{}", &digits[..8], &digits[8..17], &hash[7..13])
```

The id is date, time to the millisecond, and six characters from a hash of the clock's nanoseconds and the process id. Text sorting of such ids is time sorting, as long as two runs do not start in the same millisecond. Before this milestone the time part stopped at seconds, which is how the demo found bug B-006: two runs in one second were ordered by the random part.

The test now checks the property, not only the shape:

```rust
let a = new_execution_id();
std::thread::sleep(std::time::Duration::from_millis(3));
let b = new_execution_id();
assert!(a < b, "{a} should sort before {b}");
```

### 6.3 The documents

Not code, but they follow rules worth knowing:

- **Every number is copied from `docs/results/`.** The README's results table, the recovery document's table and the ADR costs all point at files a reader can open.
- **Every claim in the threat model names a test.** A row without a test would be an opinion.
- **Limits are written next to claims**, not in a separate file nobody opens: "Status" at the top of the README, "What it does not do" before the repository layout, §7 of the recovery document, §4 and §5 of the threat model.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-066 | `make demo`: one script for fix, crash, recover, resume | Commands listed in the README; a `demo` subcommand |
| D-067 | Execution ids carry milliseconds (replaces the format in D-036) | A counter file; ordering by first event |
| D-068 | Tag v0.1 locally with the open box stated; license and name left to Prince; demo recorded as text | Not tagging; picking a license; a video |

## 8. Problems hit

1. **B-006: the demo recovered the wrong run.** Expected step 5 to print a recovery verdict. It printed "already finished; nothing to recover", because "latest execution" had picked the first run: both were started in the same second and ids had one-second resolution. Found by reading the demo's output, not by a test. Fixed in the id format and in the script (commit `1757fed`). Nothing was damaged; recovery refuses to touch a finished execution.
2. **A result that looked too good.** The fresh-clone demo took 23 s including a full build. Checked that the clone had built its own `target/` (it had) and worked out why: 12 cores, crate sources already downloaded. Reported with that caveat instead of as "a stranger needs 23 seconds".
3. **A garbled sentence in the threat model** (about `python3 -m pytest`) was caught on re-reading and rewritten before commit.
4. **Results files held local paths** (found in M5, finished here): the LLM demo trace and one experiment note showed the home folder; replaced with `<repo>` and `/home/<user>`. Earlier commits still contain them; history is not rewritten.

## 9. How to run it

```sh
git clone <repository> && cd agentos
nix develop -c make demo          # the story in one command
nix develop -c make test          # all suites, quick
nix develop -c make eval          # all suites at full size, about 13 minutes
```

Read next, in this order: `README.md`, `docs/threat-model.md`, `docs/recovery-semantics.md`, then the milestone documents M1 to M5 for the code.

## 10. What this milestone does not do

- **Nothing was pushed.** No remote has this code. CI has never run.
- **`fhs` / Ubuntu is unverified**, and the CI workflow itself is untested and may fail for reasons unrelated to AgentOS (the systemd user session on the runner).
- **No license, and the working name "AgentOS" stands** (D-023, D-068).
- **No video or asciinema recording.**
- **The "Bonus"** (a fake external tool with idempotency keys, to exercise an effect that cannot be rolled back) has not been started.
- **Open technical items carried forward**, all documented where they belong: no seccomp filter; no disk limit; checkpoints never pruned; no power-loss testing; host facts visible in the sandbox; LLM agent tried with one small model only.

## 11. Check your understanding

1. What are the three suites, and which single command runs them?
2. The definition of done has six boxes. Which one is open, why can it not be closed from this machine, and what follows for the resume?
3. Why does the README have a "Status" section before the demo instead of after the results?
4. Pick any row of the threat-model table and explain the attack, the defence and what the test does.
5. Name three things the threat model puts out of scope, and for one of them say what it would take to bring it in.
6. From `docs/recovery-semantics.md`: the runtime is killed at P6 during an `fs.write`. What is on disk, what does recovery do first, and what verdict does it reach?
7. Why is "at-least-once with rollback" the honest description, and not "exactly-once"?
8. How did two runs started in the same second break the demo, and why did the old unit test not catch it?
9. The fresh-clone demo took 23 seconds. What does that number include, and what does it leave out?
10. Why was a license not chosen by the agent?
11. An interviewer asks: "How do you know your sandbox is secure?" Give the answer this project can honestly support.
12. If you could add one thing to v0.2, what would you choose from the "does not do" lists, and which existing test or document would you extend first?

## Notes added after the milestone

The text above is left as it was written at tag `v0.1`. Later changes are listed here.

- **2026-11-12, license.** Prince chose dual `MIT OR Apache-2.0` (D-069). `LICENSE-MIT`, `LICENSE-APACHE`, the `license` field in `Cargo.toml` and a README section were added after the tag. "No license chosen" above describes the state at `v0.1`, not the current state.
- **2026-11-13, `fhs` preset.** Every "not yet verified" about the `fhs` preset above describes the state when this document was written. Prince pushed on 2026-11-12. The first two CI runs failed the 200-kill suite and found bugs B-007 and B-008 (`docs/bugs.md`, D-070, D-071). The third, <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> on commit `c3c9502` (Ubuntu 24.04), is green: lint, all test suites (32 security cases, the benign suite) and 4,800 kills with 0 violations. Results of record: `docs/results/recovery-200-fhs.md`, `docs/results/security-fhs.md`. See D-072. Still not run on Ubuntu: the benchmarks and `make demo`.
- **2026-11-13, definition of done.** With the `fhs` box ticked, all six boxes of the v1 document are ticked. The CI workflow did not fail "for reasons unrelated to AgentOS" as §10 feared; it worked unchanged, and the two failures were AgentOS bugs.
