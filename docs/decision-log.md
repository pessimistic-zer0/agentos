# AgentOS decision log

This file records every decision made on this project: what was chosen, what was thrown away, and why. It is written so that someone who was not there (including future Prince, or an AI helping him study) can follow the reasoning without the original conversation.

## How to use this file

- **To learn:** pick an entry, read "In plain words", then ask an AI to explain the terms under "Concepts to learn" and to quiz you on why the discarded options lost.
- **To check a claim:** entries that say "Evidence" describe something that was actually run or read. Entries without it are reasoning only.
- **To change a decision:** do not edit the old entry. Add a new entry that says which one it replaces, and set the old entry's status to `Replaced by D-0xx`.

## Rules for writing entries

- Append only. Newest entries go at the bottom of the "Entries" section.
- One decision per entry. A discarded idea is also a decision and gets recorded.
- Every entry has all the fields in the template, even if a field says "none".
- Write the plain-words part so that it needs no other entry to make sense.
- Record bugs, failed experiments and surprises in the "Findings" section. Those are the best interview stories.
- New terms go in the glossary at the bottom.

## Entry template

```markdown
### D-0xx — Short title
- **Date:** YYYY-MM-DD
- **Status:** Taken | Discarded | Open | Replaced by D-0xx
- **Area:** scope | sandbox | durability | policy | tooling | process
- **ADR:** number in `docs/decisions/`, or none

**In plain words.** Two to four sentences a beginner can follow.

**The problem.** What forced a choice.

**Options looked at.**
| Option | Result | Why |
|---|---|---|

**What this means for the code.** Concrete consequences.

**Evidence.** What was run or read, with the result. Or "none; reasoning only".

**Concepts to learn.** Terms to look up or ask about.
```

---

## Entries

### D-001 — Why this project exists
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** scope
- **ADR:** none

**In plain words.** AgentOS is a learning and resume project. It replaces two older projects on the resume: Drishti (too similar in theme to another project, Zinthos) and later PromptShield (unfinished, and reads as a blockchain project at a glance). It is not a startup and has no users to support, though other people using it would be welcome.

**The problem.** Without a stated purpose, every feature looks worth building.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Learning and resume project | Taken | Matches what Prince wants from it |
| Product for users / startup | Discarded | Not the goal; would justify far more scope |

**What this means for the code.** Anything that does not teach something or prove a claim is cut. Prince should be able to explain every part in an interview, so the code is written in small pieces he understands.

**Evidence.** None; stated by Prince.

**Concepts to learn.** None.

### D-002 — One plan document, fixed scope
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** There are two documents. The big spec describes six phases and many ideas. The v1 document describes one small thing to build in about twelve weeks. The v1 document is the plan; the big spec is a "maybe someday" list. The outline, build order and cut list in the v1 document are fixed.

**The problem.** The big spec has six phases. PromptShield was dropped partly because it was unfinished, and a six-phase plan invites the same ending.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| v1 document is the only plan; spec is a someday list | Taken | Gives a finish line |
| Treat the whole spec as the roadmap | Discarded | Phases 2–6 are years of work for one person |
| Delete the big spec | Discarded | It holds useful ideas and the reasoning behind v1 |

**What this means for the code.** New ideas go into the spec's parking lot (§22), not into the code. Changing a v1 decision needs a new entry here with a reason.

**Evidence.** None; reasoning only.

**Concepts to learn.** Vertical slice; scope creep.

### D-003 — Rust as the language
- **Date:** 2026-11-10
- **Status:** Replaced by D-024 (same choice, reasons now written down)
- **Area:** tooling
- **ADR:** none

**In plain words.** The runtime is written in Rust. This was marked "Decided" in the original spec before this log started.

**The problem.** The project makes many low-level Linux calls and must not crash or corrupt its own state.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Rust | Taken | Chosen in the original spec; the reason was not written down there |
| C++ | Discarded | Listed in the spec as "more familiar"; no further reason recorded |
| Go | Discarded | Listed in the spec as "what many container tools use"; no further reason recorded |

**What this means for the code.** One crate. Week 0 is spent learning Rust. Prince should write the missing reason here in his own words once he has one.

**Evidence.** None; carried over from the spec.

**Concepts to learn.** Ownership and borrowing; `Result` and error handling; crates.

### D-004 — Blocking code, no async
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** The program does one thing at a time and waits for it to finish. It does not use Rust's async system. A separate watchdog thread kills commands that run too long.

**The problem.** Async Rust is harder to learn and to debug.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Synchronous code plus a watchdog thread | Taken | One agent makes one call at a time, so nothing needs to run in parallel |
| Tokio (async) | Discarded | Adds learning cost and gives nothing with a single agent |

**What this means for the code.** No `async fn`, no Tokio. HTTP to the LLM uses a blocking client (`ureq`).

**Evidence.** None; reasoning only.

**Concepts to learn.** Threads; blocking versus async I/O.

### D-005 — SQLite for the event log
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** The diary of everything the runtime does is stored in a SQLite database file. SQLite already knows how to write data so that a crash does not corrupt it.

**The problem.** The log must survive the program being killed at any instant, with no half-written records.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| SQLite in WAL mode, `synchronous=FULL` for intents | Taken | Crash safety is already solved and well documented |
| A custom append-only log file | Discarded | Good learning, but a second hard problem next to the sandbox; a tool call takes far longer than a database write, so speed is not a reason |
| PostgreSQL | Discarded | Needs a server; only makes sense with remote workers (not in v1) |

**What this means for the code.** `rusqlite` with the bundled SQLite. The runtime only inserts rows; it never updates or deletes events.

**Evidence.** None yet. The week 6–8 experiment (insert rows, kill mid-transaction, reopen) will give some.

**Concepts to learn.** Write-ahead log (WAL); `fsync`; transactions; append-only.

### D-006 — Write the intent before doing the thing
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** Before the runtime does anything that changes something, it writes "I am about to do X" to the diary and makes sure that line is safely on disk. Only then does it do X. Afterwards it writes "X is done". If the program dies in between, the diary shows a started-but-not-finished step, and recovery knows to look at it.

**The problem.** After a crash, a program that only logs finished work cannot tell "never started" from "started and maybe half done".

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Intent first, then effect, then completion record | Taken | The only order where no change can happen without a trace |
| Log only after the effect | Discarded | A crash after the effect leaves a change with no record |
| Claim exactly-once execution | Discarded | Not possible in general; an honest "uncertain" state is used instead |

**What this means for the code.** Each tool call has a state machine: REQUESTED → INTENT_LOGGED → STARTED → COMPLETED or FAILED. A call with an intent and no ending is UNCERTAIN after a crash and is never re-run until it is reconciled.

**Evidence.** None; this is the standard write-ahead idea.

**Concepts to learn.** Write-ahead logging; idempotency; at-least-once versus exactly-once.

### D-007 — Checkpoints are folder copies, not git commits
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** durability
- **ADR:** 0007

**In plain words.** A save point is a full copy of the working folder. The first plan was to use git commits as save points. A test showed git does not save everything, so restoring from it can leave the folder different from how it was.

**The problem.** Recovery promises that after a rollback the folder is exactly as it was at the save point.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Full directory copy, reflink where the filesystem supports it | Taken | Captures everything; nearly free on btrfs and XFS; a plain copy on ext4 is fine for a small repo |
| Git commits in a separate `GIT_DIR` | Discarded | Misses ignored files, empty directories, most file modes, special files, and refuses to store a nested `.git` |
| overlayfs (changes kept in a separate layer) | Discarded | Elegant, but rootless overlay has extra pitfalls; more to learn for no v1 benefit |
| btrfs or ZFS snapshots | Discarded | Only works on those filesystems; the project must not depend on one |
| Own content-addressed file tree | Discarded | Most code of all the options |

**What this means for the code.** The copy is written under a temporary name and then renamed, so a half-made copy is never used. Start by calling `cp -a --reflink=auto`. The patch for the human is a `diff` between the first save point and the final folder.

**Evidence.** Run on 2026-11-10 (git, btrfs, NixOS). A folder was snapshotted with a separate `GIT_DIR`, changed, then restored with `git reset --hard` and `git clean -fd`. Only the two non-ignored files had been captured. After restore: a commit made in the folder's own `.git` was still there (so that inner repo no longer matched its files); a changed file in an ignored directory stayed changed; an empty directory was gone; a file's mode had gone from `600` to `644`.

**Concepts to learn.** Reflink (copy-on-write copy); atomic rename; `.gitignore`; `GIT_DIR` and `GIT_WORK_TREE`; overlayfs.

### D-008 — Denied files are never copied into the workspace
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** policy
- **ADR:** 0008

**In plain words.** The rules file can say "the agent must not read `.env`". The first design only checked this when the agent asked the gate to read a file. But the agent can also run tests, and test code can open any file in the folder. So the fix is to never put denied files in the folder at all.

**The problem.** Running `pytest` runs the repo's own code (`conftest.py`), and that code sees the whole folder. A rule that only the gate checks is not a real rule.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Leave denied files out when the workspace copy is made | Taken | A file that is not there cannot be read by anything |
| Check deny rules only in the `fs.*` tools | Discarded | Code run by `exec` walks around it |
| Use Landlock to deny the paths | Discarded | Landlock only grants access to folders; it cannot carve a denied file out of an allowed folder |
| Hide denied files by mounting something over them | Discarded | Works, but one mount per denied path, and files created later are not covered |

**What this means for the code.** `workspace.rs` filters by deny globs while copying. The `fs.*` tools still check deny globs, in case a denied path is created later. The security suite has a `conftest.py` that tries to read a denied file.

**Evidence.** Read from the v1 document as it was: deny globs applied to `fs.*` calls only, and Landlock gave `exec` read-write on all of `/work`.

**Concepts to learn.** Landlock; glob patterns; defence in depth; arbitrary code execution.

### D-009 — Write rules for `exec` are checked afterwards
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** policy
- **ADR:** 0009

**In plain words.** The rules file can say "only write inside `src/` and `tests/`". Code run by a command can write anywhere in the folder. After each command, the runtime compares the folder with the last save point, undoes any change outside the allowed places, and records it.

**The problem.** Same as D-008, but for writing.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Compare with the checkpoint after each `exec`; revert and log outside changes | Taken | Works for any glob; uses the checkpoint that already exists |
| Mount the non-writable parts read-only | Discarded | Globs do not map cleanly onto mounts, and a tool that needs a scratch file elsewhere would fail in confusing ways |
| Apply write rules to `fs.write` only and say so | Discarded | Honest but weak; the fix above is cheap |

**What this means for the code.** A tree comparison after every `exec`. The call still counts as completed; the reverted paths are logged as a policy violation event.

**Evidence.** None; reasoning only.

**Concepts to learn.** Bind mounts; read-only mounts.

### D-010 — Workspace on disk, no hard size limit
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** sandbox
- **ADR:** 0010

**In plain words.** The working folder lives on the normal disk. The first design put it on an in-memory filesystem with a size limit, to stop the agent filling the disk. But an in-memory filesystem created inside the cage setup disappears when the program is killed, and the whole point is to recover after being killed.

**The problem.** Without root, the only easy way to cap a folder's size is a tmpfs, and a tmpfs mounted in a private mount namespace is gone once the last process in that namespace dies.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Workspace on disk; disk fill listed as a known limit | Taken | Recovery keeps working, including the `fs.write` hash check |
| Size-capped tmpfs | Discarded | Lost on `kill -9`; every recovery would become "restore the last checkpoint", and the hash check would never run |
| Filesystem quotas | Discarded | Need root |

**What this means for the code.** No disk cap in v1. The README's limits section says so.

**Evidence.** None; this follows from how mount namespaces work, and was not run.

**Concepts to learn.** tmpfs; mount namespace lifetime; disk quotas.

### D-011 — A call is "done" only after its save point exists
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** The diary and the save points are stored in two different places, so they cannot be updated in one step. The order is: do the thing, make the save point, then write "done" in the diary with the save point's name.

**The problem.** If "done" were written first and the program died before the save point, a later rollback would go to an older save point and silently erase a step the diary calls done.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Effect → checkpoint → COMPLETED record naming the checkpoint | Taken | A completed call always has its checkpoint |
| COMPLETED record first, checkpoint after | Discarded | Rollback can lose a completed effect |

**What this means for the code.** "Last checkpoint" means the last one named in the log. Recovery deletes checkpoints the log does not name. The crash-point list grew from seven to eight (before, during and after the checkpoint).

**Evidence.** None; reasoning only. The fault-injection harness will test it.

**Concepts to learn.** Atomicity across two stores; crash consistency; invariants.

### D-012 — System folders come from policy presets
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** sandbox
- **ADR:** 0011

**In plain words.** The cage needs to see the folders where programs like Python live. Those folders differ between Linux systems. So the list is in the rules file, with two ready-made settings: `fhs` for normal Linux (Ubuntu, Fedora, Arch) and `nix` for NixOS.

**The problem.** The first design hardcoded `/usr` and `/lib`. Prince develops on NixOS, where programs live in `/nix/store`, and the project must also run for a stranger on Ubuntu.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Read-only mounts listed in policy, with `fhs` and `nix` presets | Taken | No distro knowledge in the code |
| Hardcode `/usr`, `/lib` | Discarded | Does not work on the development machine |
| Support NixOS only | Discarded | Nobody else could run the demo |

**What this means for the code.** The runtime finds the real path of each allowed command on the host and builds `PATH` from it. Both test suites must pass on both presets.

**Evidence.** Checked on 2026-11-10 on the development machine: NixOS 26.05, programs resolved under `/etc/profiles/per-user/...` (links into `/nix/store`); `pytest` was not installed.

**Concepts to learn.** Filesystem Hierarchy Standard (FHS); the Nix store; bind mounts; `PATH`.

### D-013 — No root: delegated cgroups through systemd
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** sandbox
- **ADR:** 0006

**In plain words.** Limits on memory, CPU and process count are set through cgroups. Normally that needs root. systemd can hand a normal user their own piece of the cgroup tree, so the runtime runs without root.

**The problem.** Requiring root to run a security tool is a bad look and a real risk.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Rootless, under `systemd-run --user --scope -p Delegate=yes` | Taken | Works without root on systemd machines |
| Require root | Discarded | A bug in the runtime would then be a bug with root power |

**What this means for the code.** A cgroup cannot hold both processes and child groups with limits, so the runtime first moves itself into a leaf group and then makes one child group per command. The runtime checks at startup that delegation works.

**Evidence.** Run on 2026-11-10 (kernel 7.1.8): `unshare -Urmpn` worked as a normal user; Landlock was in the active security module list; the user's cgroup had `cpu io memory pids`; inside a delegated scope a child cgroup could be created. Not yet checked on Ubuntu, where version 24.04 restricts unprivileged user namespaces by default.

**Concepts to learn.** cgroup v2; cgroup delegation; the "no internal processes" rule; user namespaces; systemd scopes.

### D-014 — Build the sandbox by hand
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** The cage is built directly from Linux kernel features instead of wrapping an existing tool like bubblewrap. It is more work, and it is where most of the learning is.

**The problem.** A hand-built sandbox is easier to get wrong than a battle-tested one.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Own code using namespaces, cgroups, Landlock | Taken | Learning value; this is what the interview questions will be about |
| Wrap bubblewrap or nsjail | Discarded for now | Less to get wrong, but hides the mechanisms |
| gVisor or microVMs | Discarded | Stronger isolation, far more setup; not needed for the v1 threat model |

**What this means for the code.** Read bubblewrap's source and the Codex CLI Linux sandbox first and copy their setup order. A fallback (wrap bubblewrap if the sandbox block overruns badly) was suggested in review; it is not part of the plan and would need its own entry.

**Evidence.** None; reasoning only.

**Concepts to learn.** Namespaces (user, mount, PID, network); `pivot_root`; capabilities; `PR_SET_NO_NEW_PRIVS`.

### D-015 — File tools run in the runtime, confined by `openat2`
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** sandbox
- **ADR:** 0001

**In plain words.** Reading and writing files for the agent is done by the trusted program itself, not inside the cage. To make sure a path like `../../etc/passwd` or a sneaky symlink cannot escape the folder, the program asks the kernel to refuse any path that leaves the folder.

**The problem.** Checking paths as text is unreliable: symlinks, `..`, and a file being swapped between the check and the use all defeat it.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| In the runtime, `openat2` with `RESOLVE_BENEATH` and `RESOLVE_NO_MAGICLINKS` | Taken | The kernel does the check and the open in one step |
| A helper process inside the sandbox | Discarded | A second process and protocol to build; no gain in v1 |
| String checks on the path | Discarded | Beaten by symlinks and races |

**What this means for the code.** Every `fs.*` call opens relative to a directory handle for the workspace. The security suite includes a loop that swaps a symlink while the agent writes.

**Evidence.** None yet; the week 3–5 experiment (a 30-line `openat2` program) will give some.

**Concepts to learn.** `openat2`; file descriptors and dirfds; symlinks; TOCTOU races.

### D-016 — How each tool is recovered after a crash
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** durability
- **ADR:** 0003, 0004

**In plain words.** After a crash, each unfinished step is examined. A read changed nothing, so it is marked "did not happen". For a file write, the diary stored a fingerprint of the file before and after; the runtime compares the file with both to see whether the write happened. For a command, nobody can know what it changed, so the folder goes back to the last save point.

**The problem.** "Uncertain" has to be turned into "happened" or "did not happen" before the agent continues.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Per-tool rules (read: not applied; write: compare hashes; exec: roll back) | Taken | Each tool gets the cheapest rule that is still correct |
| Roll back to the checkpoint for everything | Discarded | Simpler, but then nothing in v1 ever decides "applied", which is the interesting case |
| Pause and ask a human after an uncertain `exec` | Discarded | Nothing in v1 is irreversible, so a rollback is safe |

**What this means for the code.** The `fs.write` intent stores the path, the old hash and the new hash. File contents live in a blob folder named by hash; the log stores only hashes.

**Evidence.** None; the harness will test it.

**Concepts to learn.** Hashing (SHA-256); content-addressed storage; reconciliation.

### D-017 — The agent's conversation lives in the log
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** durability
- **ADR:** 0005

**In plain words.** Every message to and from the AI model is saved in the diary. After a crash, the conversation is rebuilt from the diary, and the agent is told what happened to each step that was cut off.

**The problem.** The model's memory of the task is only in the program's RAM, which is lost on a crash.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Each LLM message is an event; rebuild on resume | Taken | One source of truth |
| Restart the task from scratch | Discarded | Throws away work and avoids the hard part |

**What this means for the code.** A cut-off LLM call is simply sent again; the answer may differ, and that is accepted.

**Evidence.** None.

**Concepts to learn.** Event sourcing; LLM nondeterminism.

### D-018 — No network at all, so no web search
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** scope
- **ADR:** none

**In plain words.** Commands in the cage have no network. The gate has no web tool. The AI model itself is reached by the trusted program from outside the cage. Web search was asked about and left out.

**The problem.** Web search opens two holes the cage cannot close. The agent can put private data from the folder into a search query, which sends it out. And a web page can contain text telling the agent to do something else.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| No network, no web tool | Taken | The demo (fix a failing test) needs no web |
| A `web.search` tool run by the gate | Discarded for v1 | Needs an allowlist of sites and human approval to be safe; that is Phase 2 material |
| Give the cage internet | Discarded | Removes the main protection |

**What this means for the code.** Each command gets an empty network namespace. If search is ever added, it is a gate tool: checked by rules, written to the diary, run outside the cage.

**Evidence.** None; reasoning only.

**Concepts to learn.** Network namespaces; data exfiltration; prompt injection.

### D-019 — Things removed from v1
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** scope
- **ADR:** none

**In plain words.** Because this is a learning project and not a product, several planned pieces were dropped so the rest can be finished.

**The problem.** Each of these costs time and none is needed to prove the two main claims (the cage holds; recovery is correct).

**Options looked at.**
| Item | Result | Why |
|---|---|---|
| seccomp filter | Discarded | Was a stretch goal; the other layers cover the v1 threat model |
| Hash-chained log | Discarded | Only detects corruption; no v1 test depends on it |
| Large benchmark set | Discarded | Three numbers are enough: cage start time, one file write, whole demo time |
| `SandboxBackend` trait | Discarded | One backend does not need an abstraction |
| `QUEUED` and `PAUSED` states | Discarded | There is no scheduler and no pause feature |
| "TraceWeaver" as a separate name | Discarded | It is one CLI command, `agentos trace` |
| Web UI, replay, MCP, SDKs, multi-agent, remote workers, approvals | Discarded | Already cut in the first v1 draft |

**What this means for the code.** None of these appear in v1. They stay in the big spec as ideas.

**Evidence.** None.

**Concepts to learn.** seccomp; hash chains (only if curious).

### D-020 — Fake external tool is a bonus, not part of v1
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** scope
- **ADR:** none

**In plain words.** Everything the agent can do in v1 can be undone by going back to a save point. The harder case, a step that cannot be undone such as calling an outside service, never comes up. A small fake outside service would let that case be tested. It is allowed only after v0.1 is finished.

**The problem.** The review first said the durability machinery "has nothing to do" in v1. That was overstated: the `fs.write` hash check is a real decision between "happened" and "did not happen". What is true is that nothing in v1 is irreversible.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Bonus after v0.1 is tagged | Taken | Keeps the fixed outline; still available as a strong extra |
| Put it in v1 | Discarded | Adds scope; Prince wants the outline fixed |
| Never build it | Discarded | It is the best next step if there is time |

**What this means for the code.** Nothing until v0.1.

**Evidence.** None.

**Concepts to learn.** Idempotency keys; irreversible side effects.

### D-021 — Twelve weeks, not ten
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** The plan went from ten to twelve weeks. The extra two weeks go to the cage block and the recovery block, which are the hardest and are being learned at the same time as they are built.

**The problem.** Two weeks each for the sandbox and for durability, while learning Rust and alongside coursework, was the tightest part of the plan.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| 12 weeks: sandbox weeks 3–5, durability weeks 6–8 | Taken | More room where the risk is |
| Keep 10 weeks | Discarded | Likely to slip and eat into tests |

**What this means for the code.** If time runs short, it comes out of the documentation weeks, never out of the tests.

**Evidence.** None; an estimate.

**Concepts to learn.** None.

### D-022 — Claims the project does not make
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** The README will say plainly what is not promised: surviving a power cut, stopping a full disk, kernel bugs, and anything that happens after the human applies the patch.

**The problem.** The tests kill the process with `kill -9`. Data the operating system has accepted but not yet written to disk survives that, so the tests cannot show that the fsync calls are right. And a patch can change files (test hooks, CI scripts) that later run outside the cage.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| State the limits in the README and threat model | Taken | Honest, and reviewers look for it |
| Add power-loss testing (kill a virtual machine, or a fault-injecting filesystem) | Discarded for v1 | A project of its own |

**What this means for the code.** No wording like "durable against power loss" anywhere.

**Evidence.** None; this is how the page cache works.

**Concepts to learn.** Page cache; `fsync`; the difference between a process crash and a machine crash.

### D-023 — Project name
- **Date:** 2026-11-10
- **Status:** Open
- **Area:** process
- **ADR:** none

**In plain words.** "AgentOS" is a working name. The spec itself says it is not an operating system, and the name is used by other projects. No new name has been chosen.

**The problem.** A resume project is easier to find and remember with a name of its own.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Keep "AgentOS" for now | Current | No better name yet |
| Rename before v0.1 | Open | Suggested in review; not decided |

**What this means for the code.** Avoid baking the name deep into file formats until this is settled.

**Evidence.** None.

**Concepts to learn.** None.

### D-024 — Rust as the language, with reasons
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** The language stays Rust. Prince had no specific reason when he first chose it, so the choice was looked at again. Rust fits the work (low-level Linux calls in a security tool), the nearest real tools are written in it, and it adds a new skill to the resume.

**The problem.** D-003 recorded the choice with no reason. A choice without a reason is hard to defend in an interview and easy to regret mid-project.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Rust | Taken | Makes Linux system calls as directly as C; the compiler catches memory mistakes that would be security holes in a sandbox; similar tools (Codex CLI sandbox, youki) are Rust; it is used most in systems, infrastructure and security teams, which is where this project points |
| Go | Discarded | Most container tools are Go and there are more junior openings, but Go's runtime starts threads by itself, which makes namespace setup awkward; real Go container tools work around it with a C or re-exec helper |
| C++ | Discarded | Already familiar, so less to learn, but memory bugs in a security tool are the thing being prevented, and it adds nothing new to the resume |

**What this means for the code.** No change. The cost is learning Rust and Linux internals at the same time; week 0 and the 12-week plan (D-021) already allow for it.

A limit to keep in mind: one project does not make someone a "Rust developer". Rust openings are fewer than Go or C++ ones and often ask for experience. The project shows real Rust ability, and the Linux knowledge carries over to systems roles in any language.

**Evidence.** None; reasoning only. The statements about the job market are general knowledge, not something checked for this entry.

**Concepts to learn.** Memory safety; why threads and `unshare` do not mix (namespaces apply per thread); the Go runtime's threads.

### D-025 — The agent builds without stopping; Prince learns from the record
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** An AI agent builds the project from start to finish without asking questions along the way. Prince does not follow each step live. Instead, everything is written down as it happens, and he studies it afterwards with an AI's help.

**The problem.** The first working rule was step by step: explain, show a little code, wait for Prince. That is slow, and Prince preferred a complete record to study over being present for each step.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Agent builds on its own and documents everything | Taken | Prince's choice; gives a full written story of the build |
| Step by step, waiting for Prince each time | Discarded | Was the first rule in `CLAUDE.md`; replaced |
| Prince writes the code, the agent reviews | Discarded | Offered as an option; not chosen |

**What this means for the code.** The agent makes choices and logs them instead of asking. It stops only for things only Prince can provide, or when going on would mean faking a result. Tests are never weakened to pass. The risk is that code exists which Prince cannot yet explain; the milestone documents (D-026) and their question lists are there to close that gap before the project goes on the resume.

**Evidence.** None; stated by Prince.

**Concepts to learn.** None.

### D-026 — One document per milestone, and git history as a storyline
- **Date:** 2026-11-10
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** Each of the seven milestones gets its own document: the aim, the requirements and proof they were met, the code explained piece by piece, the decisions, the problems, and questions to test understanding. The git history is written as a story too: small commits, each saying what changed and why, pushed as the work goes.

**The problem.** The decision log explains choices, but not the code itself or the order things were built in.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Milestone documents from a fixed template, plus small explained commits | Taken | Covers the code and the order of events; the template keeps every milestone equally thorough |
| Decision log only | Discarded | Does not explain the code |
| One document per week | Discarded | Some milestones span three weeks; splitting them would cut explanations in half |
| Heavy comments in the code instead | Discarded | Comments explain a line, not the story; and they go stale |

**What this means for the code.** Documents live in `docs/milestones/`. A milestone is not finished until its document is. The agent adds no Claude attribution to commits. (This entry first allowed the agent to push; D-027 replaced that part the same day.) Each milestone ends with a tag (`m0` … `m6`, then `v0.1`).

**Evidence.** None.

**Concepts to learn.** Git tags; commit messages (subject and body).

### D-027 — The agent commits but never pushes
- **Date:** 2026-11-10
- **Status:** Taken (replaces the push permission in D-026)
- **Area:** process
- **ADR:** none

**In plain words.** The agent saves its work as commits in the local repository and must make a commit every time a milestone is achieved. It never uploads anything to GitHub. Prince does every push himself.

**The problem.** D-026 let the agent push whenever it wanted. Prince changed his mind shortly after: a push is public and hard to take back, and he wants to decide when work leaves his machine.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Commit locally, always at each milestone; never push | Taken | Prince's decision; keeps the full history while he controls what is published |
| Push whenever a step is finished | Discarded | Was allowed in D-026; withdrawn |

**What this means for the code.** No `git push` in any form, including tags, and no other command that uploads the repository. Milestone tags are local. One consequence: the Ubuntu (`fhs` preset) tests run in CI, and CI only runs after a push, so the agent cannot confirm them itself. It records them as "not yet verified" until Prince pushes and the run is green.

**Evidence.** None; stated by Prince.

**Concepts to learn.** Local versus remote repository; what `git push` does; continuous integration (CI).

### D-028 — The resume file in the project folder is not committed
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** The project folder also holds Prince's resume (a `.tex` file). It is personal, not part of AgentOS, so it stays out of git. A line in `.gitignore` makes sure it is never committed by accident.

**The problem.** The first commit was "commit the existing docs". The folder had one file that is not a project document.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Leave it out and ignore `/*.tex` | Taken | Personal data stays out of a repository that will be public; no project file is a `.tex` |
| Commit it with the docs | Discarded | It is a resume with contact details, not a project document |
| Move or delete it | Discarded | It is Prince's file; the agent does not move his files |

**What this means for the code.** `.gitignore` has `/*.tex`. If the project ever needs a LaTeX file, that line must change.

**Evidence.** Read the first lines of the file: it is a LaTeX resume template.

**Concepts to learn.** `.gitignore`.

### D-029 — Development tools come from a pinned Nix dev shell
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** The compiler, Python with pytest, and hyperfine are provided by `flake.nix`. Running `nix develop` opens a shell with exactly those versions. The versions are fixed by `flake.lock`, so the same commands give the same tools next month. On Ubuntu, the README will list the normal packages instead.

**The problem.** The machine had a broken Rust setup: `rustup` knew a 1.87 toolchain, but that toolchain had no `rustc` in it, and no toolchain was active. `pytest` and `hyperfine` were not installed at all.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `flake.nix` dev shell, pinned by `flake.lock` | Taken | Listed in the v1 stack table; fixes the versions; changes nothing outside the project |
| `rustup toolchain install stable` + `pip install pytest` | Discarded | Changes Prince's global setup; versions drift; pip on NixOS fights the system |
| `rust-toolchain.toml` with rustup | Discarded for development | Pins Rust only, not pytest or hyperfine. Still the simplest path for an Ubuntu user, so the README will mention rustup |

Which nixpkgs commit to pin was a second, smaller choice:

| Option | Result | Why |
|---|---|---|
| The commit the development machine already runs (`c69ae8fb…`, from `nixos-version --json`) | Taken | Rust, Python, gcc and make were already on disk, so only pytest, hyperfine and clippy (a few MB) had to be downloaded |
| Latest `nixos-unstable` | Discarded | Tried first. It needed a new gcc, LLVM, Python and Rust (hundreds of MB); at the measured download speed that would have taken hours |

**What this means for the code.** All commands are run as `nix develop -c <command>` on the development machine. Nothing in `src/` may depend on Nix; the flake is only a convenience. `rusqlite` uses its bundled SQLite, so no system SQLite library is needed. The shell does not set `RUST_BACKTRACE` (see Findings, 2026-11-11).

**Evidence.** `rustup show` printed "no active toolchain"; `RUSTUP_TOOLCHAIN=1.87.0 rustc --version` failed with "Missing manifest"; `~/.rustup/toolchains/1.87.0-.../bin` held only `cargo` and clippy. With the pinned commit, `nix develop -c ...` printed rustc 1.95.0, cargo 1.95.0, pytest 9.0.3, hyperfine 1.20.0. The `nixos-unstable` attempt ran for 22 minutes and was stopped; `curl` measured about 30 KB/s from cache.nixos.org at that time.

**Concepts to learn.** Nix flakes; lock files; toolchains.

### D-030 — The week-0 program is a separate throwaway crate
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** The week-0 practice program (read a TOML file, write rows to SQLite) lives in `experiments/m0-ramp/` as its own small Rust project. It is not part of the `agentos` program.

**The problem.** The v1 CLI is fixed at four commands (`run`, `status`, `trace`, `recover`). A practice command inside it would add a fifth that nobody needs.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Own crate under `experiments/` | Taken | Same place as the other experiments; the real crate starts clean in M1 |
| A subcommand of `agentos` | Discarded | Adds CLI surface the plan does not list |
| Start the `agentos` crate now and grow it | Discarded | Mixes practice code into the real code's history |

**What this means for the code.** The practice crate uses the same libraries the runtime will use (`serde`, `toml`, `rusqlite`, `clap`, `anyhow`, `thiserror`), so each is met once alone. It has its own `Cargo.toml` and is not a workspace member.

**Evidence.** None; reasoning only.

**Concepts to learn.** Crates and packages; `Cargo.toml`.

### D-031 — Crate versions: the newest release of each crate in the stack table
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** Each library from the v1 stack table is used at its newest version as of today: `rusqlite` 0.42 (with bundled SQLite), `toml` 1, `clap` 4, `serde` 1, `serde_json` 1, `anyhow` 1, `thiserror` 2, `sha2` 0.10, `nix` 0.31. `Cargo.lock` is committed, so a build next year uses the same versions.

**The problem.** The stack table names crates but not versions. Old versions would need upgrading mid-project.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Newest release, locked by a committed `Cargo.lock` | Taken | Current documentation matches the code; the lock file makes builds repeatable |
| Versions already in the local cargo cache | Discarded | Saves a few downloads, but some were old (`toml` 0.x) |

**What this means for the code.** `Cargo.lock` is committed for the `agentos` crate and for each experiment. `toml` 1.x is used even though many examples online show 0.x; the reading API (`toml::from_str`) is the same.

**Evidence.** `cargo search` on 2026-11-11 listed rusqlite 0.42.0, nix 0.31.3, landlock 0.4.7, ureq 3.5.1, criterion 0.8.2. The M0 program built and passed its tests with these.

**Concepts to learn.** Semantic versioning; `Cargo.lock`.

### D-032 — A small glob matcher written in the code, not a crate
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** policy
- **ADR:** none

**In plain words.** The policy says which paths are allowed with patterns such as `src/**` or `**/.env`. Matching a path against such a pattern is about 30 lines of code, so it is written in `policy.rs` instead of adding a library. Writing it also fixes the exact rules, which matter for security.

**The problem.** The v1 stack table lists no glob library, and adding a dependency needs a reason. The matching rules must be exact: whether `secrets/**` also covers the folder `secrets` itself decides whether that folder is copied into the workspace.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Own matcher: `**` = zero or more whole folders, `*` and `?` inside one name, nothing else | Taken | No dependency; the rules fit in five lines and have unit tests |
| `glob` crate (`Pattern`) | Discarded | Small and dependency-free, but its `**` rules and options are easy to get subtly wrong for this use |
| `globset` crate | Discarded | Pulls in regex and aho-corasick; far more than needed |

**What this means for the code.** Patterns are relative to the workspace root, with no leading `/` and no `.` or `..` parts; the policy loader refuses anything else. `secrets/**` matches `secrets`, `secrets/a` and `secrets/a/b`. `**/.env` matches `.env` at any depth. `*` never crosses a `/`. There are no `[abc]` classes.

**Evidence.** `policy::tests::glob_basics` (15 cases) and `deny_wins_over_allow` pass.

**Concepts to learn.** Glob patterns; recursion; why "small and exact" can beat "general" for security rules.

### D-033 — Event log layout: one SQLite file per execution, one row per event
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** Each execution has its own folder with its own `log.db`. Inside, a table `events` holds one row per event: a sequence number (1, 2, 3, … with no gaps), the time, the event type, the tool call it belongs to, and the full event as JSON. Every write is fsynced before the runtime goes on.

**The problem.** D-005 chose SQLite but not the table layout, and said `synchronous=FULL` "for intent records". The layout decides how recovery and `trace` read the log.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| One `events` table; a typed Rust enum stored as JSON in `body`; `seq` set by the runtime | Taken | One append path; the enum makes illegal events hard to write; JSON keeps the schema open while it settles |
| Separate tables per event type | Discarded | More SQL, and "everything in order" needs a join across tables |
| One database for all executions | Discarded | Two runs would share one file and one lock; deleting a run would mean deleting rows, breaking "append only" |
| `synchronous=FULL` only while writing intents, `NORMAL` otherwise | Discarded for now | Switching the setting per write is easy to get wrong, and with WAL a later fsync covers earlier commits anyway. One rule ("every event is on disk when `append` returns") is easier to reason about. M5 measures the cost |

**What this means for the code.** `log.rs`: `EventLog::append` assigns `seq = last + 1` and commits one row. A `meta` table stores `schema_version = 1`. Call states are worked out by reading events in order (`log::calls`), which also refuses illegal orders such as COMPLETED without an intent.

**Evidence.** `log::tests::append_and_read_back_in_order` (the sequence continues after reopening) and `illegal_transitions_are_reported` pass.

**Concepts to learn.** Event sourcing; schema versioning; serde's internally tagged enums (`#[serde(tag = "type")]`).

### D-034 — No separate STARTED record; the intent marks the start
- **Date:** 2026-11-11
- **Status:** Taken (replaces the state list in D-006)
- **Area:** durability
- **ADR:** none

**In plain words.** D-006 listed the states of a tool call as REQUESTED → INTENT_LOGGED → STARTED → COMPLETED or FAILED. Nothing happens between writing the intent and starting the effect, so a separate STARTED record would be a second fsync that tells recovery nothing new. The intent record itself means "about to start".

**The problem.** Every record costs an fsync. A record is worth writing only if recovery would decide differently with it.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| REQUESTED → DENIED, or REQUESTED → INTENT → COMPLETED / FAILED; a crash after INTENT means UNCERTAIN | Taken | Each record changes what recovery can conclude |
| Keep STARTED between INTENT and the effect | Discarded | Intent-without-STARTED would prove "not started", but the two writes are back to back, so that case is only a crash between two fsyncs. It saves no real work |

**What this means for the code.** `log::CallState` has `Requested`, `Denied`, `IntentLogged`, `Completed`, `Failed`. Recovery (M3) adds UNCERTAIN and RECONCILED as records written after a crash. D-006's main decision (intent before effect) still holds, so its status is unchanged; this entry replaces only its list of states.

**Evidence.** None; reasoning only.

**Concepts to learn.** State machines; the cost of fsync.

### D-035 — The output patch never follows symlinks
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** The patch is made with `diff -ruN`. By default `diff` follows symlinks. If the agent made a symlink in the workspace pointing at a host file such as `~/.ssh/id_ed25519`, the runtime (which runs outside the sandbox) would read that file and put its content into the patch. `--no-dereference` makes `diff` compare the links themselves.

**The problem.** Anything the trusted runtime reads on the agent's behalf can leak host files. The patch step is easy to forget because it runs after the agent is done.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `diff -ruN --no-dereference` | Taken | One flag; a symlink shows up as a changed link target |
| Plain `diff -ruN` | Discarded | Follows links out of the workspace |

**What this means for the code.** `checkpoint::write_patch` passes the flag and rewrites header paths to `a/` and `b/`, so the patch applies with `patch -p1` or `git apply`. The security suite (M2) gets a test for it.

**Evidence.** The demo patch applied with `patch -p1 --dry-run` and `git apply --check` (both exit 0). The symlink case is reasoning until the M2 test exists.

**Concepts to learn.** Symlinks; the confused-deputy problem (a trusted program misused by an untrusted one).

### D-036 — Where things live: one folder per execution
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** All state goes under a state folder (`--state-dir`, else `$AGENTOS_STATE_DIR`, else `~/.local/share/agentos`). Each execution has `executions/<id>/` holding `log.db`, a copy of the policy, a lock file, `work/` (the workspace), `checkpoints/`, `blobs/` and `result.patch`. Only `work/` is ever shown to the sandbox.

**The problem.** The log and the checkpoints must be out of the sandbox's reach (design rule 9), and recovery must find everything from the execution id alone.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| One folder per execution with fixed names inside | Taken | Recovery needs only the id; deleting a run is deleting a folder |
| Workspaces in `/tmp` | Discarded | Often a tmpfs, emptied on reboot; the same problem as D-010 |
| Separate top-level folders for logs, blobs and workspaces (as in the big spec §10.3) | Discarded | Spreads one execution over several places for no gain |

**What this means for the code.** `workspace::ExecDir` has one method per path. The id is `YYYYMMDD-HHMMSS-xxxxxx` in UTC, so ids sort by start time; the suffix comes from hashing the clock's nanoseconds and the process id. The policy text is copied into the folder, so recovery uses exactly the policy the run started with.

**Evidence.** `workspace::tests::execution_ids_sort_by_time` passes; the demo run created the layout above.

**Concepts to learn.** XDG base directories.

### D-037 — The scripted agent reads TOML and repeats a step that was not applied
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** The test agent plays a list of `[[step]]` entries from a TOML file, one per tool call. TOML's triple-quoted literal strings hold a whole Python file without escaping. If a call comes back NOT APPLIED (after a crash, recovery undid it), the agent asks for the same step again; any other result moves it to the next step.

**The problem.** The fault-injection harness needs an agent that is deterministic and that behaves sensibly after recovery.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| TOML script, same fields as `ToolCall` | Taken | Same parser as the policy; file content stays readable |
| JSON script | Discarded | Python code would need `\n` escapes on every line |
| Steps written in Rust test code | Discarded | The security suite wants many small scripts; files are easier to read and to run from the CLI |

**What this means for the code.** `agent/scripted.rs`. A denied or failed step is *not* repeated, so an attack script does not loop.

**Evidence.** `agent::scripted::tests` (2 tests) pass; `examples/scripts/fix-demo.toml` fixes the demo repo.

**Concepts to learn.** Determinism; replay.

### D-038 — M1 file tools use a text-based path check, replaced in M2
- **Date:** 2026-11-11
- **Status:** Replaced by D-047
- **Area:** sandbox
- **ADR:** none

**In plain words.** In M1 the file tools clean up the agent's path as text (drop `.`, apply `..`, refuse a leading `/` or a climb above the root) and then open it with normal file calls. Design rule 7 says confinement must never rely on string checks, because a symlink inside the folder can still lead outside. M1 has no sandbox on purpose (the plan builds it in M2), so this is a known, stated gap until then.

**The problem.** The vertical slice needs working file tools before the `openat2` experiment that the plan schedules for M2.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Text check now; `openat2` in M2, with a test that shows the text check failing | Taken | Keeps the plan's order; the M2 test makes the reason for `openat2` concrete |
| Build `openat2` in M1 | Discarded | Skips the M2 experiment the plan asks for first |
| No check at all in M1 | Discarded | `Path::join` with an absolute path replaces the base entirely (`/ws` joined with `/etc/passwd` is `/etc/passwd`) |

**What this means for the code.** The comment on `tools/fs.rs::resolve` says it is not a security boundary. M2 must replace it.

**Evidence.** `tools::fs::tests::resolve_cleans_paths_and_refuses_escapes` passes. It does not, and cannot, catch symlinks.

**Concepts to learn.** Path canonicalization; why `Path::join` with an absolute path discards the base.

### D-039 — One runtime per execution, enforced with a file lock
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** While a runtime works on an execution it holds a lock on `executions/<id>/lock`. A second `agentos` process on the same execution stops with a clear message. The kernel drops the lock when the process dies, even on `kill -9`, so a crash never leaves a stale lock. `status` uses the same lock to tell "running" from "interrupted".

**The problem.** If recovery ran while the original runtime was still alive, both would change the workspace at once.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `flock` through Rust's `File::try_lock` | Taken | Released by the kernel on death; no stale-lock cleanup |
| A PID file | Discarded | Stays behind after `kill -9`; PIDs are reused |
| No lock | Discarded | Two writers on one log and one workspace |

**What this means for the code.** `runtime::lock_execution` and `is_locked` (since D-071, taking the lock retries for up to three seconds). `File::try_lock` became stable in Rust 1.89, so `Cargo.toml` sets `rust-version = "1.89"`.

**Evidence.** None yet; the M3 harness relies on it.

**Concepts to learn.** Advisory locks (`flock`); what the kernel cleans up when a process dies.

### D-040 — How malformed and failing calls are recorded
- **Date:** 2026-11-11
- **Status:** Taken; its last rule ("`fs.write` does not create folders") is replaced by D-084
- **Area:** policy
- **ADR:** none

**In plain words.** A request that is malformed (an empty argv, a path climbing out with `..`) is refused before any intent, like a policy denial, with the rule `invalid request`. A request that is allowed but cannot be done (the file does not exist) gets an intent and then a FAILED record. `fs.write` does not create folders: the folder must already exist.

**The problem.** Every call must end in exactly one terminal record, and recovery must never meet a half-done write it cannot explain.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Malformed = DENIED (no intent); allowed-but-impossible = INTENT then FAILED | Taken | "Denied" always means "nothing was attempted"; every allowed call has an intent |
| A separate INVALID state | Discarded | One more state that recovery would treat exactly like DENIED |
| `fs.write` creates missing folders | Discarded | A crash after making the folder but before writing the file would leave a change that matches neither the old nor the new hash, so recovery would have to roll back more often |

**What this means for the code.** `gateway.rs`: `decide` returns an error for malformed requests; `fail_before_intent` writes intent + failed. `fs.write` fails with "the folder for `x` does not exist".

**Evidence.** `tests/vertical_slice.rs::denied_command_and_unknown_path_are_reported` passes.

**Concepts to learn.** Terminal states.

### D-041 — `fs.read` returns UTF-8 text only, capped by `max_output_kb`
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** policy
- **ADR:** none

**In plain words.** `fs.read` returns a file's text. A file that is not valid UTF-8 text, or larger than `limits.max_output_kb`, is refused with a message. The same limit caps how much of a command's output is kept.

**The problem.** The agent is a language model reading text. A huge or binary file would flood its context and the log.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Text only; refuse above the limit | Taken | A clear failure the agent can act on |
| Return the first N bytes | Discarded | A cut-off source file looks complete and misleads the agent |
| Base64 for binary files | Discarded | No v1 task needs binary files |

**What this means for the code.** `tools/fs.rs::read`. `fs.write` content is a `String`, so it is also text only.

**Evidence.** None; reasoning only.

**Concepts to learn.** UTF-8.

### D-042 — The sandbox is set up by a fresh copy of the agentos binary
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** To run a command in the cage, the runtime starts its own program a second time with a hidden first argument (`__sandbox-init`). That fresh process builds the cage around itself and then turns into the command. Three processes are involved: the helper H (joins the cgroup, creates the namespaces), its child C (process number 1 inside the cage; builds the file view, drops rights, then stays as a tiny "init"), and C's child G (becomes the command with `execve`).

**The problem.** The runtime has several threads (watchdog, output readers). The kernel refuses `unshare(CLONE_NEWUSER)` in a process with more than one thread, and running complex code between `fork` and `exec` in a multi-threaded process can deadlock. D-024 already noted that Go tools hit the same wall.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Re-run the same binary as a single-threaded helper | Taken | A fresh process has one thread; one binary to ship; same trick as runc and Codex's Linux sandbox |
| `fork` inside the runtime and do the setup in the child | Discarded | Unsafe with threads; `unshare` of a user namespace fails outright |
| A second, separate helper binary | Discarded | Two binaries to build, find and keep in sync |
| Command runs as PID 1 itself (no init process C) | Discarded | PID 1 ignores most signals and must reap orphans; ordinary programs do neither |

**What this means for the code.** `main.rs` checks for `__sandbox-init` before clap, so it never appears in `--help` and is not a fifth CLI command. The spec travels as one JSON argument. Results come back on a pipe placed on file descriptor 3: `error: …` for setup failures, `exec-failed: …`, then `exit N` or `signal N`. H and C set `PR_SET_PDEATHSIG(SIGKILL)`, so if the runtime is killed, H dies, then C dies, and the kernel kills everything else in the PID namespace.

**Evidence.** Probe run inside the sandbox printed `pid: 2`, processes visible `[1, 2]`. Test `background_processes_die_with_the_command` passes.

**Concepts to learn.** `fork` and `execve`; why threads and `fork` do not mix; PID 1 and zombie reaping; `PR_SET_PDEATHSIG`; close-on-exec.

### D-043 — Inside the sandbox the user keeps its own uid, with no capabilities
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** `unshare -r` maps the user to "root" (uid 0) inside the namespace. AgentOS instead maps uid 1000 to uid 1000. The helper still gets full powers inside the new namespace for the setup (the kernel grants them to whoever creates a user namespace, whatever the mapping), and then throws all of them away before the command starts.

**The problem.** Programs behave differently when they think they are root (pip warns, some tools refuse to run, files look root-owned).

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Identity map (`uid uid 1`), then drop every capability explicitly | Taken | The command sees a normal user; nothing depends on "execve drops caps for non-root" alone |
| Map to root inside | Discarded | Confusing for tools; one mistake in dropping capabilities would leave "root in the namespace" |

**What this means for the code.** `ns.rs::drop_privileges`: drop every capability from the bounding set, clear ambient capabilities, set `no_new_privs`, then `capset` everything to zero. `capset` and the bounding-set calls are raw syscalls through `libc` (the `nix` crate has no wrapper).

**Evidence.** Probe inside the sandbox: `uid 1000`, `CapPrm/CapEff/CapBnd: 0000000000000000`, `NoNewPrivs: 1`, `os.setuid(0)` fails. Test `exec_has_no_capabilities_and_cannot_gain_root` passes.

**Concepts to learn.** uid maps; capability sets (effective, permitted, bounding, ambient); `no_new_privs`.

### D-044 — The sandbox's file view: built on a tmpfs, entered with two `pivot_root` calls
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** The cage's root folder is built from nothing: an empty in-memory folder that gets `/work` (the workspace, writable), the policy's system folders (read-only), a `/dev` with five device files, a fresh `/proc`, and an empty `/tmp`. Then the process swaps its root to that folder and detaches the host's root. There is no `/home`, no `/etc`, no `/sys`. The steps and their order are copied from bubblewrap.

**The problem.** `experiments/m2-unshare` showed a new mount namespace hides nothing by itself. Most namespace bugs are ordering bugs (v1 design), so the order should come from a tool that has had years of review.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| bubblewrap's order: tmpfs on `/tmp` → `pivot_root` (host root becomes `/oldroot`) → bind mounts from `/oldroot/...` → detach `/oldroot` → `pivot_root(".", ".")` | Taken | Works even when the workspace itself is under `/tmp` (the test suites' is not, but a user's may be) |
| `chroot` | Discarded | Can be escaped by a process with the right capability; `pivot_root` plus detaching the old root leaves nothing to escape to |
| Bind-mount `/` read-only and hide parts | Discarded | "Allow a few folders" is safer than "hide the dangerous ones" |

**What this means for the code.** `ns.rs::build_root`. Details that matter: every read-only mount is remounted with `MS_RDONLY|MS_NOSUID|MS_NODEV` while keeping the flags the host set (the kernel refuses to clear those inside a user namespace); a policy mount that is a symlink on the host (`/bin -> usr/bin`) is recreated as a symlink; a missing one (`/lib64` on some systems) is skipped; `/work` is `nosuid,nodev`; the root itself ends read-only. The loopback interface is brought up (the v1 table says "loopback only"); there is still no route anywhere else. Sandbox `/tmp` is a tmpfs, so it is counted against the cgroup's memory limit.

**Evidence.** Probe inside the sandbox listed exactly these mounts: `/` (ro), `/work` (rw), `/nix/store` (ro), `/dev` + 5 device files, `/proc`, `/tmp`. `/home` and `/etc/passwd` gave "No such file", writing `/x` and `/nix/store/x` gave "Read-only file system", connecting to 1.1.1.1 gave "Network is unreachable", connecting to its own loopback listener worked.

**Concepts to learn.** `pivot_root` versus `chroot`; mount propagation (`MS_PRIVATE`); locked mount flags; tmpfs.

### D-045 — Landlock rules: built for ABI 5, best effort on older kernels
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** After the new root is in place, the helper applies Landlock: read and execute under the system folders; everything under `/work` and `/tmp`; read and write on five device files; read under `/proc`. Anything else is refused, even listing `/`. This is a second lock behind the mount setup.

**The problem.** Landlock has grown in versions ("ABIs"). Code must say which rights it handles, and what to do on a kernel that knows fewer.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Handle all filesystem rights of ABI 5, `CompatLevel::BestEffort`, refuse to run if nothing is enforced | Taken | Newer kernels enforce everything; older ones enforce what they know; never silently unconfined |
| Require the newest ABI | Discarded | Would not start on Ubuntu 22.04-era kernels for no real gain |
| Also use Landlock's network and signal scoping (ABI 4, 6) | Discarded | The network and PID namespaces already cover those; not in the v1 table |

**What this means for the code.** `sandbox/landlock.rs`. One useful side effect, checked by a test: Landlock forbids `mount` for restricted processes, so a command that creates its own nested user namespace (which the kernel allows) still cannot rearrange its file view.

**Evidence.** `experiments/m2-landlock`. Tests `exec_cannot_mount_even_in_a_nested_user_namespace` and the probe (`os.listdir("/")` → Permission denied) pass.

**Concepts to learn.** Landlock ABIs; best-effort compatibility; defence in depth.

### D-046 — `libc` as a direct dependency
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** The `libc` crate (raw bindings to system calls) is used directly in five places where the `nix` crate has no safe wrapper: `capset`, dropping bounding capabilities, bringing up the loopback interface (`ioctl`), `close_range`, and asking the kernel for its Landlock version. It was already in the build because `nix` and `landlock` depend on it.

**The problem.** The v1 stack table lists `nix` "(raw syscall if `nix` lacks it)" but not `libc` by name. A dependency outside the table needs an entry.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Use `libc` directly for those five calls | Taken | No new code enters the build; each `unsafe` block is commented |
| The `caps` crate, a netlink crate, etc. | Discarded | Three more dependencies to save about 40 lines |

**What this means for the code.** Every `unsafe` block has a `SAFETY:` comment saying what it relies on (CLAUDE.md code style). `openat2` did **not** need a raw syscall: `nix` 0.31 has it.

**Evidence.** `cargo tree` shows `libc` once. `cargo clippy --all-targets` is clean.

**Concepts to learn.** `unsafe` in Rust; FFI.

### D-047 — File tools resolve every path with `openat2`; policy is checked on the resolved path
- **Date:** 2026-11-11
- **Status:** Taken (replaces D-038)
- **Area:** sandbox
- **ADR:** 0001

**In plain words.** The file tools now open every path through the kernel's "stay beneath this folder" mode. The policy step keeps the opened handle and the effect uses that same handle, so what was checked is what is used. The policy is asked twice: once on the name the agent typed (deny rules only), and once on the path the kernel actually arrived at (all rules).

**The problem.** D-038's text check cannot see symlinks. And a check followed by a separate open leaves a gap in which a symlink can be swapped (TOCTOU).

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `openat2(RESOLVE_BENEATH \| RESOLVE_NO_MAGICLINKS)`, keep the fd, check policy on the fd's real path | Taken | Check and open are one kernel call; the fd cannot be redirected afterwards |
| `RESOLVE_NO_SYMLINKS` for everything | Discarded | Refuses harmless symlinks that stay inside the repo |
| Canonicalize the path, check the prefix, then open | Discarded | The classic TOCTOU bug |

**What this means for the code.** `sandbox/beneath.rs`, `tools/fs.rs`, `gateway.rs::decide`. An escape is logged as `tool.denied` with rule `path leaves the workspace (openat2: …)`. `fs.write` opens the *parent folder* beneath the workspace, creates the temp file with `O_EXCL|O_NOFOLLOW`, and renames it over the target by name; `renameat` never follows a symlink at the target. If the target is currently a symlink or not a regular file, the write fails rather than replacing it. `beneath::lexical_rel` still cleans paths as text, but only to match deny rules on the requested name; it confines nothing.

**Evidence.** `experiments/m2-openat2`; unit test `escapes_are_refused_by_the_kernel`; seven security tests in section 1 of `tests/security/main.rs`.

**Concepts to learn.** File descriptors as capabilities; `O_PATH`; `O_NOFOLLOW`; `/proc/self/fd`.

### D-048 — How changes outside the write globs are found and undone after `exec`
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** policy
- **ADR:** 0009

**In plain words.** After each command, the runtime walks the workspace and the last checkpoint side by side. For every path the write globs do not allow, the workspace is made equal to the checkpoint again: new things are deleted, missing or changed things are restored. The list of touched paths is logged as a `policy.violation` event and shown to the agent. Files are compared by content, never by timestamp.

**The problem.** D-009 decided *that* this happens; the details decide whether it is safe. The undo runs in the trusted runtime on a folder the attacker shaped. If the attacker replaced `tests` with a symlink to `~`, a naive "restore `tests/data.txt`" would write into the home folder.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Every delete/restore goes through a handle on the parent folder, opened with `RESOLVE_BENEATH \| RESOLVE_NO_SYMLINKS` | Taken | A symlink anywhere on the way makes the open fail instead of being followed |
| Plain path operations (`fs::copy(base/x, work/x)`) | Discarded | Follows planted symlinks out of the workspace |
| Compare size and mtime | Discarded | The attacker controls mtime |
| Throw the whole workspace away and restore the checkpoint if anything is outside | Discarded | Would also discard the allowed changes |

**What this means for the code.** `checkpoint::revert_outside`. A new folder outside the globs is removed only if it ends up empty (it may hold allowed files). Because `src/**` also matches `src` itself (D-032), changing the mode of `src` is an allowed change. No sandboxed process is alive during the revert (the exec's cgroup has been destroyed), so nothing can swap paths under it. In M2 the comparison base is `cp-0000` plus allowed changes since; M3 moves it to the per-call checkpoint.

**Evidence.** Tests `exec_changes_outside_write_globs_are_reverted` and `exec_cannot_replace_a_folder_with_a_symlink_to_escape_the_revert` pass.

**Concepts to learn.** Confused deputy; `RESOLVE_NO_SYMLINKS`.

### D-049 — cgroup details: leaf group, retries, a systemd check, a probe, `cgroup.kill`
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** sandbox
- **ADR:** 0006

**In plain words.** At startup the runtime (1) asks systemd whether its cgroup was delegated and stops if the answer is no, (2) moves every process of the delegated group into a child group called `agentos-runtime`, (3) turns on the memory, pids and cpu controllers for children, and (4) creates and removes a throwaway group with all three limits as a probe. Each command then gets its own group named `x-<execution id>-c<call id>`. The watchdog kills a command by writing `1` to the group's `cgroup.kill`.

**The problem.** D-013 chose delegated cgroups; three practical problems showed up while building (see Findings): other processes share the scope, several runtimes can start at once, and a non-delegated scope half-works.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Move *all* processes of the scope into the leaf, retrying while new ones appear | Taken | The runtime is rarely alone (`cargo`, a shell); the kernel refuses to enable controllers while any process sits in the parent |
| Move only the runtime itself (as the v1 text says) | Discarded | Fails with EBUSY whenever a parent process shares the scope |
| Trust "mkdir worked" as proof of delegation | Discarded | A user can create child groups in a non-delegated scope too; the run then failed at the first limit, after disturbing the terminal's cgroup |
| Kill by process id | Discarded | Misses children; ids can be reused. `cgroup.kill` has neither problem |

**What this means for the code.** `sandbox/cgroup.rs`. `memory.swap.max` is set to 0 so the memory limit cannot be dodged through swap. The group name is fixed by execution and call id, so M3's recovery can find and kill a group left by a crashed runtime. The systemd question is asked with `systemctl show <unit> -p Delegate`; if `systemctl` is missing, the probe alone decides. The v1 document's sentence "the runtime first moves itself into a leaf group" is updated to say it moves the scope's processes.

**Evidence.** `experiments/m2-pids`. Without `systemd-run`, `agentos run` now stops before creating anything, with a message naming the fix. Tests `fork_bomb_is_stopped_by_pids_max`, `memory_exhaustion_is_stopped_by_memory_max`, `infinite_loop_is_killed_at_the_time_limit` pass; the full suite passed five runs in a row with 25 runtimes starting concurrently in one scope.

**Concepts to learn.** cgroup v2 "no internal processes" rule; `cgroup.subtree_control`; `cgroup.kill`; OOM killer; systemd scopes and `Delegate=`.

### D-050 — Symlinks in the output patch are listed as comments
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** policy
- **ADR:** none

**In plain words.** A normal patch file can describe changed text, not symlinks. When the agent adds or removes a symlink, the patch starts with comment lines such as `# agentos: symlink added: src/leak.py -> /some/target`. `patch` ignores them; the human sees them and decides.

**The problem.** With `--no-dereference` (D-035), GNU diff exits with an error when a symlink exists on one side only, and the whole run failed at the last step.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Find one-sided symlinks ourselves, accept diff's error only for exactly those paths, add comment lines | Taken | Smallest change; nothing is silently dropped |
| Drop `--no-dereference` | Discarded | Reopens the leak D-035 closed |
| `git diff --no-index` (can express symlinks) | Discarded | Adds git as a runtime dependency; the plan says `diff -ruN` |

**What this means for the code.** `checkpoint::write_patch`. Any other diff error still stops the run.

**Evidence.** Reproduced by hand with diffutils 3.12 (`diff: a/new: No such file or directory`, exit 2). Test `patch_does_not_follow_symlinks_to_host_files` passes: the link is named in the patch and the linked file's content is not.

**Concepts to learn.** Unified diff format and its limits.

### D-051 — How the security suite plays the attacker
- **Date:** 2026-11-11
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** Each security test is a small script for the scripted agent. Attacks through the file tools are plain `fs.*` steps. Attacks "from code run by an allowed command" use a test policy that allows `python3`, and run Python that tries the forbidden thing and prints `ESCAPED: …` if it works or `BLOCKED: …` if it is stopped. A test fails on any `ESCAPED`, and also if an expected `BLOCKED` line is missing.

**The problem.** The threat model says any allowed test command is arbitrary code execution. The tests need a direct way to run arbitrary code, and must not pass just because an attack script crashed early.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Allow `python3` in the test policy; ESCAPED/BLOCKED lines; require the BLOCKED lines | Taken | Same power as `pytest` + `conftest.py`, far less boilerplate; a crashed attack fails the test |
| Only `pytest` with a `conftest.py` per attack | Discarded as the default | Slower and noisier; kept for one realistic test (`conftest_cannot_read_a_denied_file`) |
| Call sandbox functions directly from Rust tests | Discarded | Would skip the gateway, the log and the CLI, which are part of what is being tested |

**What this means for the code.** `tests/security/main.rs`, 25 cases in 7 groups. Canary strings (fake secrets) are searched for in the whole execution folder afterwards. The shipped policies do **not** allow bare `python3`.

**Evidence.** `systemd-run --user --scope -p Delegate=yes cargo test --test security` → 25 passed, 0 failed.

**Concepts to learn.** Canary values; negative testing (why "the attack printed nothing" is not a pass).

### D-052 — A test crash is SIGKILL to itself, chosen by environment variables
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** To test recovery, the runtime can be told to kill itself at a named point. `AGENTOS_CRASH_AT=P4` picks the point, `AGENTOS_CRASH_CALL=6` picks the tool call, `AGENTOS_CRASH_HIT=2` picks the second time the point is reached. For points in the middle of an effect (P3, P5), a timer thread kills the process after a random delay up to `AGENTOS_CRASH_DELAY_US`. The kill is always SIGKILL, the same signal as `kill -9`.

**The problem.** The v1 document says the runtime "calls `abort()`" at the point. `abort()` raises SIGABRT, which makes the system write a core dump; the full suite kills the runtime about 5,000 times.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `kill(getpid(), SIGKILL)` at every point | Taken | Exactly what `kill -9` does: no cleanup code runs, no core dump |
| `abort()` as the v1 text says | Discarded | Thousands of core dumps; SIGABRT is not what the claim ("survives `kill -9`") is about |
| The harness kills from outside after a delay | Discarded for the named points | Cannot hit "after the intent, before the effect" reliably; kept in spirit for P3 and P5 through the timer thread |
| Compile the hooks only into test builds (`cfg`) | Discarded | The tests must run the same binary that ships; the hooks cost one environment lookup |

**What this means for the code.** `src/faults.rs`: `point(name, call)` and `arm(name, call)`. `gateway.rs` has P1–P7, `recovery.rs` and `checkpoint::restore` have P8 (five places). The v1 document's sentence is updated. With no variable set the functions do nothing.

**Evidence.** In the harness runs, killed processes ended with signal 9 (the harness checks `status.signal() == Some(9)`).

**Concepts to learn.** Signals (SIGKILL, SIGABRT); core dumps; fault injection.

### D-053 — Rolling back: copy the checkpoint, then swap folders atomically
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** durability
- **ADR:** 0003

**In plain words.** To put the workspace back to a save point, the save point is copied to a temporary folder next to the workspace, and then the two folders are swapped in one kernel call. If the runtime dies before the swap, the workspace is untouched; if it dies after, the workspace is fully restored. There is no moment at which it is half of each.

**The problem.** Recovery can be killed too (crash point P8). A rollback done as "delete the workspace, then copy the checkpoint in" would leave an empty or half-filled workspace if killed in between.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Copy to `work.restore-tmp`, `renameat2(RENAME_EXCHANGE)`, delete the old tree | Taken | One atomic step; repeating the whole thing after a crash gives the same result |
| Delete `work/`, copy the checkpoint to `work/` | Discarded | A kill in the middle leaves a broken workspace |
| Rename `work` away, rename the copy in (two renames) | Discarded | A kill between the two leaves no `work/` at all |
| `rsync`-style in-place sync | Discarded | Many small steps; any of them can be interrupted |

**What this means for the code.** `checkpoint::restore`. Recovery deletes a leftover `work.restore-tmp` first. The verdict record (`tool.reconciled`) is written only after the restore finished, so an interrupted recovery simply runs again. `RENAME_EXCHANGE` needs Linux 3.15 and a filesystem that supports it (btrfs, ext4, XFS and tmpfs do).

**Evidence.** Harness cells `P8 fs.write` and `P8 exec`: recovery killed at a random one of five points, then run again; 0 violations (numbers in D-057).

**Concepts to learn.** `renameat2` and `RENAME_EXCHANGE`; idempotent operations; re-entrant recovery.

### D-054 — A checkpoint after every mutating call; unnamed checkpoints are deleted
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** durability
- **ADR:** 0002

**In plain words.** After every `fs.write` and every `exec` that ran, the workspace is copied to `checkpoints/cp-0001`, `cp-0002`, … and the COMPLETED record names that copy. A copy that exists on disk but is not named in the log (the runtime died between making it and logging it) does not count, and recovery deletes it.

**The problem.** D-011 fixed the order (effect, checkpoint, COMPLETED). What was open: whether a command that changed nothing still gets a checkpoint, how checkpoints are numbered, and what is and is not synced to disk.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Always checkpoint after a mutating call, changed or not | Taken | One rule; "workspace equals the last checkpoint between calls" always holds, which recovery and the post-exec revert both rely on |
| Skip the copy when nothing changed | Discarded | Needs a full comparison to know, which costs about as much as a reflink copy |
| Number = last named number + 1 | Taken | A deleted orphan's number is reused, so names stay dense and the log alone says what the next name is |
| fsync every file of the checkpoint | Discarded | Only matters for power loss, which v1 does not claim (D-022); it would dominate the cost |

**What this means for the code.** `gateway.rs` (P4/P5/P6 around `checkpoint::create`), `log::named_checkpoints`. A failed mutating call (`tool.failed`) gets no checkpoint: `fs.write` fails before its rename, and `exec` fails only if the sandbox could not start, so neither changed the workspace. Old checkpoints are never pruned in v1 (cheap with reflinks; a known disk cost on ext4).

**Evidence.** Seeded-bug check: with orphan deletion removed from recovery, the harness reported `recover failed: checkpoint cp-0001 already exists` and leftover checkpoints (21 violations in 96 runs).

**Concepts to learn.** Reflink copies; orphaned state; why two stores need an agreed "source of truth".

### D-055 — Recovery kills what the dead runtime left running
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** If the runtime is killed while a command runs in the sandbox, that command must be gone before recovery repairs the workspace, or it could keep writing. Two things make sure: the sandbox processes are set to die when the runtime dies, and recovery also kills the command's cgroup by the path stored in the intent, then waits until it is empty.

**The problem.** "Die with the parent" (`PR_SET_PDEATHSIG`) takes effect a moment after the kill, and recovery can start within milliseconds.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Both: `PR_SET_PDEATHSIG` in the helper, and `cgroup.kill` + wait in recovery | Taken | The second closes the timing gap of the first |
| `PR_SET_PDEATHSIG` only | Discarded | A race between the dying command and the rollback |
| Record process ids in the log and kill those | Discarded | Ids are reused; a cgroup is not |

**What this means for the code.** The `exec` intent stores the cgroup's full path (the dead runtime lived in a different systemd scope than the recovering one, so the name alone is not enough). `recovery::kill_leftover_cgroup`: a missing folder means nothing is running, because a cgroup cannot be removed while it holds processes.

**Evidence.** Harness cell `P3 exec` (killed while the command runs): 0 violations, 0 duplicated effects (numbers in D-057).

**Concepts to learn.** Orphaned processes; why a cgroup is a reliable handle on "everything this command started".

### D-056 — Resuming: `run --resume`, replaying results to place the agent
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** durability
- **ADR:** 0005

**In plain words.** `agentos recover <id>` only repairs. `agentos run --resume <id>` repairs and then lets the agent continue. To know where the agent was, the runtime reads from the log the result of every earlier call, including the verdicts recovery wrote, and feeds them to the agent again. The scripted agent's script is saved in the execution folder at the start so it can be reloaded.

**The problem.** The CLI has four commands and none is "resume". And the agent's position exists only in memory.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `--resume <id>` on `run`; `recover` stays repair-only | Taken | No fifth command; `recover` is safe to run to inspect a crash without continuing |
| `recover` always continues the agent | Discarded | Sometimes one wants the verdict and the repaired workspace without more calls |
| Save the agent's position in a side file | Discarded | A second source of truth that can disagree with the log |

**What this means for the code.** `Agent::persist` and `Agent::restore`; `runtime::resume`; `recovery::history`. New log records: `recovery.started` (may appear several times if recovery was interrupted), `tool.reconciled` (`applied`, `reason`, the checkpoint taken or the checkpoint restored to), `recovery.completed`. A reconciled APPLIED call counts as done for the agent; NOT APPLIED makes the scripted agent ask for the step again. The resumed run uses the policy copy stored with the execution, not the file on disk now.

**Evidence.** Manual run: killed at P4 on call #6, `recover` printed `#6 fs.write APPLIED: file has the new content; checkpoint cp-0002 taken`, `run --resume` ran call #7 and completed. Unit test `restore_finds_the_position_again`. Every harness trial resumes to completion.

**Concepts to learn.** Event sourcing (state rebuilt from the log); synthetic tool results.

### D-057 — How the fault-injection harness decides pass or fail
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** The harness runs a seven-step script that uses all four tools, kills the runtime at a chosen point, and then checks the result against things it works out *independently of the runtime's own opinion*: it reads the files left on disk to decide whether the cut-off write really happened, and it adds up the applied calls in the log to predict what the workspace must contain. Each `exec` step appends one line to two files, so an effect that ran twice shows up as an extra line.

**The problem.** A harness that asks the runtime "did you recover correctly?" proves nothing. It also must be shown able to fail.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Independent oracle: file hashes before recovery, a model built from the log, a final-state check after resuming | Taken | Catches wrong verdicts, missing rollbacks, doubled effects and leftovers |
| Only check that `recover` exits 0 and the log parses | Discarded | Would pass with a recovery that does nothing |
| P4–P6 for read-only tools too | Discarded | Those points surround the checkpoint, which reads do not take; the cells would test nothing |

**What this means for the code.** `tests/recovery/main.rs`. Cells: P1, P2, P3, P7 × four tools; P4, P5, P6, P8 × two mutating tools; 24 cells. For P8 the run is first killed at a random point P2–P6, then recovery is killed at a random one of its five P8 points, then recovery runs again. Default is 3 runs per cell so `cargo test` is quick; `make eval-recovery` uses 200. The report is printed and written to `target/recovery-report.json`. The test also fails if fewer than half the runs were actually killed.

**Evidence.** `AGENTOS_RECOVERY_RUNS=200 systemd-run --user --scope -p Delegate=yes cargo test --test recovery -- --nocapture` (what `make eval-recovery` runs) on 2026-11-12, `nix` preset, debug build, 8 workers: 4,800 runs, 4,788 killed, 0 invariant violations, 0 duplicated effects, 3,866 of 3,866 cut-off calls classified correctly; `agentos recover` wall time p50 19.1 ms, p99 49.8 ms; 492 s. Full table: `docs/results/recovery-200-nix-m3.md`. Mutation check with 4 runs per cell: seeding "no rollback for exec" gave 31 duplicated effects in 96 runs; seeding "keep orphan checkpoints" gave 21 violations in 96 runs; the restored code gave 0.

**Concepts to learn.** Test oracles; mutation testing (seeding a bug to check the test); percentiles.

### D-058 — The demo LLM is a local llama.cpp server; no hosted API, no API key
- **Date:** 2026-11-12
- **Status:** Taken (answers the open question "which LLM for the demo")
- **Area:** tooling
- **ADR:** none

**In plain words.** The real-model run uses a model file that was already on the development machine (Qwen3-VL-4B-Instruct, GGUF, 3.7 GB) served by a `llama-server` that was already in the Nix store, on the laptop's own graphics card. Nothing was downloaded and nothing left the machine. The agent code itself talks to any OpenAI-compatible address, so a hosted API works by setting three environment variables.

**The problem.** The exit criterion needs a real LLM. An `NVIDIA_NIM_API_KEY` was present in the shell environment, but Prince had not given it for this project, and the download speed (about 30 KB/s) ruled out fetching a better model.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Local llama.cpp with the model already on disk | Taken | Free, private, repeatable; listed first in the v1 table |
| The hosted NVIDIA NIM endpoint with the key found in the environment | Discarded | The key was not handed over for this; using someone's credential and sending requests on their account is their decision |
| Download a stronger coding model | Discarded | Several GB at 30 KB/s is many hours |
| Mark M4 blocked and stop | Not needed | The local model completed the task once the agent loop was hardened (D-060) |

**What this means for the code.** `AGENTOS_LLM_BASE_URL` (default `http://127.0.0.1:8080/v1`), `AGENTOS_LLM_MODEL`, `AGENTOS_LLM_API_KEY` (optional). The key is sent as a header and is never written to the log, `llm.json` or anywhere else (a test greps for it). The model is small and is a vision model used for text; it is the weakest model the demo is likely to meet, which made the agent loop's weak points visible.

**Evidence.** `llama-server -m Qwen3-VL-4B-Instruct-UD-Q6_K_XL.gguf --jinja -ngl 99 -c 8192` answered `/health` with `{"status":"ok"}`; `nvidia-smi` showed 5,059 MiB in use. Four consecutive runs of the demo task completed (5 calls each, final `pytest -q` exit 0).

**Concepts to learn.** OpenAI-compatible chat API; GGUF and llama.cpp; quantization (Q6); why credentials are not borrowed.

### D-059 — The LLM agent: native tool calls, one per reply, every message an event
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** tooling
- **ADR:** 0005

**In plain words.** The model is offered four functions (`fs_list`, `fs_read`, `fs_write`, `exec`) through the API's tool-calling feature. Each reply may use one. The runtime turns it into a normal `ToolCall` and sends it through the gateway, then gives the result back as a `tool` message. A reply with no tool call ends the run. Every message, in order, is written to the event log before the call it leads to.

**The problem.** The agent must (a) never act by itself, (b) be rebuildable after a crash from the log alone.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| API tool calls (`tools` + `tool_calls`), first call only | Taken | Structured arguments, no parsing of free text; one call per step keeps "one intent at a time" |
| Ask the model to print JSON in its text and parse it | Discarded | Fragile; every model formats differently |
| Run several tool calls from one reply | Discarded | The gateway handles one call at a time; parallel calls would need a scheduler (cut from v1) |
| Let the agent write to the log itself | Discarded | The agent has no access to anything; it hands messages to the runtime (`take_messages`), which appends them |

**What this means for the code.** `src/agent/llm.rs`. Function names use `_` because the API forbids `.`. A malformed call (unknown tool, missing argument) is answered with an error message to the model, up to three times. On restart, `restore` rebuilds the message list from `agent.message` events and looks at how it ends: an assistant call that reached the gateway waits for its result (recovery's verdict, if it was cut off); an assistant call that never reached the gateway is dropped and the model is asked again; a trailing tool result means the request to the model was cut off, and it is simply sent again (D-017). Results of completed calls are read back from the blob store. `ureq` 3 (blocking HTTP, listed in the stack table) with its `json` feature is the client.

**Evidence.** `tests/llm_agent.rs` (mock endpoint): message roles in the log are `system, user, (assistant, tool) × 5, assistant`; the first assistant message precedes the first `tool.requested`; after a kill at P4 the first request after restart carries four tool results and ends with the result for `call_4`; the write is not repeated.

**Concepts to learn.** Function calling; chat roles (system, user, assistant, tool); event sourcing.

### D-060 — Guard rails in the agent loop for weak models
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** Four small protections were added after the first three real-model runs failed. (1) A reply is capped at 4,096 tokens. (2) If the model makes the same read-only call again and again, the result says so from the second time, and the run stops at the fifth. (3) If a write leaves out four or more lines the model had read in that file, the result warns and names some of them. (4) A run that gives up is recorded as `stopped`, not `completed`, and the stored summary is capped at 600 characters.

**The problem.** The 4B model fixed the bug but sent only the changed function to `fs_write`, deleting the other function; it then insisted the function was still there. In run 1 it listed two folders alternately until the 50-call budget was gone; in run 2 it repeated one paragraph of text until its context was full, and that text became the run's "summary"; in run 3 it re-ran pytest five times.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Generic guard rails in the loop (the four above) | Taken | None mentions the demo task; each addresses a failure seen in a real run and helps any model |
| A patch/edit tool instead of whole-file writes | Discarded | A fifth tool; the tool list is fixed in v1. Added to the parking lot |
| Hints specific to the demo ("keep `median`") | Discarded | That would be faking the result |
| Only a stronger model | Discarded | Not available offline (D-058), and a loop that burns its budget on repeats is a defect with any model |

**What this means for the code.** `LlmAgent::track_file_content`, `note_call`/`repeat_count`, `MAX_REPLY_TOKENS`, `Action::GiveUp`. The warning text is plain information added to the tool result; the model still decides what to do. The system prompt states the whole-file rule as a numbered rule.

**Evidence.** Runs 1–3 failed as described (traces in `docs/results/llm-failed-runs-qwen3-vl-4b.md`). With the guard rails, run 4 and three repeats completed: partial write at call #3, warning, full write at #4, `pytest -q` exit 0 at #5. Unit tests `a_write_that_drops_lines_gets_a_warning`, `repeated_calls_are_counted_until_a_write`.

**Concepts to learn.** Context window; `max_tokens`; why temperature 0 makes a stuck model stay stuck.

### D-061 — How the three overhead numbers are measured
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** Three costs are measured, each against doing the same thing without AgentOS: starting a command in the sandbox, one 4 KB file write through the gateway, and the whole demo task. One script (`bench/run.sh`, or `make bench`) runs all three, writes every raw sample to `docs/results/bench-<preset>/`, records the machine, and builds the summary table from those files.

**The problem.** The v1 design names the three numbers and the tools (hyperfine, criterion) but not what exactly is timed or what "direct execution" means for each. A number without a stated baseline is not a result.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Spawn: a small example program (`examples/sandbox_spawn.rs`) that uses the library; in-process timing of 500 spawns, plus hyperfine on the whole program | Taken | In-process timing is the spawn alone; hyperfine (which the plan names) can only time whole programs, so it also counts program start and the startup checks. Both are reported and labelled |
| Spawn: a hidden `agentos` subcommand for hyperfine | Discarded | Adds CLI surface for a benchmark |
| `fs.write`: criterion around `gateway.handle`, with two baselines: the same durable replace (temp file, fsync, rename) and a plain write | Taken | The durable baseline is the fair one; the plain one shows what durability itself costs |
| `fs.write`: only against a plain write | Discarded | Would blame AgentOS for the cost of fsync, which any durable writer pays |
| End to end: `agentos run` with the scripted demo against a shell script doing the same steps on a copy of the repo | Taken | Same work on both sides, including the copy |
| Percentiles from criterion's summary | Discarded | criterion reports a mean with a confidence interval, not p95/p99; each sample's time is recorded by the benchmark itself and percentiles are computed from that |

**What this means for the code.** `runtime::create` was split out of `runtime::start` so a benchmark can get a real gateway. `criterion` is a dev-dependency with default features off (no plotting). Release build. The numbers are from one laptop on battery-or-mains unknown, with a desktop session running; they are an order of magnitude, not a guarantee.

**Evidence.** `docs/results/bench-nix/summary.md` and the raw files next to it (see the Findings row of 2026-11-12 for the values).

**Concepts to learn.** Percentiles (p50, p95, p99) and why the mean hides slow cases; baselines; warm-up; why fsync dominates small writes.

### D-062 — The TOCTOU test uses an attacker stronger than the threat model
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** The race test needs something that changes a symlink at the same moment the runtime opens a path through it. Inside AgentOS that cannot happen: the agent makes one call at a time, and every process a command started is dead before the call returns. So the test cheats in the attacker's favour: a thread of the test program, running on the host, re-points a symlink in the workspace thousands of times while the scripted agent makes 300 writes and reads through it.

**The problem.** The v1 design asks for "a TOCTOU loop that swaps a symlink while the agent writes". A test where the swap can never coincide with the open would pass trivially.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Host thread re-pointing `src/swap` (a symlink) between an inside folder and an outside folder, by atomic rename of a fresh link | Taken | Exercises the real race; the workspace stays valid at every instant |
| Exchanging two names inside the workspace (`RENAME_EXCHANGE`) | Discarded | Tried first. A checkpoint copied mid-exchange captured both names as symlinks; after one rollback the real folder was gone and nearly every call was refused, so the "race" was no longer one |
| A sandboxed background process doing the swaps | Discarded | Impossible by design: it is killed with its cgroup when its call ends |

**What this means for the code.** `toctou_symlink_swap_during_writes_and_reads_never_escapes`. The host thread also races the runtime's own checkpoint copy, which can make the runtime stop with an error; the test then resumes the execution until all steps are settled. It asserts: nothing was created in the outside folder, the outside file is unchanged, its content appears in no tool result and not in the log, more than 1,000 swaps happened, and at least 20 calls ended each way (inside, and refused as leaving the workspace).

**Evidence.** Three consecutive runs: 6,425 / 6,747 / 6,396 swaps; 130 / 148 / 153 calls completed inside; 169 / 151 / 146 refused; 0 escapes.

**Concepts to learn.** TOCTOU; why "the test passed" needs "and the dangerous case actually occurred".

### D-063 — The benign suite: 23 ordinary operations under the shipped policy
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** policy
- **ADR:** none

**In plain words.** A sandbox that refuses everything has no escapes either. The benign suite runs the things a well-behaved agent does (list, read, write source and test files, run pytest three ways, use symlinks that stay inside, names with spaces and accents, `..` that stays inside) under the policy exactly as shipped, and requires that none is denied, none fails, and nothing is reverted.

**The problem.** The v1 pass criteria are "0 escapes" and "0 false denials"; the second needs its own test.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| One scripted run of 23 named steps; each must reach COMPLETED | Taken | A failure names the step and the rule |
| Count the vertical-slice demo as the benign suite | Discarded | Seven steps, one of them an intended denial |

**What this means for the code.** `tests/benign.rs`. `make eval-security` runs the adversarial and the benign suite together.

**Evidence.** `benign: 23 operations, 0 false denials, 0 failures, 0 reverts`.

**Concepts to learn.** False positives versus false negatives in a security control.

### D-064 — `fs.read` only reads regular files
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** sandbox
- **ADR:** none

**In plain words.** `fs.read` now refuses anything that is not an ordinary file: named pipes (FIFOs), sockets, device files. It opens with a flag (`O_NONBLOCK`) that makes the open return at once instead of waiting.

**The problem.** Bug B-005: a command could create a FIFO in the workspace, and a later `fs.read` of it blocked the runtime forever.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `O_NONBLOCK` on open, then refuse non-regular files | Taken | No waiting at any point; the agent gets a clear failure |
| A timeout thread around every file read | Discarded | More machinery, and a blocked `open` cannot be cancelled cleanly |
| Remove special files during the post-exec revert | Discarded | They can be inside the write globs, where changes are allowed |

**What this means for the code.** `tools/fs.rs::open_for_read` and `read`. `fs.write` already refused to replace non-regular files (D-047); recovery's hash check already used `O_NONBLOCK`.

**Evidence.** The new test timed out at 20 s before the fix and passes after it; the two calls are logged as FAILED with "not a regular file".

**Concepts to learn.** FIFOs (named pipes); blocking `open`; availability as a security property.

### D-065 — A CI workflow for the `fhs` preset, written but never run
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** `.github/workflows/ci.yml` tells GitHub to run the linter, all tests and the 200-kill fault-injection suite on Ubuntu 24.04 with the `fhs` policy. It has never run, because the agent does not push. It will probably need fixing on its first run.

**The problem.** CLAUDE.md says the `fhs` preset is tested in CI on an Ubuntu runner. Without a workflow file, a push would test nothing.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Write the workflow now, mark it unverified in the file and in the milestone documents | Taken | A push then gives a real answer; nobody is told it passes |
| Leave CI for Prince | Discarded | The portability claim would have no way to be checked |
| Test `fhs` locally in a container or VM | Discarded | No container tool is installed, and downloading an Ubuntu image over this link is impractical |

**What this means for the code.** The uncertain part is cgroup delegation on a GitHub runner: the workflow enables a systemd user session for the runner user (`loginctl enable-linger`, `XDG_RUNTIME_DIR`, `DBUS_SESSION_BUS_ADDRESS`) so `systemd-run --user` works, and turns off Ubuntu's AppArmor restriction on user namespaces. If that does not work, the suites will fail at startup with the runtime's own "cannot run the sandbox" message.

**Evidence.** None; the workflow has not run.

**Concepts to learn.** Continuous integration; GitHub Actions; lingering user sessions in systemd.

### D-066 — `make demo`: one script that shows the fix, the crash and the recovery
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** `./demo.sh` (or `make demo`) builds the program, shows the failing test, lets the scripted agent fix it in the sandbox, prints the patch, then runs the task again with a deliberate kill after the file write, and shows `recover` and `run --resume`. It puts itself under a delegated cgroup and picks the policy preset for the system it runs on.

**The problem.** The M6 exit criterion is that a stranger can clone and run the demo in under ten minutes. Five commands with a `systemd-run` prefix and a policy path are five chances to get it wrong.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| One shell script, also reachable as `make demo` | Taken | One command after cloning; the script is short enough to read first |
| A list of commands in the README | Discarded | Easy to mistype; the delegated-cgroup prefix is the usual stumbling block |
| A new `agentos demo` subcommand | Discarded | CLI surface is fixed at four commands |

**What this means for the code.** `demo.sh`: re-runs itself under `systemd-run --user --scope -p Delegate=yes` when needed; preset is `nix` if `/nix/store` exists and `/usr/bin/python3` does not, else `fhs` (override with `AGENTOS_TEST_PRESET`). The preset choice lives in the script, not in `src/`, which must not know about distros.

**Evidence.** `docs/demo-transcript.md` is the script's unedited output on the development machine.

**Concepts to learn.** Shell `exec`; why a demo should be one command.

### D-067 — Execution ids carry milliseconds
- **Date:** 2026-11-12
- **Status:** Taken (replaces the id format in D-036)
- **Area:** durability
- **ADR:** none

**In plain words.** An execution id was `YYYYMMDD-HHMMSS-xxxxxx`: the time to the second, then six random-looking characters. Two runs started in the same second were therefore ordered by those characters, that is, at random. The id is now `YYYYMMDD-HHMMSSmmm-xxxxxx`, with milliseconds.

**The problem.** Bug B-006: `agentos trace` and `status` with no id, and the first version of the demo script, take "the latest execution" to be the last id in sorted order. The demo starts two runs back to back; in its first full run the "latest" was the wrong one, and the recover and resume steps were applied to the run that had already finished.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Milliseconds in the id | Taken | Smallest change; ids still sort as text; two runs in the same millisecond are not something a person or the demo produces |
| A counter file in the state folder | Discarded | Needs a lock shared by all runs |
| Find the latest by the first event's timestamp | Discarded | Opens every log just to list executions |

**What this means for the code.** `workspace::new_execution_id`; its unit test now checks that two ids made 3 ms apart sort in order. The demo script no longer depends on ordering at all. D-036's main decision (one folder per execution) is unchanged. Old executions with the short id format still open; they sort before new ones made on the same day.

**Evidence.** Before: demo steps 5–7 printed "already finished; nothing to recover" for the wrong id. After: `RECOVERED … #6 fs.write APPLIED`, then `RESUMING after 6 calls` (`docs/demo-transcript.md`).

**Concepts to learn.** Why "sorts by time" needs enough time resolution; lexicographic order.

### D-068 — What v0.1 ships, and what is left for Prince
- **Date:** 2026-11-12
- **Status:** Taken (the "no license" part is replaced by D-069)
- **Area:** process
- **ADR:** none

**In plain words.** v0.1 is tagged locally with the code, the three suites passing on NixOS, a README, a threat model, a recovery-semantics document, eleven ADRs and a demo transcript. Three things are deliberately left undone because they are Prince's to do or decide: pushing (which is also what lets the Ubuntu tests run), choosing a license, and deciding the project's name.

**The problem.** The v1 "definition of done" has six boxes. One of them, "both suites pass with the `fhs` and the `nix` preset", cannot be checked by an agent that does not push. The question was whether to tag v0.1 anyway.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Tag `v0.1` locally; state the open box in the README, the M6 document, the v1 document and here | Taken | CLAUDE.md says to tag `v0.1` at the end and to record `fhs` as "not yet verified", never as passing. The definition of done gates putting the project on the resume, and that gate stays closed until CI is green |
| Do not tag until CI has run | Discarded | The agent cannot make CI run; the work would end without its last marker |
| Choose MIT or Apache-2.0 so the repository has a license | Discarded | A license is a legal choice about Prince's work; it is listed under "Still open" for him |
| Rename the project | Discarded | D-023 leaves the name open; nothing forces a choice now |
| Record the demo as a video or asciinema cast | Discarded | No recorder is installed; a text transcript of the real output is checkable and searchable |

**What this means for the code.** No `LICENSE` file. The README says "No license has been chosen yet" and "Not yet verified: any other Linux distribution". The "Bonus" (a fake external tool) has not been started.

**Evidence.** Fresh clone on the development machine: `nix develop -c make demo` finished in 23 seconds with crates and the Nix shell already on disk. `make test` and `make lint` pass. `docs/results/` holds the suites' results of record. Nothing was run on Ubuntu.

**Concepts to learn.** Software licenses (MIT, Apache-2.0); what a release tag means.

### D-069 — License: MIT or Apache-2.0, at the user's choice
- **Date:** 2026-11-12
- **Status:** Taken (closes the "License" row of "Still open"; replaces the "no license" part of D-068)
- **Area:** process
- **ADR:** none

**In plain words.** A license is the text that tells other people what they may do with the code. Without one, the law says "nothing", even if the repository is public. AgentOS now carries two licenses, and anyone using the code picks whichever suits them: MIT (very short: do what you like, keep the copyright notice) or Apache-2.0 (longer: the same freedom, plus an explicit permission to use any patents the authors hold on the code). Prince chose this on 2026-11-12.

**The problem.** D-068 left the repository without a license because it is a legal choice about Prince's work. The repository is now public on GitHub, and a public repository with no license cannot legally be reused by anyone.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Dual `MIT OR Apache-2.0` | Taken | Prince's decision, on Claude's recommendation. It is what the Rust compiler and most Rust crates use, so the code can be combined with almost anything; Apache-2.0 gives the patent grant, MIT keeps it compatible with GPLv2-only projects, which Apache-2.0 alone is not |
| MIT only | Discarded | Simplest, but has no patent grant |
| Apache-2.0 only | Discarded | Has the patent grant, but cannot be combined with GPLv2-only code |
| A copyleft license (GPL) | Discarded | Not asked for; makes reuse harder for a portfolio project meant to be read and borrowed from |
| No license | Discarded | Was the state at v0.1; nobody may legally reuse the code |

**What this means for the code.** Two files at the top of the repository, `LICENSE-MIT` (copyright line "2026 Patel Prince", the name in the git history) and `LICENSE-APACHE` (the unmodified Apache-2.0 text). `Cargo.toml` has `license = "MIT OR Apache-2.0"`, the machine-readable form (an SPDX expression). The README has a "License" section. No per-file headers: neither license requires them. The tag `v0.1` points at a commit without a license; the license applies from the commit that adds it.

**Evidence.** `LICENSE-APACHE` was copied from the `anyhow` 1.0.104 crate in the local cargo cache (176 lines, ends at "END OF TERMS AND CONDITIONS"); it is the standard text with no project-specific lines. `nix develop -c cargo build` accepts the manifest. The compatibility statements are general knowledge, not legal advice and not checked for this entry.

**Concepts to learn.** Software licenses; permissive versus copyleft; patent grant; SPDX license expressions; dual licensing.

### D-070 — The checkpoint copy dies with the runtime
- **Date:** 2026-11-12
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** A checkpoint is copied by a helper program, `cp`. When the runtime was killed, that helper kept running by itself for a moment and could create a half-made checkpoint folder after recovery had already tidied up. Now the helper asks the kernel: "if the program that started me dies, kill me too." This is the same request the sandbox helper has made since M2.

**The problem.** Bug B-007, found by the first CI run on Ubuntu: 1 leftover folder in 4,800 kills. Recovery assumes that once the runtime is dead, nothing of it is still writing. That was true for sandboxed commands (D-055) but not for the `cp` child.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `PR_SET_PDEATHSIG(SIGKILL)` in the `cp` child, plus a check that the parent is still the runtime | Taken | Smallest change (one function); the kernel queues the kill while the runtime is exiting, before anyone can see that it died |
| Copy the folder inside the runtime instead of starting `cp` | Discarded | No child, so no orphan, but it means writing a recursive copy with reflink support by hand: a large change to fix a small window. The v1 stack table says to shell out to `cp` |
| Let `cp` inherit the execution's lock file, so recovery refuses to start while a copy lives | Discarded | A hard guarantee, but recovery would then fail with "in use" in exactly the moment after a crash, and would need a wait-and-retry loop |
| Make recovery wait a little, or delete leftovers twice | Discarded | Guessing at a delay; the orphan could still write later |
| Loosen the harness's "no leftovers" check | Discarded | The check was right; weakening a test to pass is not allowed |

**What this means for the code.** `checkpoint::run_cp` uses `pre_exec` (code that runs in the child between `fork` and `exec`) to call `prctl` and `getppid`. It is an `unsafe` block with a comment on what it relies on. One detail worth knowing: the kernel ties the death signal to the *thread* that started the child, not the process; checkpoints are made on the main thread. What remains: a child that is in the middle of one system call when the kill arrives finishes that call first. That is one `mkdir` or one write, not a copy.

**Evidence.** Before the fix, `checkpoint_copy_dies_with_the_runtime` (a `cp` wrapper that sleeps 0.4 s) failed with `leftover …: checkpoints on disk [".tmp-cp-0001", "cp-0000", "cp-0001"]`, the same shape as the CI failure. After the fix it passes. CI evidence: `docs/results/recovery-200-fhs-run1-failed.md`. The full local runs after the fix are recorded in the Findings row of 2026-11-12.

**Concepts to learn.** Orphan processes and re-parenting; `PR_SET_PDEATHSIG`; `fork`/`exec` and `pre_exec`; async-signal-safety.

### D-071 — Taking the execution lock waits up to three seconds
- **Date:** 2026-11-13
- **Status:** Taken (adds to D-039, which stays)
- **Area:** durability
- **ADR:** none

**In plain words.** Only one `agentos` process may work on an execution, and a lock file enforces that. After a kill, the lock should be free at once. It turned out that a child process of the dead runtime can hold a copy of the lock for a few thousandths of a second more. So when the lock is taken, "someone else has it" is now tried again for up to three seconds before it is reported as an error.

**The problem.** Bug B-008, found by the second CI run: 3 of 4,800 recoveries were refused with "in use by another agentos process". The fix for B-007 (D-070) had made the gap between `fork` and `exec` longer, and in that gap a child holds a copy of every open file of its parent, the lock file included.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Retry the lock for a bounded time (3 s, every 5 ms) | Taken | Covers every child the runtime starts (`cp`, the sandbox helper, `systemctl`, `diff`), not just one; a few lines in one function; a runtime that is really running is still detected |
| Take D-070 back | Discarded | Brings B-007 back |
| Start `cp` through a small hidden helper mode of the `agentos` binary that sets the death signal and then runs `cp`, so the fast start path is kept | Discarded | Narrows the gap again but does not remove it, and needs the binary's path passed into checkpoint and recovery code |
| A POSIX record lock (`fcntl`) instead of `flock`; those are not copied to children | Discarded | Removes the cause, but such a lock is dropped when the process closes *any* handle on the file, a well-known trap; a bigger change to D-039 than the problem needs |
| Copy folders inside the runtime (no child process) | Discarded | Same reason as in D-070: a large change |
| Have the harness wait before running `recover` | Discarded | That would hide the bug from the test, and a person running `agentos recover` right after a crash would still hit it |

**What this means for the code.** `runtime::lock_execution`, used by `recover`, `run --resume` and new runs. Trying to recover an execution that is really still running now takes three seconds to fail instead of failing at once. `is_locked` (used by `status` to say "running" or "interrupted") does not wait, so `status` can say "running" for an instant after a kill. D-039's choice of `flock` is unchanged.

**Evidence.** CI run 43375633929 (`docs/results/recovery-200-fhs-run2-failed.md`). Local stress with 64 workers: 4 violations in 1,440 runs before this change, 0 in 2,880 at tag `v0.1`, 0 in 2,880 after it (`docs/results/b008-stress.md`). Test `recovery_waits_for_a_lock_that_is_about_to_be_released`: a lock held throughout is still refused, after at least 2 s and without any recovery record being written; a lock released after 0.4 s is waited for.

**Concepts to learn.** `fork` and inherited file descriptors; close-on-exec; `flock` versus `fcntl` locks; why Rust's `Command` has a fast path (`posix_spawn`) and when it cannot use it.

### D-072 — The `fhs` box is closed by one green CI run, and what that run covers
- **Date:** 2026-11-13
- **Status:** Taken (answers the open question "Does the `fhs` preset pass on Ubuntu?")
- **Area:** process
- **ADR:** 0011

**In plain words.** The last open item of the "definition of done" was: do the test suites also pass on an ordinary Linux (Ubuntu), not only on NixOS? They now do, in GitHub's CI. It took three runs. The first two failed, and each failure was a real bug in AgentOS, not a problem with the CI setup. The documents now say "verified on Ubuntu 24.04", name the exact run, and list what that run did not cover.

**The problem.** After two red runs and one green one, what may the documents claim? And which numbers go where?

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Tick the box on the first green run, cite its URL and commit everywhere, keep both failed reports next to it, and list what was not run | Taken | It is what CLAUDE.md and the v1 document ask for ("a run is green"); the failed runs stay visible so nobody reads one green run as more than it is |
| Require several green runs in a row before ticking | Discarded | A sensible extra, but not the written rule, and each run needs a push from Prince. Later pushes will add runs anyway |
| Say "works on Ubuntu, Debian, Fedora, Arch" because they share the preset | Discarded | Only Ubuntu 24.04 was run |
| Put the CI recovery times in the README's headline numbers | Discarded | A shared runner under load is not a benchmark; they are listed and labelled as such |
| Run the benchmarks and the demo in CI too | Discarded for now | Not part of the box as written; benchmark numbers from a shared runner would mislead. Noted as "not run on Ubuntu" |

**What this means for the code.** No code change. `docs/results/recovery-200-fhs.{md,json}` and `docs/results/security-fhs.md` are the `fhs` results of record. README (Status, results table), CLAUDE.md, the threat model, the recovery-semantics document, ADR 0011, the v1 definition of done and the comment at the top of `ci.yml` are updated. The old milestone documents keep their text and get a dated note. From now on a commit counts as verified on `fhs` only when a CI run on that commit is green.

**Evidence.** <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105>, commit `c3c9502`, conclusion `success`: lint; `make test` (22 unit, 4 vertical slice, 32 security, 1 benign test of 23 operations, 2 LLM mock, 3 recovery); `make eval-recovery`: 4,800 runs, 4,800 killed, 0 violations, 0 duplicated effects, 3,756/3,756 verdicts correct. The two failed runs: `docs/results/recovery-200-fhs-run1-failed.md`, `docs/results/recovery-200-fhs-run2-failed.md`. Not run on Ubuntu: `make bench`, `make demo`. The runner's kernel version and filesystem are not in the log.

**Concepts to learn.** Continuous integration; why one passing run of a randomized test is weaker evidence than it looks; flaky tests versus rare real bugs.

### D-073 — The bonus starts: one external tool, `ext.call`, as a fifth gateway tool
- **Date:** 2026-11-13
- **Status:** Taken (carries out D-020)
- **Area:** scope
- **ADR:** none

**In plain words.** Everything the agent could do in v0.1 happened inside the working folder, and a folder can be put back from a copy. The bonus adds the one thing a copy cannot undo: asking a service outside the runtime to do something. There is a new tool, `ext.call`, with two fields: the name of a service and a text payload. The gateway sends the payload to that service over HTTP. The service is a small fake written for this project. It is the "milestone 7" of the record, and it is not part of v0.1.

**The problem.** D-020 allowed the bonus once v0.1 was tagged, and the v1 document describes it in two sentences: "a small local service with injectable timeouts, called through the gateway with an idempotency key". Prince asked for it on 2026-11-12, to start after CI was green (it has been since run 43385661105, D-072). The shape of the tool had to be chosen, and so did how much of the rest of the system it may touch.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| A fifth tool `ext.call {service, payload}` that goes through the same gateway steps as the other four | Taken | The bonus says "called through the gateway". Same log records, same intent-before-effect rule, same fault points |
| Let `exec` reach the service (give the sandbox a network) | Discarded | Breaks "no network for commands", and an effect made by a command could not be given an idempotency key by the runtime |
| A general HTTP tool (method, URL, headers chosen by the agent) | Discarded | Far more than "one fake external tool"; the agent choosing URLs is a data-leak tool |
| Offer `ext.call` to the LLM agent as a fifth function | Discarded | The bonus is about recovery, which the scripted agent tests. The LLM's tool list stays at four; a model talked into calling an outside service is a different problem (human approval was cut from v1) |
| Allow it in the shipped policies | Discarded | No rule means deny. The shipped `fhs` and `nix` policies have no `[external]` section, so `ext.call` is refused unless someone writes a policy for it |

**What this means for the code.** `ToolCall::ExtCall`, a `[external]` policy section (D-074), `src/tools/ext.rs` for the HTTP request, a new branch in `gateway.rs` and in `recovery.rs`. No new dependency: `ureq` is already in the stack table for the LLM client. The CLI keeps its four commands. An `ext.call` does not change the workspace, so it takes no checkpoint. The v1 document's cut-list line "External tools … See Bonus" and its Bonus section get a status note; the five parts and the build order do not change.

**Evidence.** None; this entry fixes the plan. The entries after it carry the evidence.

**Concepts to learn.** Side effects that cannot be rolled back; why a tool boundary is also a security boundary.

### D-074 — External services are named in the policy and must be on `127.0.0.1`
- **Date:** 2026-11-13
- **Status:** Taken
- **Area:** policy
- **ADR:** none

**In plain words.** The agent never gives a web address. It says a name, such as `ledger`. The policy file has a list of names with their addresses, and a name that is not on the list is refused. Every address must start with `http://127.0.0.1:` and a port number, which means "a program on this same computer". A policy that names any other machine is refused when it is loaded.

**The problem.** v1 says "network of any kind" is cut, and the threat model lists network egress as something protected. A tool that makes HTTP requests could quietly end that. The bonus only needs a *local* fake service.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `[[external.service]]` with `name` and `url`; the URL must be `http://127.0.0.1:<port>[/path]`, checked at load | Taken | The runtime can still only talk to its own machine; a typo or a hostile policy edit cannot point it at the internet without the load failing |
| Any `http://` or `https://` URL in the policy | Discarded | That is a real external tool. It needs its own decision (TLS, credentials, what data may leave), not a bonus |
| Also accept `localhost` | Discarded | `localhost` is a name that is looked up; `/etc/hosts` or a resolver can make it mean another machine |
| The agent passes the URL, the policy has allowed prefixes | Discarded | Prefix rules on URLs are easy to get wrong (`http://127.0.0.1:80@evil.example/` has host `evil.example`) |

**What this means for the code.** `policy::ExternalRules { services, timeout_ms, attempts }`, with defaults 2000 ms and 3 attempts when the section leaves them out. `Policy::check_external(name)` returns the decision and the matching service. `is_loopback_url` is a text check; that is acceptable here because the policy file is trusted input, and it rejects the user-info trick above, `#`, spaces and a zero or missing port. The request itself is made with redirects off and with no proxy taken from the environment (D-075), so the loopback rule cannot be bypassed after the check.

**Evidence.** `policy::tests::external_services_are_denied_unless_listed` and `external_urls_must_be_loopback_by_number` (4 accepted forms, 11 refused forms, a duplicate name, zero limits, an unknown key) pass.

**Concepts to learn.** Loopback address; URL user-info (`user@host`); server-side request forgery (SSRF); why names need a lookup and numbers do not.

### D-075 — The fake service: a std-only HTTP server with three faults
- **Date:** 2026-11-13
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** The "outside service" is about 250 lines of plain Rust in `tests/common/fake_service.rs`. It listens on `127.0.0.1`, takes `POST /apply` with an `Idempotency-Key` header, remembers what it applied, and never applies the same key twice. It can be told to misbehave in three ways for chosen requests: answer late, lose the request (do nothing, never answer), or lose the answer (do it, never answer). The tests start it inside the test program; `examples/fake_service.rs` runs the same code as a program for trying things by hand.

**The problem.** The bonus asks for "a small local service with injectable timeouts". It must outlive the runtime (it is "outside"), the tests must be able to read what it really did without asking the runtime, and it must not add a dependency.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Hand-written HTTP on `std::net`, state in memory, one file used by the tests and by an example program | Taken | No dependency; the test process owns the service, so killing `agentos` never kills it, and the tests read its record directly as ground truth |
| A second binary in `src/bin/` | Discarded | It would ship as part of AgentOS; a test double belongs with the tests |
| A web framework or `tiny_http` as a dev-dependency | Discarded | A new dependency for four routes |
| A Python `http.server` script | Discarded | The harness could not read its state in-process, and CI timing would depend on Python start-up |
| Persist the service's state to disk and kill the service too | Discarded | Whether a *service* survives its own crash is the service's problem, not AgentOS's; the bonus is about the caller |
| Faults chosen by position ("the 2nd request fails") | Discarded | Faults are queued per payload instead, so a test can aim at one step of a script however many requests came before |

**What this means for the code.** `FakeService::start(port, hold)`, `inject(payload, &[Fault])`, `applied()`, `seen()`. `Fault::{DelayMs, DropBefore, DropAfter}`. A dropped request keeps its connection open for `hold` and then closes it, so the caller sees a timeout, not an error page. The check "was this key applied?" and the apply are one step under one lock; that is the whole idempotency promise. Extra rules that make refusals testable: a body starting with `reject` gets 422, and a reused key with a different body gets 409; neither applies anything. `GET /state` shows what was applied.

**Evidence.** `tests/external.rs`: `the_service_applies_each_key_once_and_replays_its_answer`, `refusals_are_definite_and_apply_nothing`, `a_lost_request_and_a_lost_answer_look_the_same_from_outside` pass.

**Concepts to learn.** Test doubles (fakes); HTTP request and response format; idempotency keys as used by payment APIs; fault injection on the *other* side of a call.

### D-076 — What an answer proves: 2xx applied, 4xx refused, everything else unknown
- **Date:** 2026-11-13
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** After sending a request, the runtime puts the result in one of three boxes. The service said yes (HTTP 200–299): it was done. The service said no (400–499): it was not done. Anything else, such as no answer in time, a broken connection, a server error, or nothing listening: the runtime does not know. "Does not know" is never recorded as "failed", because a request that timed out may still have been carried out.

**The problem.** The log's FAILED record means "nothing was changed" (D-040). A timeout cannot honestly be written as FAILED. A lost request and a lost answer are indistinguishable to the caller.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Three outcomes: applied / refused / unknown; only an HTTP status from the service counts as definite | Taken | One rule with no special cases; every "unknown" is handled the same way (send again with the same key, D-078) |
| Treat "connection refused" as definite failure, because nothing was sent | Discarded | True in theory, but it means reading error kinds out of the HTTP library, and a wrong guess turns "maybe done" into "not done". The cost of calling it unknown is only a retry |
| Treat HTTP 5xx as failure | Discarded | A server can fail after doing the work |
| Treat a timeout as failure and let the agent decide | Discarded | This is the bug the bonus exists to show: the agent retries, and the effect happens twice |

**What this means for the code.** `tools/ext.rs`: `Answer::{Applied, Refused, Unknown}`, `send`, and `send_until_answered` (same key every time, at most `attempts` requests). Redirects are never followed and a proxy from the environment is never used, so a request allowed for `127.0.0.1` cannot end up anywhere else; a redirect counts as unknown. The answer body is cut at `limits.max_output_kb`.

**Evidence.** `tests/external.rs`: `send_until_answered_retries_with_the_same_key_and_gives_up_as_unknown`, `nobody_listening_is_unknown_not_failed`, `a_redirect_is_not_followed` (a server that redirects to the fake service; the service sees no request), `long_answers_are_cut_at_the_limit` pass. The proxy setting is by reading ureq 3.5.1's `config.rs` (its default takes a proxy from the environment); it has no test.

**Concepts to learn.** The two-generals problem in its everyday form (a lost request and a lost answer look the same); HTTP status classes; why "unknown" is a real third state.

### D-077 — The idempotency key is the execution id plus the call id, written in the intent
- **Date:** 2026-11-13
- **Status:** Taken
- **Area:** durability
- **ADR:** 0012

**In plain words.** Every `ext.call` is sent with a label, the idempotency key, that tells the service "if you have seen this label before, do not do the work again". The label is `agentos-<execution id>-c<call number>`. It is not random: it is built from two things already in the diary. It is written into the intent record, with the service's address, the payload's fingerprint, the timeout and the number of attempts, before the first request leaves.

**The problem.** After a crash, recovery must send *exactly* the request the dead runtime sent or was about to send. If the two could disagree about the key, the service would see two different requests and do the work twice.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Key computed from execution id and call id; stored in the intent as well | Taken | Cannot differ between the first send and any later one; unique because ids are unique per state folder and call numbers are never reused; readable in the trace |
| A random key (UUID) stored in the intent | Discarded | Works, since the intent is durable before the request, but needs a randomness source (a new dependency or hand-rolled) and gives nothing the computed key lacks |
| A hash of the payload as the key | Discarded | Two deliberate, separate calls with the same payload ("charge 5 euros" twice) would be merged into one |
| Let the agent supply the key | Discarded | A hostile or confused agent could reuse or vary keys; the guarantee would depend on the least trusted part |

**What this means for the code.** `tools::ext::idempotency_key`. The intent's `plan` holds `service`, `url`, `key`, `payload_hash`, `timeout_ms`, `attempts`. The payload itself goes to the blob store before `tool.requested` is logged (as file content does for `fs.write`, ADR 0004), so the log holds only its hash and recovery can read it back. A step the agent asks for again after NOT APPLIED is a new call with a new number, hence a new key; that is safe only because NOT APPLIED is never said unless the service did nothing (D-078). Two different state folders on one machine could, in theory, produce the same execution id in the same millisecond with the same 6-character suffix; that is not defended against.

**Evidence.** `tests/external.rs::allowed_call_is_logged_keyed_and_applied_once`: the intent's key is `agentos-<id>-c1`, the service recorded that key, and the log's `tool.requested` has a `payload_hash` and no payload. `a_lost_answer_is_retried_with_the_same_key_and_applied_once`: two requests, one key, one effect.

**Concepts to learn.** Idempotency keys; why "the same request" has to be defined before the first send; deterministic versus random identifiers.

### D-078 — An uncertain `ext.call` is settled by sending it again; silence settles nothing
- **Date:** 2026-11-13
- **Status:** Taken (adds a rule to D-016 for the new tool)
- **Area:** durability
- **ADR:** 0012

**In plain words.** For files and commands, recovery can always fall back on "put the folder back as it was". A request that may have reached another program cannot be put back. So recovery goes forwards instead: it sends the same request again, with the same key. The service does the work only if it had not done it already. If the service answers "done", the call is APPLIED. If it answers "refused", NOT APPLIED. If it does not answer, recovery does not decide anything: it stops with an error, and the run stays stopped until recovery is tried again and the service answers.

**The problem.** This is the case D-020 kept for the bonus: "the uncertain call that cannot be rolled back". Design rule 5 says an uncertain call is never re-executed before it is reconciled. The rule had to be given a meaning for a tool with no rollback.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Send again with the same key; 2xx = APPLIED, 4xx = NOT APPLIED, no answer = still uncertain, stop | Taken | No moment at which the answer can be wrong. The re-send is the reconciliation, not a re-execution: the key makes it the same request |
| Ask the service "did you apply key K?" and trust the answer | Discarded | "No" can be stale: the first request may still be in flight and land afterwards. The agent would then ask again under a new key, and the work would be done twice |
| Mark it NOT APPLIED and let the agent decide | Discarded | Same double effect, with certainty whenever the answer (not the request) was lost |
| Mark it APPLIED without contacting the service | Discarded | Loses the effect whenever the request never left |
| Pause for a human every time | Discarded | Human approval is cut from v1; and with a key-honouring service the machine can settle it safely |
| Keep retrying inside recovery until the service answers | Discarded | `agentos recover` would hang for as long as a service is down. A bounded number of attempts, then a clear error, leaves the choice of when to retry to the person |

**What this means for the code.** `recovery::reconcile_external`. No workspace rollback, no checkpoint. The verdict text carries the first 200 characters of the service's answer, because a reconciled call has no stored output (the agent sees "applied" without the body; a known limit, same as for a reconciled `fs.write`). When there is no answer, `recover` exits with an error after `recovery.started` and without a `tool.reconciled`; D-056 already allows `recovery.started` to appear more than once. `run --resume` recovers first, so it stops the same way and the agent makes no further call. The guarantee rests on the service honouring keys; AgentOS cannot verify that from outside, and says so in the threat model.

**Evidence.** `tests/external.rs::an_outage_leaves_the_call_uncertain_until_recovery_gets_an_answer`: the first request is applied and its answer lost; `recover` and `run --resume` both fail with "still UNCERTAIN", write no verdict and make no new call; once the service answers, the verdict is APPLIED with `"replayed":true`, and the service holds each order once. `killed_after_the_intent_recovery_sends_the_request`: killed at P2, the service had seen nothing, recovery sent the request, the step was not asked for again.

**Concepts to learn.** Roll-forward versus roll-back recovery; at-least-once delivery plus idempotency equals effectively-once; why a status query is weaker than an idempotent retry.

### D-079 — No answer during a live run stops the run with the intent open
- **Date:** 2026-11-13
- **Status:** Taken
- **Area:** durability
- **ADR:** 0012

**In plain words.** A call can become uncertain without any crash: the service simply does not answer. The gateway tries up to `external.attempts` times (three by default), always with the same key. If there is still no answer, it does not write "completed" and does not write "failed", because it knows neither. It stops the whole run with an error message. The diary then looks exactly as it does after a crash at that point, and `agentos recover` deals with it by the rule of D-078.

**The problem.** The log has two endings for a call that ran: COMPLETED and FAILED ("nothing was changed"). Neither is true after a timeout.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Stop the run with an error; the open intent is the record | Taken | No new state and no new record type: an existing, tested path (recovery) handles it. "Uncertain" stays what it was, an intent without an ending |
| A new terminal state `UNCERTAIN` handed to the agent as a result | Discarded | The agent would continue past a call nobody has settled, which is what rule 5 forbids; and every reader of the log would need to learn a new ending |
| Record FAILED after the last timeout | Discarded | A lie whenever the service did the work |
| Retry for ever | Discarded | A run that hangs silently on a dead service |
| Refused (4xx) also stops the run | Discarded | A refusal is definite and nothing was done, so it is an honest FAILED and the agent can react |

**What this means for the code.** `gateway.rs::effect` returns a runtime error for `Answer::Unknown`; the process exits with status 1 and a message that names the call, the service, the number of requests and the two commands that continue. `agentos status` shows the execution as interrupted and `agentos trace` shows the call as UNCERTAIN. Worst-case time for one call is `attempts × timeout_ms`.

**Evidence.** `an_outage_leaves_the_call_uncertain_until_recovery_gets_an_answer`: exit code 1 (not a signal), stderr contains "is UNCERTAIN: no answer after 3 request(s)", one intent and no ending in the log, `status` says interrupted.

**Concepts to learn.** Fail-stop behaviour; why adding a state is more expensive than reusing one.

### D-080 — Fault-injection cells for `ext.call`: five kill points, three service faults, the service as oracle
- **Date:** 2026-11-13
- **Status:** Taken (adds to D-057; the 24 original cells are unchanged)
- **Area:** process
- **ADR:** none

**In plain words.** The harness gets eight new cells for the external tool. In five of them the runtime is killed: before the intent, after the intent but before the request, while the request is in flight, after the call was recorded as done, and during recovery itself. In three nothing is killed and the *service* misbehaves instead: it loses a request, it loses an answer, or it answers nothing at all for a while. After each trial the harness looks at the fake service's own list of what it applied. Each of the two orders in the script must be on that list exactly once.

**The problem.** D-057's oracle reads files in the workspace. An external effect is not in the workspace. And the bonus asks for "injectable timeouts", which are a kind of fault the P1–P8 list does not have: nothing dies, yet a call becomes uncertain.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| A second test in the same harness file, with its own workload (two `ext.call` steps between file and command steps), its own report, and the service's record as ground truth | Taken | The 24 original cells and their headline numbers stay exactly as they were; the service runs inside the test program, so it survives every kill and can be read directly |
| Add `ext.call` as a fifth tool to the existing matrix | Discarded | Would change the 4,800-run result of record, and P4–P6 (around the checkpoint) do not exist for a call that takes no checkpoint |
| Count duplicates by idempotency key | Discarded | The service never applies a key twice, so that count is always zero. A doubled effect shows up as the same *payload* under two keys; duplicates are counted by payload |
| Treat a run or recovery that stops "uncertain" as a failure of the trial | Discarded | Stopping is the designed behaviour when no answer comes. The trial tries `recover` again, as a person would, and checks that the stopped attempt wrote no verdict and made no new call |
| Only kill cells | Discarded | Would leave the "injectable timeouts" half of the bonus untested |

**What this means for the code.** `tests/recovery/main.rs::external_calls_hold_under_kills_and_timeouts`, cells P1, P2, P3, P7, P8, T1 (request lost), T2 (answer lost), T3 (no answer to any attempt; half the time the first recovery gets none either). Checks per trial: every request the service ever received carries a key that an intent in the log names (invariant 1 for an outside effect); recovery's verdict equals the service's record; every ending the log gives an `ext.call` agrees with the service; each order applied exactly once at the end; plus the workspace invariants I2–I5 from D-057, since the workload also writes files and runs a command. Each cell also asserts that its situation really occurred in more than half its runs ("as intended"), so a cell cannot pass by not happening. The mid-flight cell uses a slow or answer-losing service so the kill lands inside the request. `make eval-recovery` runs both tests; the report is `target/recovery-ext-report.json`. Recovery times in these cells include waiting for the service and are reported separately from the v0.1 numbers.

**Evidence.** Mutation check, 20 runs per cell (`docs/results/recovery-ext-mutations.md`): the real code gave 0 violations and 0 duplicated effects in 160 runs; "recovery says NOT APPLIED without asking" gave 48 duplicated effects; "a new key for every attempt" gave 70. The full-size run is recorded in the Findings row for it and in `docs/results/recovery-ext-200-nix.md`.

**Concepts to learn.** Test oracles outside the system under test; mutation testing; why duplicates must be counted by meaning (payload), not by identifier (key).

### D-081 — `make eval-recovery` runs its tests one at a time
- **Date:** 2026-11-13
- **Status:** Taken
- **Area:** process
- **ADR:** none

**In plain words.** The fault-injection test file now holds two big tests: the original 24 cells and the 8 external cells. Rust's test runner starts all tests of a file at the same moment. Each big test starts 8 worker threads of its own, so together they put 16 workers on the machine. The full-size command now tells the runner to run one test after the other.

**The problem.** The recovery times of the v0.1 cells (p50, p99) are quoted in the README and compared between runs. With a second test loading the machine at the same time, a new run would not be comparable with the old ones. B-008 had just shown how much load changes timing.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `--test-threads=1` for `make eval-recovery` | Taken | One flag; the same load per test as before the bonus |
| Leave them running together | Discarded | Both passed that way (see Evidence), but the timing numbers would quietly change meaning |
| Put the external cells in a separate test file and a separate make target | Discarded | They share the workload helpers and the invariant checks with the original cells; `make eval-recovery` should stay the one command for "does recovery hold" |
| Also serialise plain `cargo test` | Discarded | With 3 runs per cell the overlap lasts seconds, and `make test` is about pass or fail, not timing |

**What this means for the code.** One line in the `Makefile`. `make test` is unchanged. CI calls `make eval-recovery`, so it gets the same thing. Tests run in name order, so the external cells run before the 24 original cells.

**Evidence.** The first full-size run after the bonus ran both tests together: external cells 1,600 runs, 0 violations, 0 duplicated effects; original cells 4,800 runs, 0 violations, 0 duplicated effects. Those reports are kept as `docs/results/recovery-ext-200-nix-concurrent.json` and `recovery-200-nix-m7-concurrent.json` and are not the result of record; the results of record are from the sequential run (`docs/results/recovery-ext-200-nix.md`, `docs/results/recovery-200-nix-m7.md`).

**Concepts to learn.** How `cargo test` runs tests in parallel threads; why a measurement needs the same conditions each time.

### D-082 — The bonus counts as verified on `fhs` by one green CI run on the M7 commit
- **Date:** 2026-11-13
- **Status:** Taken (same rule as D-072, applied to the bonus)
- **Area:** process
- **ADR:** none

**In plain words.** The bonus was built and tested on NixOS. Prince pushed it, and GitHub's CI ran the same tests on Ubuntu 24.04. That run is green, so the documents now say the bonus passes on both systems, name the run, and keep the limits that D-072 set: one run, one Ubuntu version.

**The problem.** D-072 said a commit counts as verified on `fhs` only when a CI run on that commit is green. The bonus commits were written after the run D-072 cites, so they were "not yet verified" until now.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Record the first green run on `1bc27da`, save both of its reports, and update the documents that said "not yet verified" for the bonus | Taken | The rule from D-072, unchanged |
| Rewrite the M7 milestone document so it never says "not yet verified" | Discarded | Milestone documents keep their text; a dated note says what changed |
| Run the mutation check in CI as well | Discarded | It needs deliberately broken source; it shows the harness can fail, which does not depend on the distribution |

**What this means for the code.** No code change. `docs/results/recovery-ext-200-fhs.{md,json}` and `recovery-200-fhs-m7.json` are the results of record. The README bonus table has an `fhs` column; `docs/recovery-semantics.md` §9.4, `docs/threat-model.md` §7, the v1 document's Bonus status and CLAUDE.md cite the run. The M7 document has a dated note.

**Evidence.** <https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321>, commit `1bc27da`, conclusion `success`. External cells: 1,600 runs, 0 violations, 0 duplicated effects, 942/942 verdicts matching the service, 303 stops with the call left uncertain. The 24 original cells in the same run: 4,800 runs, 4,799 killed, 0 violations, 0 duplicated effects (`docs/results/recovery-ext-200-fhs.md`). The run's `make test` step, which includes `tests/external.rs`, passed. This was the first CI run on the bonus; unlike v0.1, it needed no fix.

**Concepts to learn.** None new; see D-072.

### D-083 — One push by the agent, at Prince's direct request
- **Date:** 2026-11-13
- **Status:** Taken (a one-time exception to D-027, which stays in force)
- **Area:** process
- **ADR:** none

**In plain words.** The rule is that the agent saves work locally and Prince does every upload to GitHub himself. On 2026-11-13, with the licence and the bonus finished and CI green, Prince told the agent to "push whatever is needed to be pushed". The agent did that once: the `main` branch and the tags `m0` to `m7` and `v0.1`. The rule itself is not changed. The next push is Prince's again unless he says otherwise.

**The problem.** D-027 and `CLAUDE.md` say "never run `git push`, in any form". Prince wrote that rule, and Prince gave a direct instruction that goes against it. Following the instruction silently would leave a push in the history that the written rule says cannot exist.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Push once, as asked, and record it as an exception | Taken | The rule exists so that Prince decides when work leaves his machine. He decided. The record says who decided and what was sent |
| Refuse and point at the rule | Discarded | The rule protects Prince's choice; it is not a reason to override his choice |
| Treat the request as lifting the rule for good | Discarded | He asked for one push, not for a change of policy. An old or general go-ahead is not a standing permission |
| Push `main` only, leave the tags | Discarded | "Whatever is needed": the tags were listed as an open item only he could close, and they were the only other thing not on the remote |

**What this means for the code.** Nothing. `CLAUDE.md`'s push rule is unchanged; its status section now says `main` and the tags are on GitHub. The untracked file `docs/project-overview.md` was not committed or pushed: the agent did not write it.

**Evidence.** Before the push, `git log origin/main..main` listed three commits and `git ls-remote --tags origin` listed no tags. The push commands and their results are in the Findings row of this date.

**Concepts to learn.** Why a rule and the reason for the rule are different things; branches versus tags on a remote.

### D-084 — `fs.write` makes the folders that are missing on the way to its file
- **Date:** 2026-11-14
- **Status:** Taken (replaces the last rule of D-040)
- **Area:** policy
- **ADR:** none

**In plain words.** Before, `fs.write` to `src/newpkg/mod.py` failed if the folder `src/newpkg` did not exist, and no tool could make a folder. An agent could therefore never add a new package, which is ordinary work. Now `fs.write` makes the missing folders itself, but only when the policy allows the file to be written there. Nothing new can be reached this way: the same rules decide, and the kernel still confines the path.

**The problem.** A hand check on 2026-11-13 showed the block (Findings). Prince chose to remove it. D-040 had discarded this because a crash between "folder made" and "file written" leaves a workspace that matches neither the old nor the new state.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `fs.write` makes missing folders; the intent lists them; recovery rolls back when it finds the list and no new file | Taken | No new tool. The crash case D-040 worried about is handled by the rule recovery already has for "neither old nor new": roll back to the last checkpoint |
| A fifth tool `fs.mkdir` | Discarded | A new tool is new scope (the tool list is fixed), and an empty folder is not something a patch can carry anyway |
| Leave it; let the agent make folders through `pytest` | Discarded | It works only by running code, which is the opposite of what the file tools are for |
| Allow the write if the *file* matches a write glob, and also require every new folder to match one | Discarded | `exec` may already leave new folders anywhere as long as they hold allowed files (D-009); one rule for both is easier to explain. Deny rules are still checked on every new folder |

**What this means for the code.**
- `tools/fs.rs::open_for_write`: if the folder cannot be opened because it does not exist, the deepest folder that does exist is opened instead, still with `openat2(RESOLVE_BENEATH)`, and the rest of the path is kept as a list of names to make (`WriteTarget.missing`). A `..` among those names is refused: a path through a folder that does not exist cannot be resolved.
- `gateway.rs::decide`: the policy is checked on the full resolved path as before. In addition every folder that would be made is checked against the deny rules, so `src/.env/x.py` is refused by the rule for `.env`.
- Nothing is made before the intent is on disk (design rule 3). The intent's plan gets `created_dirs`, the list of folders about to be made.
- `tools/fs.rs::write`: one `mkdirat` per folder, through the handle of the folder above, then open it with `O_NOFOLLOW`. `mkdirat` fails if the name exists in any form, including a symlink that points at nothing. If the write then fails, the folders are removed again, so FAILED still means "nothing changed".
- `recovery.rs`: a cut-off `fs.write` whose intent has `created_dirs` and whose file is absent is rolled back to the last checkpoint instead of being called NOT APPLIED on the spot, because some of the folders may exist.
- The LLM agent's description of `fs_write` says that missing folders are made.

**Evidence.** `tests/hardening.rs`, run on the `nix` preset on 2026-11-14: `fs_write_makes_missing_folders_inside_the_write_globs`, `fs_write_makes_no_folder_it_should_not` (8 refused forms; nothing made inside or outside the workspace) and `a_folder_making_write_is_recovered_at_every_crash_point` (P2, P4, P6 and 12 kills at P3) pass. Mutation check: with the new recovery rule switched off, the last test failed 3 times out of 3 with `P3 (delay 40): NOT APPLIED but folders were left behind`. Not yet run on `fhs`.

**Concepts to learn.** `mkdirat`; why "check, then act" is safe here (the check and the act both go through one already-opened folder handle).

### D-085 — When the runtime cannot record what a command left behind, the call fails and is undone
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** After every command the runtime walks the workspace and copies it. A command can leave things that make the walk or the copy fail: a file name that is not valid text, a file nobody may read, a folder nobody may open. Before, that stopped the whole run with an error. Now the runtime puts the workspace back to the last checkpoint and tells the agent that the call failed and why. The run goes on.

**The problem.** Bug B-009. Errors were split into "the tool failed" (told to the agent) and "the runtime broke" (the run stops) by which piece of code raised them. The scan and the copy are runtime code, but what they trip over was chosen by untrusted code.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Roll back to the last checkpoint, log FAILED, hand the reason to the agent | Taken | FAILED already means "nothing was changed" (D-040), and after the rollback that is true. No new state, no new record type |
| Repair the workspace instead (rename bad names, `chmod` files readable) and complete the call | Discarded | The runtime would be inventing workspace content the command did not write, and every repair is a new place for a bug |
| Skip what cannot be read and complete the call | Discarded | The checkpoint would no longer be a copy of the workspace, which is the invariant recovery rests on |
| Keep stopping the run | Discarded | Lets code in the repository end the agent's work, and the agent never learns why |
| Apply the same to a checkpoint failure after `fs.write` | Discarded | `fs.write` cannot create such files, and after this change none survive an `exec`. A copy failure there is a real runtime problem (disk full) and still stops the run |

**What this means for the code.** `gateway.rs::undo_exec` is called when `checkpoint::revert_outside` fails after a command, or when `checkpoint::create` fails for an `exec` call. It runs `checkpoint::restore` and returns a `ToolError`; host paths in the message are replaced by `<execution>`. If the restore itself fails, the runtime stops with the intent open, exactly as after a crash, and `agentos recover` settles it. The command's output is not kept for such a call. `checkpoint::create` removes its half-made copy when `cp` fails.

**Evidence.** `tests/hardening.rs`, `nix` preset, 2026-11-14: four tests named `…_fails_the_call_not_the_run` (non-UTF-8 name, `chmod 000` file, `chmod 000` folder inside and outside the write globs) pass; each checks that call 1 is FAILED, call 2 completes, the workspace equals `cp-0000` and no host path is in the message. All four failed on the code before the fix (commit `0c29b2d`). Not yet run on `fhs`.

**Concepts to learn.** Who caused an error versus where it surfaced; why a rollback keeps an invariant that a repair would not.

### D-086 — Trees shaped by sandboxed code are deleted with `remove_tree`, which unlocks folders first
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** To delete a folder you must be allowed to list it and to remove names from it. Those rights are three bits on the folder, and its owner can switch them off. A command in the sandbox runs as the same user as the runtime, so it can leave folders that the runtime's plain "delete this tree" cannot remove. Recovery deletes the old workspace as its last step, so one such folder made recovery fail every time. The runtime now switches the three bits back on, for folders only, and deletes again.

**The problem.** Bug B-010. While fixing it a second form turned up: a read-only folder (mode 555) with a file in it *can* be copied, so it ends up inside a checkpoint, and then neither a rollback nor the removal of an orphan checkpoint could delete it.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Try the plain delete; if it fails, give every folder in the tree owner read, write and search, then delete again | Taken | Small, and changes nothing in the common case |
| Normalise modes when the checkpoint is made | Discarded | Changes what the command wrote; a repo may really contain a read-only folder |
| Leave the undeletable tree and move on | Discarded | It is the old workspace under the name the next restore needs; and leftovers that grow are their own problem |
| Run the delete in a user namespace as "root" | Discarded | Far more machinery than three permission bits |

**What this means for the code.** `checkpoint::remove_tree` replaces `std::fs::remove_dir_all` in `checkpoint::create`, `restore`, `remove_leftovers` and `remove_entry`, and in `recovery::recover` (orphan checkpoints). It never follows a symlink: a mode is changed only on what `symlink_metadata` reports as a real folder. It relies on no sandboxed process being alive, which holds everywhere it is called (the exec's cgroup is destroyed first; recovery kills leftover cgroups first).

**Evidence.** `tests/hardening.rs`, `nix` preset, 2026-11-14: `recovery_works_after_a_kill_while_the_workspace_holds_a_locked_folder` (the runtime is killed from outside while the command sleeps; `recover` succeeds, a second `recover` finds nothing to do) and `recovery_works_when_a_checkpoint_holds_a_read_only_folder` (kill at P6; the orphan `cp-0002` is removed) pass. Both failed on the code before the fix. Not yet run on `fhs`.

**Concepts to learn.** Permission bits on a folder: read = list names, write = add and remove names, execute = "search" (use a name inside it). Why the owner is still bound by them (only root is not).

### D-087 — The post-M7 fixes count as verified on `fhs` by one green CI run on `7d0ac1b`
- **Date:** 2026-11-14
- **Status:** Taken (same rule as D-072 and D-082)
- **Area:** process
- **ADR:** none

**In plain words.** The fixes for B-009 and B-010 and the folder-making `fs.write` were built and tested on NixOS only. Prince pushed the commit, and GitHub's CI ran every suite on Ubuntu 24.04. The run is green, so the documents now say these changes pass on both systems and name the run.

**The problem.** D-084, D-085 and D-086 each end with "Not yet run on `fhs`". By D-072 that stays true until a CI run on that commit is green.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Record the green run on `7d0ac1b` in a new entry and update the status lines that said "not yet verified" | Taken | The rule from D-072, unchanged. D-084 to D-086 keep their text |
| Replace the results of record in `docs/results/` with this run's reports | Discarded | The existing files record v0.1 and M7. This run confirms that nothing regressed; its numbers are in the run's log and artifacts |

**What this means for the code.** No code change. `CLAUDE.md` ("Current status") and `docs/threat-model.md` §5 cite the run.

**Evidence.** <https://github.com/pessimistic-zer0/agentos/actions/runs/43573592353>, commit `7d0ac1b`, image `ubuntu-24.04`, conclusion `success`, read with `gh run view --log` on 2026-11-14. `tests/hardening.rs`: 9 passed, 0 failed. The other test files: 24, 1, 14, 2, 4, 32 and 4 passed, 0 failed. The 200-kill step: 4,800 runs, 4,798 killed, 0 invariant violations, 0 duplicated effects; external cells 1,600 runs, 0 violations, 0 duplicated effects. First CI run on these changes; it needed no fix. Not run on Ubuntu, as before: benchmarks, `make demo`.

**Concepts to learn.** None new; see D-072.

### D-088 — After M7: fix what a newcomer trips over, and open scope for exactly two features
- **Date:** 2026-11-14
- **Status:** Taken (Prince's decision)
- **Area:** scope
- **ADR:** none

**In plain words.** Prince asked where a real user would struggle. A walk-through with the built binary gave a list (Findings, 2026-11-14). Part of the list is mistakes and unclear wording in what v1 already has; those are fixed. The rest would be new features. Prince approved two of them, because without them a real project cannot get past its first test run: tests that need installed packages, and running on Ubuntu without switching off a system-wide protection. Everything else stays in the parking lot.

**The problem.** "Scope is fixed" (CLAUDE.md, the v1 document) and "make it usable by someone else" pull in opposite directions. Fixing defects is not new scope; new features are, and only Prince can open it.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Fix the defects inside v1; add two features: dependencies in the sandbox, and an AppArmor profile for Ubuntu | Taken | Claude's recommendation, approved by Prince on 2026-11-14 ("go on with your rec"). The two features are the ones that stop a user completely |
| Fix the defects only | Discarded | The tool would stay correct and unusable on any project with dependencies |
| Build the whole list (clean-up command, leaving folders out of the copy, starting itself under `systemd-run`, a Ctrl-C message, a check command, an apply command) | Discarded for now | Each is an inconvenience, not a wall. They are in the spec's parking lot (§22) |

**What this means for the code.** Inside v1: D-089 (first messages), D-090 (what the patch leaves out). New: D-091 and D-092 (dependencies), D-093 (AppArmor profile). The v1 document gets a section "After the bonus". CLAUDE.md's scope paragraph names the two additions. No tool is added and the five parts stay as they are.

**Evidence.** The walk-through: Findings row of 2026-11-14 and commit `b20e809`. Web reading on what users of similar sandboxes report (bug trackers and posts about bubblewrap, Claude Code, Cursor and Codex on Ubuntu 24.04; git-worktree set-ups): read through search summaries on 2026-11-14, not checked against each primary page.

**Concepts to learn.** Defect versus feature; why a scope rule needs an owner who can change it.

### D-089 — First messages: check the user's input first, say the fix first, start nothing before the checks pass
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** Four things a newcomer sees early were wrong or unclear. A mistake in the policy file was printed twice. A mistyped `--repo` could be hidden behind an unrelated complaint. Without the `systemd-run` prefix the message began with cgroup words and ended with the command to use. And `--agent llm` with no model server copied the whole workspace, then failed with "Connection refused", named no setting, and left a half-started execution behind.

**The problem.** None of these is wrong in the sense the suites test, and all of them cost a new user time.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Rename the inner error field of `PolicyError::Read` and `Parse` from `source` to `error` | Taken | `thiserror` treats a field called `source` as the cause; the message already contains it, so `anyhow` printed it twice |
| Check `--repo` before the policy's programs | Taken | The user's own argument first |
| One function for every "not delegated" error: the command to use, then what was found | Taken | The finding ("no `cpu` controller") means nothing without knowing cgroups |
| Before creating an execution with the LLM agent, open a TCP connection to the server; on failure name the three variables and their current values (never the key) | Taken | Fails in milliseconds, creates nothing |
| Send a real request (`GET /models`) instead | Discarded | Not every OpenAI-compatible server has it; a TCP connection is the one thing all accept |
| Start itself under `systemd-run` when it is not delegated | Discarded | A feature; parked (spec §22) |

**What this means for the code.** `policy.rs` (field rename), `sandbox/cgroup.rs::not_delegated`, `runtime.rs::create` (order; heading "the AgentOS sandbox cannot start"), `agent/llm.rs::LlmConfig::check_reachable`, called in `main.rs` for a new and a resumed LLM run. The check is skipped when a proxy variable is set, because the request then goes elsewhere. The mock server in `tests/llm_agent.rs` now ignores a connection that sends nothing.

**Evidence.** `nix` preset, 2026-11-14: `tests/llm_agent.rs::no_server_is_reported_before_anything_is_started`; `tests/hardening.rs`: `a_policy_mistake_is_reported_once_and_before_anything_is_started`, `a_wrong_repo_path_is_reported_as_that`, `without_a_delegated_cgroup_the_message_starts_with_what_to_do` pass. The two older LLM tests failed once after the change, because the mock counted the empty connection as a request; they pass with the mock corrected, assertions unchanged. Not yet run on `fhs`.

**Concepts to learn.** Error chains (`source`) in Rust; "fail fast".

### D-090 — The patch lists what it cannot carry
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** The result of a run is a patch: a text file of changed lines. Text cannot hold a picture or a compiled file, cannot say "this file is now executable", and cannot make an empty folder. `patch` and `git apply` skip all of that silently, so a user who applied the patch got less than the agent made and was not told. Now the runtime finds every such change, writes one comment line per change at the top of the patch, and prints one line at the end of the run.

**The problem.** In the walk-through a run added a binary file, made a script executable and changed a mode. `git apply` accepted the patch without a message; the binary file was missing and both modes were lost (Findings, 2026-11-14). Design rule 10 says the output is a patch, so the patch has to be honest about its limits.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Keep the unified diff; list binary files, mode changes, new executable files, empty folders (and, as before, symlinks) as `# agentos:` lines at the top, with where to copy a binary file from; print a one-line count after the run | Taken | No new dependency and no new command. Both `patch` and `git apply` ignore text before the first file header |
| A git-format patch (`git diff --binary`), which carries binary files and modes | Discarded | Makes `git` a runtime dependency, which the v1 stack does not list, and the result would only apply with git |
| An `agentos apply` command that copies the files itself | Discarded | A feature; parked (spec §22) |
| Say nothing in the run's output, only in the patch | Discarded | The comment lines are easy to miss; one line at the end is not |

**What this means for the code.** `checkpoint::write_patch` returns a `PatchReport` (`changed`, and `LeftOut` with four counts) instead of a bool. `runtime::finish` prints `NOT in the patch: …`. A run that changed only a mode now counts as changed. Python's `.pyc` files are binary, so under the shipped policies every run that starts `pytest` would get the note; D-092 stops them being written.

**Evidence.** `tests/hardening.rs`, `nix` preset, 2026-11-14: `the_patch_lists_what_it_cannot_carry` (binary file, executable new file, changed mode, empty folder; the text part still applies with `patch -p1`, and the binary file is then indeed absent) and `a_run_that_changes_only_text_gets_no_note` pass. The demo run printed `NOT in the patch: 2 binary files` for two `.pyc` files. While looking at that output a separate bug appeared: B-011, a relative state folder (`docs/bugs.md`). Not yet run on `fhs`.

**Concepts to learn.** What a unified diff is; file modes; why "silently skipped" is worse than "refused".

### D-091 — An allowed program is started through the path it was found at, not through its fully resolved path
- **Date:** 2026-11-14
- **Status:** Taken (changes how D-012's "resolve to the real path" is done)
- **Area:** sandbox
- **ADR:** none

**In plain words.** A Python virtualenv is a folder with its own packages and a `python3` that is only a signpost (a symlink) to the system's Python. Python uses the virtualenv's packages only when it is started through that signpost. The runtime used to follow every signpost before starting a program, so it started the system's Python and the virtualenv was lost. Now it keeps the path where the program was found, and only tidies the folder part.

**The problem.** First of the two features Prince approved (D-088): tests that import installed packages. The experiment `experiments/m8-venv` showed that a mounted virtualenv already worked for `pytest`, and for `python3 -m pytest` only by luck.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Keep the found path (folder resolved, name kept) when that path and every symlink step behind it are under a read-only mount; otherwise the fully resolved path, as before | Taken | The virtualenv's link survives; systems where the found path is not visible (NixOS: `/run/current-system/sw/bin`) behave as before |
| Always keep the found path | Discarded | On NixOS it is often outside the mounts, so the program could not be started |
| Teach the runtime about virtualenvs (`VIRTUAL_ENV`, `pyvenv.cfg`) | Discarded | Python-specific knowledge in `src/`. The link rule is general: Node, Ruby and others use the same trick |
| Mount the user's whole home folder read-only | Discarded | It holds `~/.ssh`. A mount is named one folder at a time, by the policy author |

**What this means for the code.** `tools/exec.rs`: `Programs::resolve` calls `link_chain_visible`. The check that the *real* file is under a mount is unchanged and still comes first. The error for a program outside the mounts now also says what to do for a virtualenv. The intent's `program` field shows the path used. On `fhs` the visible change is that `/usr/bin/python3` is started as that, not as `/usr/bin/python3.12`.

**Evidence.** `tests/deps.rs`, `nix` preset, 2026-11-14: `a_virtualenv_outside_the_repo_is_used_when_it_is_mounted_and_active` (both steps exit 0 inside the virtualenv; the intent names `venv/bin/python3`) and `a_virtualenv_that_is_not_mounted_is_refused_at_startup_with_the_way_out` pass. Mutation check: with the old "fully resolved" rule put back, the first test failed on the intent's program path (`/nix/store/…/python3.13`), and only there, which matches the experiment: it worked before, by luck. All other suites pass unchanged. Not yet run on `fhs`.

**Concepts to learn.** Symlink; virtualenv; how a program finds out where it was started from (`argv[0]`).

### D-092 — `[sandbox.env]`: fixed environment variables from the policy; four places a mount may not be
- **Date:** 2026-11-14
- **Status:** Taken (extends D-012 and the "environment cleared" rule: still nothing from the host's environment)
- **Area:** policy
- **ADR:** none

**In plain words.** Commands in the sandbox get three environment variables and nothing else, so that no secret from the user's shell leaks in. That stays. But some projects need a setting to find their packages or to run at all (`PYTHONPATH`, `NODE_PATH`, `DJANGO_SETTINGS_MODULE`). The policy can now list such variables with fixed values. It is the policy author who writes them, as plain text in the file; nothing is copied from the shell.

**The problem.** Second half of the dependencies feature. Also two things found on the way: a read-only mount under `/tmp` broke the sandbox with "File exists", and Python's `.pyc` files made every run's patch carry notes about binary files (D-090).

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| A table `[sandbox.env]` of name = "text"; `PATH`, `HOME` and `LANG` cannot be set | Taken | Smallest thing that works; the values are in the policy file, whose hash is in the log, so a run records what it was given |
| Pass chosen variables through from the host (`pass = ["PYTHONPATH"]`) | Discarded | The value would not be in the log, and one careless name (`AWS_SECRET_ACCESS_KEY`) leaks a secret |
| Let the agent set variables per call | Discarded | The agent is untrusted; `LD_PRELOAD` alone would be a new attack to think about, and the tool arguments are fixed |
| Refuse `ro_mounts` entries that are `/` or under `/work`, `/tmp`, `/proc`, `/dev` when the policy is loaded | Taken | The sandbox builds those itself; before, the failure came at the first command and named no cause |
| Make such mounts work | Discarded | No use for it; a folder can be kept elsewhere |
| Shipped policies set `PYTHONDONTWRITEBYTECODE = "1"` | Taken | No `.pyc` files, so the patch of a Python run is text only and D-090's note stays meaningful |
| Hide `.pyc` files from the note instead | Discarded | Python knowledge in `src/`, and the files would still be in the workspace and the checkpoints |

**What this means for the code.** `policy.rs`: `SandboxRules.env` (a sorted map), `PolicyError::Env` and `ReservedMount`, checked in `validate`. `gateway.rs::run_exec` appends the policy's variables after the three fixed ones. `policies/nix.toml` and `fhs.toml` gain the table. The write glob `**/__pycache__/**` stays: a policy without the variable still needs it. The benchmarks in `docs/results/bench-nix/` were measured before this change and have not been run again; Python without `.pyc` files may start a little slower.

What a policy author must know: everything under a read-only mount can be read by any code the agent runs, and a variable such as `LD_PRELOAD` or `PYTHONSTARTUP` changes what runs. Both are in the policy file, which is trusted (threat model §1); the agent can change neither.

**Evidence.** `nix` preset, 2026-11-14. `tests/deps.rs`: `a_package_folder_is_reached_through_a_mount_and_a_policy_variable` (the import works, the variable arrives, and two attempts to write into the mounted folder from the sandbox are refused; the folder is unchanged on the host), `without_the_mount_the_package_is_simply_not_there`, `the_shipped_policies_leave_no_compiled_python_files_behind` pass. `policy::tests`: `sandbox_env_is_fixed_text_with_three_names_reserved`, and 7 refused and 4 accepted mount paths in `bad_policies_are_refused`. The experiment is in `experiments/m8-venv/NOTES.md`. Not yet run on `fhs`.

**Concepts to learn.** Environment variables; why "copied from the shell" and "written in a file" differ for auditing; bytecode caches.

### D-093 — Ubuntu: an AppArmor profile for agentos, and a startup check that tries instead of reading switches
- **Date:** 2026-11-14
- **Status:** Taken; the profile is **not yet verified** (it can only be tried on Ubuntu, in CI, after Prince pushes). Its probe mechanism (`__probe-userns`) is replaced by D-096; the profile was verified by D-094
- **Area:** sandbox
- **ADR:** none

**In plain words.** Ubuntu (23.10 and newer) lets a program use user namespaces only if a small permission file, an AppArmor profile, says so. Until now the README told Ubuntu users to switch that protection off for the whole machine. agentos now comes with a profile that allows user namespaces for agentos alone. For that to work, the startup check had to change too: it used to read the kernel switch and refuse whenever it was on. Now it starts a tiny copy of itself that really tries to build a namespace, and only complains if that fails.

**The problem.** Second feature Prince approved (D-088). The reading on 2026-11-14 showed the same wall in bug reports for bubblewrap, Claude Code and Cursor, and one report asking tools to ship a profile instead of recommending the system-wide switch.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Ship `packaging/apparmor/agentos`: a profile with `flags=(unconfined)` and the single rule `userns,`, tied to `/usr/local/bin/agentos` | Taken | The form Ubuntu itself uses for programs that need user namespaces. It restricts nothing and grants one thing |
| Keep recommending `sysctl -w kernel.apparmor_restrict_unprivileged_userns=0` only | Discarded | Weakens every program on the machine to run one. It stays in the message as the second choice |
| A confining profile (list what agentos may read and write) | Discarded | The confinement is agentos's own job (namespaces, Landlock, cgroups). A second, partial description of it would be one more thing to get wrong and keep in step |
| Startup check: run `agentos __probe-userns`, which creates user, mount and network namespaces, writes the id maps, changes a mount and brings up loopback | Taken | These are the sandbox's own first steps. On Ubuntu a program without a profile may create the namespace and is then refused inside it, so only trying shows the truth |
| Keep reading the three kernel switches | Discarded | With the profile loaded the switch is still on, and agentos would refuse to start although it could |
| Change the existing CI job to use the profile | Discarded | If the profile had a fault, the job that verifies everything else would go red with it. A second job, `fhs-apparmor`, keeps the two results apart |

**What this means for the code.** `sandbox/ns.rs::probe_main` and `PROBE_ARG`; `main.rs` handles the hidden argument before `clap`, like `__sandbox-init`; `sandbox::check_host(helper)` runs the probe and uses the switches only to explain a failure. `packaging/apparmor/agentos` is new. `.github/workflows/ci.yml` has a second job that switches the restriction on, checks that agentos refuses and names the profile, installs the profile for the test binary's path, and runs the demo task and four suites. One more process is started per run (not measured).

**Evidence.** On NixOS (no AppArmor), 2026-11-14: the probe exits 0; with the limit set to 0 inside a throwaway user namespace it exits 1 with `creating a user namespace: ENOSPC`, and agentos stops with `user namespaces cannot be used … max_user_namespaces is 0` (`tests/hardening.rs::a_machine_without_user_namespaces_is_told_so_by_trying_not_by_guessing`). `make test`: 104 passed, 0 failed. The workflow file parses (`yq`). **Not run: the profile itself, the Ubuntu branch of the message, and the new CI job.** Whether code inside the sandbox keeps the profile's permission after `execve` is not known; the security suite in the new job will show it (`exec_cannot_mount_even_in_a_nested_user_namespace`).

**Concepts to learn.** AppArmor; a profile's `unconfined` flag; why "may create it" and "may use it" are different permissions.

### D-094 — The first-user work counts as verified on `fhs`, and the AppArmor profile as working, by CI run 43590153676
- **Date:** 2026-11-14
- **Status:** Taken (same rule as D-072, D-082 and D-087)
- **Area:** process
- **ADR:** none

**In plain words.** D-089 to D-093 were built and tested on NixOS, and the AppArmor profile could not be tried there at all. Prince pushed; GitHub ran both jobs on Ubuntu 24.04 and both are green. So these changes now count as passing on both systems, and the profile has been seen to work: with Ubuntu's restriction switched on, agentos first refused to start and named the profile, and after the profile was loaded it ran the demo task and four suites.

**The problem.** D-089 to D-093 each say "not yet run on `fhs`", and D-093 says the profile is not yet verified. By D-072 that holds until a CI run on the commit is green.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Record the green run on `366c986` in a new entry and update the lines in the README and CLAUDE.md that said "not yet verified" | Taken | The rule from D-072. The earlier entries keep their text |
| Make `fhs-apparmor` run every suite, including the 200 kills | Discarded | The profile changes whether the sandbox can start, not how recovery behaves; the main job already runs those suites |
| Stop offering the system-wide `sysctl` | Discarded | `make demo` and the main CI job run the binary from `target/`, where a path-bound profile does not apply |

**What this means for the code.** No code change. The README's Ubuntu block and status line, and CLAUDE.md's status, cite the run.

**Evidence.** <https://github.com/pessimistic-zer0/agentos/actions/runs/43590153676>, commit `366c986`, both jobs `success`, read with `gh run view --log` on 2026-11-14.
- Job `fhs-apparmor` (`kernel.apparmor_restrict_unprivileged_userns = 1`): without the profile, agentos stopped with `user namespaces cannot be used (writing /proc/self/setgroups: Permission denied (os error 13))` and printed the install command for `packaging/apparmor/agentos`. With the profile loaded (`aa-status` lists `agentos`): the demo script `COMPLETED … after 7 steps`; `vertical_slice` 4, `security` 32, `benign` 1, `deps` 5 passed, 0 failed. `exec_cannot_mount_even_in_a_nested_user_namespace` passed, which answers the question D-093 left open as far as that test goes.
- Job `fhs` (restriction off, as before): `make test` 25 + 1 + 5 + 14 + 16 + 3 + 4 + 32 + 4 passed, 0 failed (104 tests), which includes `tests/deps.rs` with a real `python3 -m venv` on Ubuntu; lint clean; 200-kill step 4,800 runs, 4,800 killed, 0 violations, 0 duplicated effects; external cells 1,600 runs, 0 violations, 0 duplicated effects.
- This was the first run for all of it; nothing needed a fix. What it shows about Ubuntu itself: a program without a profile is refused at the first write to `/proc/self/setgroups`, so the namespace is created and then unusable, as D-093 expected.
- Not run on Ubuntu, as before: benchmarks, `make demo`. Not tested anywhere: the profile at its shipped path `/usr/local/bin/agentos` (CI rewrites the path to the test binary), and Ubuntu versions other than 24.04.

**Concepts to learn.** None new; see D-072 and D-093.

### D-095 — The patch also lists empty files (completes D-090)
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** durability
- **ADR:** none

**In plain words.** D-090 made the patch say what it cannot carry. The first run on a real project showed one more such thing: a new file with nothing in it, like the `__init__.py` that starts a Python package. The patch tools create nothing for it. It is now on the list, with the command that creates it.

**The problem.** Bug B-012.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Count new and deleted empty files in the patch's notes, with `touch <path>` as the hint | Taken | Same mechanism as D-090; one more kind |
| Write a newline into empty files so that the diff carries them | Discarded | Changes what the agent wrote |

**What this means for the code.** `checkpoint.rs`: `LeftOut.empty_files`; two more cases in `write_patch`.

**Evidence.** `tests/hardening.rs::the_patch_lists_what_it_cannot_carry`, extended, passes on `nix` (2026-11-14); the same test asserts that `patch -p1` leaves the empty file out. Seen first in the trial: `docs/results/real-repo-trial-nix.md`. Not yet run on `fhs`.

**Concepts to learn.** None new; see D-090.

### D-096 — The user-namespace probe runs in a forked child, not as a second start of the program (replaces the probe of D-093)
- **Date:** 2026-11-14
- **Status:** Taken; D-093's profile and its reasons stand, its `__probe-userns` mechanism is replaced
- **Area:** sandbox
- **ADR:** none

**In plain words.** To find out whether user namespaces work, the runtime tries to make one. The first version did that by starting the same program a second time with a secret argument. That only works if the program is `agentos`. The benchmarks are different programs that use the same code, and for them the trick started a second benchmark. Now the try happens in a short-lived copy of the running process (a fork) that never becomes another program, so it does not matter which program it is.

**The problem.** Bug B-013.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Do the probe's steps in the forked child of a `Command`, in `pre_exec`, and `_exit` there with the number of the failed step; the error number goes to the child's stderr as digits | Taken | Works from any program. Nothing is ever started |
| Keep the hidden argument and make every program handle it | Discarded | Every benchmark, example and future user of the library would have to remember |
| Probe in the runtime's own process | Discarded | `unshare(CLONE_NEWUSER)` changes the calling process for good, and refuses if it has several threads |
| Call `fork()` directly | Discarded | `Command` already does the fork, the pipes and the wait correctly |

**What this means for the code.** `sandbox/ns.rs::probe_in_child` (an `unsafe fn`: only async-signal-safe calls, no allocation, because another thread may have held the allocator's lock at the moment of the fork; the id-map text is prepared before the fork) and `PROBE_STEPS`. `sandbox::check_host` builds the `Command` and turns exit code and error number into the message. `main.rs` no longer knows a probe argument. The messages are unchanged in shape, so the CI job `fhs-apparmor`, which looks for two of their phrases, needs no change.

**Evidence.** `nix` preset, 2026-11-14: `make test` 105 passed; clippy clean; `the_startup_check_works_from_a_program_that_is_not_agentos` passes; with the limit set to 0 in a throwaway namespace the message is `user namespaces cannot be used (creating a user namespace: No space left on device (os error 28))`; the `fs.write` benchmark runs again (`docs/results/bench-nix-m8/`). Not yet run on `fhs`, and in particular the Ubuntu message (which step fails without the profile) has not been seen with this version.

**Concepts to learn.** `fork` and `exec` as two steps; async-signal-safety; why a library must not assume which program it is in.

### D-097 — M8 counts as verified on `fhs` by CI run 43612319559
- **Date:** 2026-11-14
- **Status:** Taken (same rule as D-072, D-082, D-087, D-094)
- **Area:** process
- **ADR:** none

**In plain words.** The last part of M8 (the empty-file note, the rewritten startup probe, two new CI steps) had only run on NixOS. Prince pushed tag commit `69cdbf0`; both CI jobs on Ubuntu 24.04 are green. So all of M8 now passes on both systems, and the two things D-094 listed as never tried on Ubuntu have been tried: the demo script, and the profile installed at the path the README gives.

**The problem.** The M8 document and CLAUDE.md say "not yet verified on `fhs`" for the commits after `366c986`. The probe in particular had been rewritten (D-096) after the run that verified the profile.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Record the green run on `69cdbf0`; update CLAUDE.md; add a dated note to the M8 document | Taken | The rule from D-072; milestone documents keep their text |
| Replace the `fhs` results of record with this run's reports | Discarded | As in D-087: this run confirms that nothing regressed |

**What this means for the code.** No code change.

**Evidence.** <https://github.com/pessimistic-zer0/agentos/actions/runs/43612319559>, commit `69cdbf0`, both jobs `success`, read with `gh run view --log` on 2026-11-14.
- Job `fhs-apparmor` (restriction on): with the rewritten probe and no profile, agentos stopped with `user namespaces cannot be used (writing /proc/self/setgroups: Permission denied (os error 13))`, the same step as before the rewrite. Then the program and the profile were installed exactly as the README says (`/usr/local/bin/agentos`, `/etc/apparmor.d/agentos`) and the demo task `COMPLETED … after 7 steps` from there; then again with the profile bound to the test binary; `vertical_slice` 4, `security` 32, `benign` 1, `deps` 5 passed.
- Job `fhs`: lint clean; `make test` 25 + 1 + 5 + 14 + 17 + 3 + 4 + 32 + 4 = 105 passed, 0 failed; `make demo` ran through, including the kill, `RECOVERED`, `#6 fs.write … APPLIED` and the resumed run; 200-kill step 4,800 runs, 4,800 killed, 0 violations, 0 duplicated effects; external cells 1,600 runs, 0 violations, 0 duplicated effects.
- The 200-kill step took 1,672 s, against about 1,010 s in the two runs before, and recovery's p50 was 31.3 ms against 21 to 26 ms. Everything in the job was slower in the same proportion (the quick recovery test took 26 s against 18 s), so this looks like a slower runner; it was not looked into further.
- Not run on Ubuntu, as before: the benchmarks. Ubuntu versions other than 24.04: not tested.

**Concepts to learn.** None new; see D-072.

### D-098 — M9: Prince opens scope for the parked conveniences, except `apply` and binary file tools
- **Date:** 2026-11-14
- **Status:** Taken (Prince's decision)
- **Area:** scope
- **ADR:** none

**In plain words.** After M8 the parking lot held twelve ideas from the walk-through and the real-project trial. Prince asked why not build all of them. Nothing technical stood in the way; the reason was the scope rule, which is his to change. Claude recommended building ten, and holding two; Prince answered "as you wish". So M9 builds those ten. No tool is added: the tool list stays at four plus the bonus.

**The problem.** Each parked item is small, but together they are about as much work as M8, and each is a guess about what a user needs, made without a user.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Build: a clean-up command, self-start under `systemd-run`, a message on Ctrl-C, a check command, reading part of a file, a note for an inactive virtualenv, a clearer end when the model gives up, a skip list and a scratch list in the policy, exceptions to deny rules | Taken | Each removes a block or a daily annoyance that was seen, not imagined (Findings, 2026-11-14, two rows) |
| `agentos apply` | Held | It writes into the original repository. Design rule 10 says the original repo is never modified; Claude asked Prince to confirm that an explicit command by the user is within that rule, and proposed to hold it until then. "As you wish" is taken as agreement with the proposal, not as that confirmation |
| Binary files and file modes through the file tools | Held | Useless before `apply` exists (the patch cannot carry the result), and it widens what the agent can ask for |
| A git-format patch | Discarded | Makes `git` a runtime dependency; `apply` would solve the same thing better |
| A read-only mount *inside* the workspace (for `node_modules`) | Discarded for now | Changes how the workspace, the checkpoints and the revert see `/work`; the skip list covers what was actually met (a virtualenv, which works from its own path) |

**What this means for the code.** D-099 to D-107. CLAUDE.md's scope paragraph and the v1 document name M9. The spec's parking lot marks what was built.

**Evidence.** Prince's messages of 2026-11-14 in the session that built M8 and M9: the list pasted back with "why not implement all of this?", then "as you wish" to Claude's recommendation.

**Concepts to learn.** None new.

### D-099 — Two more lists in the policy: `skip` (not copied) and `scratch` (a cache, not a change)
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** policy
- **ADR:** none

**In plain words.** Until now a path was either allowed, or denied because it is a secret. Two everyday cases fit neither. A virtualenv inside the repo is big and already visible to commands through its read-only mount, so copying it into the workspace only costs time and disk; in the trial the trick was to *deny* it, which says "secret" and means "big". And a test runner's cache (`.pytest_cache`) is rewritten on every run; the runtime put it back each time and reported a policy violation. `skip` says "leave this out of the copy". `scratch` says "commands may write here; it is not a change".

**The problem.** Trial on the inventory project: 2,779 files and 140 MB with the virtualenv copied, 73 files and 8.3 MB without; and `2 change(s) outside write globs reverted` on every `pytest`.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| `filesystem.skip`: globs that only shape the copy | Taken | One meaning. A skipped path is not secret; the agent may still be refused there by the other rules |
| Keep using `deny` for it | Discarded | Mixes two meanings; the copy report then lists a virtualenv among the secrets |
| `filesystem.scratch`: globs where a command's changes are kept, not reported, and left out of the patch and its notes | Taken | The cache keeps working (`--lf` and the like), and a violation means something again |
| Put caches into the write globs | Discarded | Then they are in the patch, and the agent's `fs.write` may write there |
| Leave scratch paths out of checkpoints too | Discarded | A checkpoint is a full copy of the workspace (D-007); an exception there is a new invariant to test. Caches are small |
| Deny wins over scratch | Taken | A denied path stays denied whatever else matches |

**What this means for the code.** `policy.rs`: `FsRules.skip`, `scratch`; `Policy::skipped`, `scratch`, `command_may_change`. `workspace.rs`: `CopyReport.skipped`; the `workspace.created` event gains `skipped` (absent in older logs). `gateway.rs`: the revert after a command uses `command_may_change`. `checkpoint::write_patch` takes a scratch test and drops those sections and notes; "changed" now means "something is left after that". The agent's `fs.write` still needs a write glob. The shipped policies have `skip = []` and `scratch = [".pytest_cache/**"]`.

**Evidence.** `nix` preset, 2026-11-14. `tests/deps.rs`: `a_virtualenv_inside_the_repo_is_skipped_in_the_copy_and_still_used`, `a_cache_is_scratch_and_an_example_env_file_is_not_a_secret`, and the control `without_a_scratch_rule_the_cache_is_reverted_and_reported` pass. `policy::tests::deny_exceptions_skip_and_scratch` passes. All older suites pass unchanged. Not yet run on `fhs`.

**Concepts to learn.** Why one word should not carry two meanings in a security rule.

### D-100 — `filesystem.deny_except`: names a deny rule matches that are not secrets
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** policy
- **ADR:** none

**In plain words.** The shipped rule `.env.*` keeps `.env.local` and `.env.production` away from the agent. It also catches `.env.example`, the template that holds no secret and that an agent should read to learn which settings exist. The pattern language has no "not", so the exceptions get their own list.

**The problem.** Trial: `fs.read .env.example` was denied, and the file was not even in the workspace.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| A list `deny_except`; a path that matches it is not denied. Shipped: `.env.example`, `.env.sample`, `.env.template`, at any depth | Taken | The deny rules stay as wide as they were. An exception is a name someone wrote down on purpose |
| Narrow the deny rule to known secret names (`.env.local`, `.env.production`, …) | Discarded | Misses the next name (`.env.staging`). A false denial costs a question; a missed secret is a leak |
| Add `!pattern` to the glob language | Discarded | Order-dependent rules are how deny lists go wrong; and D-032 keeps the matcher small |

**What this means for the code.** `Policy::denied` returns nothing when an exception matches. Everything that asks "is this denied?" goes through it: the copy, the file tools, the revert. A real secret named `.env.example` would now be copied; that is the policy author's risk and is said in the policy file's comment.

**Evidence.** Same tests as D-099: `.env.example` is read with its content, `.env` is still denied and absent. Not yet run on `fhs`.

**Concepts to learn.** Allow-list versus deny-list, and why exceptions should be explicit.

### D-101 — A note when a mounted folder has its own copy of an allowed program and another one was found
- **Date:** 2026-11-14
- **Status:** Taken
- **Area:** tooling
- **ADR:** none

**In plain words.** In the trial, the virtualenv was mounted but not activated. The run used the system's `pytest`, which knew none of the project's packages, and ended with "exit 4" and nothing pointing at the cause. Now the runtime notices: a folder in `sandbox.ro_mounts` has `bin/pytest`, but the `pytest` on `PATH` is a different file. It prints a note saying so and what to do, and carries on.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Print a note on stderr and continue | Taken | It may be intended (a tools folder that happens to have the program) |
| Refuse to start | Discarded | Not every such set-up is a mistake |
| Activate the virtualenv automatically (prefer a mount's `bin`) | Discarded | The runtime would be choosing programs the user did not put on `PATH` |

**What this means for the code.** `tools/exec.rs`: `Programs.notes`, filled in `resolve`; printed by `runtime::create` and `resume` as `note: …`.

**Evidence.** `tests/deps.rs::a_mounted_virtualenv_that_is_not_active_gets_a_note`: the note appears and the run goes on; with the virtualenv active there is no note. `nix` preset, 2026-11-14. Not yet run on `fhs`.

**Concepts to learn.** None new; see D-091.

### D-102 — `fs.read` can return part of a file (`start_line`, `max_lines`)
- **Date:** 2026-11-14
- **Status:** Taken (extends D-041; the cap on one answer stays)
- **Area:** policy
- **ADR:** none

**In plain words.** `fs.read` refused any file larger than the answer limit (64 KB in the shipped policies), and there was no way to look at a piece of it. In the trial a ticket was about a 134 KB data file the agent could not open. Now the agent may ask for lines: "from line 100, 50 lines". The answer still has to fit in the same limit; what changes is that a big file can be read in pieces.

**The problem.** Trial on the inventory project (Findings, 2026-11-14): `fs.read tests/fixtures/stock_movements_2025.csv` failed, and the only way to see the file's content was through the project's own import command.

**Options looked at.**
| Option | Result | Why |
|---|---|---|
| Two optional fields on `fs.read`: `start_line` (from 1) and `max_lines`. The answer holds as many whole lines as fit in `max_output_kb`, and says which lines they are and how many the file has | Taken | Still one tool, still one bounded answer. The total tells the agent where it is |
| Raise the limit | Discarded | The limit protects the log and the model's context; any fixed number is too small for some file |
| Byte offsets | Discarded | An agent thinks in lines; a byte range can cut a character in half |
| A fifth tool (`fs.search`, `fs.head`) | Discarded | The tool list is fixed |
| Cut the file silently at the limit | Discarded | The agent would believe it had seen the whole file, and then write it back shortened |

**What this means for the code.** `ToolCall::FsRead` and `LoggedCall::FsRead` gain the two fields (left out of the JSON when absent, so old logs and scripts read as before). `tools/fs.rs::read` goes through the file once, one line in memory at a time, and stops at 256 MB. `ToolOutput::Read` gains `lines` (`first`, `last`, `total`). The LLM's tool description and the text it gets back say that the answer is only part of the file. Deny rules and `openat2` apply as for any read. Still true and still a limit: `fs.write` replaces a whole file, so an agent cannot safely rewrite a file it has only read in part.

**Evidence.** `tests/hardening.rs::part_of_a_large_file_can_be_read`, `nix` preset, 2026-11-14: a 3,000-line, 150 KB file; a whole read is refused and names the two fields; lines 1 to 50 come back exactly; from line 2990 to the end; a start past the end and a start of 0 fail with a reason; a request for more than fits returns whole lines up to the limit; a denied file stays denied. `agent::llm::tests::tool_calls_are_parsed_strictly` covers the new arguments. Not yet run on `fhs`.

**Concepts to learn.** Paging through data; why a truncated answer must say that it is truncated.

---

## Still open

| Question | Notes |
|---|---|
| Project name | See D-023. Still open at v0.1 (D-068) |
| ~~License~~ | Answered by D-069: dual MIT OR Apache-2.0 |
| ~~Public repo from day one, or after v0.1?~~ | Answered by events: Prince pushed `main` to GitHub on 2026-11-12, after v0.1 was tagged locally |
| ~~Does the `fhs` preset pass on Ubuntu?~~ | Answered by D-072: yes on Ubuntu 24.04, CI run 43385661105 (third run; the first two found B-007 and B-008) |
| Do the benchmarks and `make demo` work on Ubuntu? | Not run. CI runs the test suites only (D-072) |
| ~~Which LLM for the demo: local llama.cpp server or a hosted API?~~ | Answered by D-058: local llama.cpp |

---

## Findings

Bugs, failed experiments and surprises, newest last. Each one: date, what was expected, what happened, what was learned.

| Date | Finding | Learned |
|---|---|---|
| 2026-11-10 | Git snapshot with a separate `GIT_DIR` did not restore the folder exactly (see D-007) | Test a mechanism before building on it; "git has everything" is false for ignored files, empty directories, modes and nested repos |
| 2026-11-10 | The planned `/usr` and `/lib` mounts do not exist on the development machine (see D-012) | Distro layout is configuration, not code |
| 2026-11-11 | `nix develop` on the latest `nixos-unstable` stalled for 22 minutes downloading a new compiler set over a slow link (see D-029) | Pin to what is already on disk when the network is slow; a lock file makes either choice reproducible |
| 2026-11-11 | With `RUST_BACKTRACE=1` set in the dev shell, every `anyhow` error from the M0 program printed a 12-frame backtrace under a one-line message | `anyhow` captures a backtrace whenever that variable is set; leave it unset by default and set it only when debugging |
| 2026-11-11 | The demo patch contains "Binary files … differ" lines for pytest's `__pycache__/*.pyc` files. `patch -p1 --dry-run` and `git apply --check` both still accept it | Running tests changes more than the source; harmless here, but it shows why D-009 (revert changes outside the write globs) is needed |
| 2026-11-11 | M1 was committed as a series made by a script. Two mistakes: commits `13da7df`, `0a2c113` and `c52f417` dropped entry D-032 from this log again, and `c52f417` does not build because `src/tools/mod.rs` was not staged. The build check had run on the working folder, not on the commit. Both were fixed in the next commit, not by rewriting history | Check each commit from a clean checkout (`git worktree add`), not from the working folder; prefer committing as the work happens over rebuilding a series afterwards |
| 2026-11-11 | Inside `unshare -Urmpn`, `~/.ssh` was still readable and `/sys/class/net` still listed the host's network cards (see `experiments/m2-unshare`) | A new mount namespace is a copy of the host's mounts and hides nothing by itself; files disappear only after `pivot_root` into a root built from the allowed folders. Mount no `/sys` |
| 2026-11-11 | The first Landlock experiment failed before running anything: spawning a child returned `EACCES` because `Command::output()` opens `/dev/null` and Landlock refused it (found with `strace`) | Landlock is allow-only; the sandbox must list the few device files ordinary programs expect |
| 2026-11-11 | `fs.write` failed with "Invalid argument" on its first sandboxed run: `openat2` rejects `O_PATH` together with `O_NOCTTY` (bug B-004 in `docs/bugs.md`) | `openat2` checks flag combinations strictly; an experiment must use the same flags as the real code |
| 2026-11-11 | First full run of the security suite: 20 of 25 failed because concurrent runtimes raced in cgroup setup (B-001) | Start-up code must tolerate another copy of itself running at the same time |
| 2026-11-11 | A run without `systemd-run` passed the startup check, rearranged the terminal's cgroup, then failed at `cpu.max` (B-003). The `user.delegate` xattr was checked as a possible test and is not set on this machine even for a delegated scope | Probe with the real operation; ask systemd for `Delegate=` before changing anything |
| 2026-11-11 | `diff -ruN --no-dereference` exits 2 when a symlink exists on one side only (B-002) | Tools combined with unusual flags need the unusual inputs tested too |
| 2026-11-11 | Inside the sandbox even `os.listdir("/")` and `os.listdir("/dev")` are refused: Landlock allows only the listed folders, not their parents | Allow-lists apply to exactly what is listed; the probe script had to be changed twice |
| 2026-11-11 | A security test expected `chmod src` to be reverted. It was not, correctly: `src/**` matches `src` itself, so that is an allowed change. The test now changes a folder outside the globs | The test was wrong, not the code; write the expectation from the rule (D-032), not from intuition |
| 2026-11-11 | M1 test `denied_command_and_unknown_path_are_reported` asserted the old message `invalid request` for `../../etc/passwd`; with `openat2` the rule is `path leaves the workspace` | The assertion was updated to the new mechanism; it still requires a denial |
| 2026-11-11 | `experiments/m3-rename-kill`: writing a file in place left it half-written in 149 of 200 kills; temp file + rename in 0 of 200, but left the temp file behind 164 times | Atomic rename works; recovery must delete the leftover temp file, so the intent records its name |
| 2026-11-11 | `experiments/m3-sqlite-kill`: 200 kills during 3-row transactions, 0 corrupt databases, 0 gaps, 0 half transactions, 0 lost acknowledged commits. This is the evidence D-005 said was still to come | One event per transaction gives a gap-free, untorn log under process kill |
| 2026-11-12 | The fault-injection harness passed 72 of 72 on its first run. Two bugs were then seeded into recovery to see whether it could fail: it reported 31 duplicated effects and 21 violations respectively, and 0 again with the real code | A test that has never failed has not been tested; break the code on purpose once |
| 2026-11-12 | Full run: 4,800 kills across P1–P8 and four tools, 0 invariant violations, 0 duplicated effects, 3,866/3,866 verdicts correct, recovery p50 19.1 ms / p99 49.8 ms (D-057, `docs/results/recovery-200-nix-m3.md`) | The intent-before-effect design holds under process kill on this machine. It says nothing about power loss |
| 2026-11-12 | With a kill timer of up to 8 ms, most P5 kills landed after the checkpoint copy (about 3 ms) had finished | Measure how long the window is before choosing the random delay |
| 2026-11-12 | Real-model run 1 (Qwen3-VL 4B): the model wrote only the changed function to `fs_write`, deleting `median`; pytest then failed on import; the model listed `src` and `tests` alternately until the 50-call budget ran out | Small models misuse whole-file writes; a loop needs to notice repetition |
| 2026-11-12 | Run 2: after the same partial write, the model repeated one paragraph until its context was full; the run ended "completed" with that text as its summary | Cap reply length; "the agent stopped" is not "the task is done"; cap what is stored as a summary |
| 2026-11-12 | Run 3 (sharper prompt): same partial write; the model then tried to create a root `__init__.py` (denied: no matching write rule) and re-ran pytest five times | Prompt wording alone did not fix it. The policy held against a confused model exactly as against a hostile one |
| 2026-11-12 | Run 4 (with a warning when a write drops previously read lines): the model made the partial write, read the warning, rewrote the whole file, tests passed. Repeated 3 more times with the same result; also completed after being killed at P4 and resumed | Tell the model what its action did to the world, in the tool result, instead of telling it harder what to do |
| 2026-11-12 | A `llama-server` built with CUDA and a 3.7 GB model were already on the machine from another project; an API key for a hosted service was also in the environment and was not used (D-058) | Look at what is already there before downloading or borrowing |
| 2026-11-12 | A new security test timed out: `fs.read` of a FIFO created by a command blocked the runtime for ever (B-005) | A path inside the workspace can still name something unsafe to open; check the file type, not only the location |
| 2026-11-12 | First TOCTOU test "passed" in 0.16 s having made only 15 calls: the host-side swapper also broke the runtime's checkpoint copy, the run stopped, and the test skipped its statistics | A test must fail when its dangerous case did not happen; it now requires >1,000 swaps and ≥20 calls ending each way |
| 2026-11-12 | Second TOCTOU attacker (exchanging two names in the workspace): 290 of 300 calls refused. A checkpoint copied mid-exchange held two symlinks and no real folder, and a rollback restored that | The attacker's own setup can spoil the race; re-pointing one symlink keeps the workspace valid (D-062) |
| 2026-11-12 | A security test's "canary" string was found in the log: it was in the attack command's own argv, which is logged on purpose | Build canaries at run time inside the attack, or the test finds its own text |
| 2026-11-12 | The benchmark's direct baseline failed: on NixOS `true` is a symlink to a multi-call `coreutils` binary that looks at `argv[0]` | Resolving a program to its real path must keep the original name as `argv[0]` (the gateway already did; the baseline did not) |
| 2026-11-12 | Benchmarks (release build, i5-13420H, btrfs, commit `357c313`; `docs/results/bench-nix/`): sandboxed `true` p50 7.8 ms / p95 19.6 / p99 24.6 against 1.3 / 2.2 / 2.8 direct; `fs.write` 4 KB p50 13.7 ms / p95 20.4 / p99 23.8 against 1.9 / 2.5 / 2.8 for a durable direct write; demo task p50 507 ms / p95 620 / p99 663 against 376 / 432 / 521 direct (+35%) | The sandbox costs about 6 ms per command; a write costs about seven durable writes (three fsynced log records, a blob, the file, a checkpoint copy). Both are small next to an LLM call or a test run |
| 2026-11-12 | First full run of the demo script: the recover and resume steps acted on the wrong execution. Two runs started in the same second, and ids had one-second resolution, so "latest" was picked by the random suffix (B-006, D-067) | Run the thing a stranger will run, end to end, before calling it done; an ordering claim ("ids sort by time") needs a test with two close ids |
| 2026-11-12 | Final fault-injection run on the release code: 4,800 runs, 4,797 killed, 0 violations, 0 duplicated effects, 3,918/3,918 verdicts correct, recovery p50 20.4 ms / p99 55.1 ms (`docs/results/recovery-200-nix.md`). Agrees with the M3 run on older code | Two independent full runs, one result |
| 2026-11-12 | Fresh clone, `nix develop -c make demo`: 23 s including a full debug build. Suspiciously fast, so checked: the clone had built its own 428 MB `target/`; crate sources and Nix packages were already on the machine | Check a number that looks too good; say what it does not include (downloads) |
| 2026-11-12 | First CI run (Ubuntu 24.04, `fhs`, run 43274718767). The workflow's uncertain parts (systemd user session for cgroup delegation, the AppArmor user-namespace setting; D-065) worked unchanged. Lint and `make test` passed. The 200-kill suite failed: 1 violation in 4,800 runs, a `.tmp-cp-0002` folder left after recovery (`docs/results/recovery-200-fhs-run1-failed.md`) | The part expected to break did not; the part believed proven did. The `cp` child outlived the killed runtime (B-007, D-070) |
| 2026-11-12 | B-007 could not be seen in 9,600 local kills, so it was made certain instead of likely: a `cp` wrapper that sleeps 0.4 s turned a sub-millisecond window into a test that failed every time before the fix and passes after | To reproduce a rare race, slow down one side of it on purpose |
| 2026-11-12 | After the B-007 fix, on the development machine (`nix`): lint clean, `make test` passes, and the full suite gives 4,800 runs, 4,788 killed, 0 violations, 0 duplicated effects, 3,887/3,887 verdicts correct, recovery p50 19.5 ms / p99 48.7 ms (`docs/results/recovery-200-nix-after-b007.json`) | The fix did not disturb anything locally. It says nothing yet about Ubuntu: that needs a push and a new CI run |
| 2026-11-13 | Second CI run (run 43375633929, with the B-007 fix): the B-007 leftover was gone, but 3 of 4,800 recoveries were refused with "in use by another agentos process" (`docs/results/recovery-200-fhs-run2-failed.md`). Reproduced locally with 64 workers; tag `v0.1` did not show it (`docs/results/b008-stress.md`) | The fix had created the next bug (B-008): `pre_exec` makes Rust use `fork`, and a forked child that has not run yet holds a copy of the parent's lock. Rerun the whole suite after every change to crash handling, on the machine that found the bug |
| 2026-11-13 | To reproduce a CI-only race locally, the harness was run with 64 workers on 12 CPUs. It showed the error within two minutes | Overloading the machine is a cheap way to widen scheduling windows; a quiet laptop hides them |
| 2026-11-13 | After the B-008 fix, on the development machine (`nix`): lint clean, `make test` passes, full suite 4,800 runs, 4,799 killed, 0 violations, 0 duplicated effects, 3,932/3,932 verdicts correct, recovery p50 21.4 ms / p99 56.7 ms (`docs/results/recovery-200-nix-after-b008.json`) | Clean locally at normal load, as it also was after B-007; the 64-worker stress and the next CI run are the checks that count here |
| 2026-11-13 | Third CI run, <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> (commit `c3c9502`, Ubuntu 24.04, `fhs`): green. 4,800 kills, 0 violations, 0 duplicated effects, 3,756/3,756 verdicts correct; 32 security cases, 0 escapes; benign suite passes (`docs/results/recovery-200-fhs.md`, `security-fhs.md`). `agentos recover` p99 was 315.3 ms, against 97.1 and 108.3 ms in the two failed runs; the cause was not looked into | The second machine was worth more than more runs on the first: 14,400 kills at normal load on the laptop found neither bug, 9,600 on the runner found both. Portability testing is also a way of finding timing bugs |
| 2026-11-13 | External-call harness, mutation check (20 runs per cell): with recovery answering "not applied" without asking the service, 48 duplicated effects and 37 of 90 verdicts wrong; with a fresh key per attempt, 70 duplicated effects, all 20 answer-lost runs doubled. Real code: 0 and 0 (`docs/results/recovery-ext-mutations.md`) | The two "obvious" designs (tell the agent it failed; retry on timeout) are exactly the ones that double an irreversible effect. In both, the cells where no request had left stayed clean, which is why such bugs survive light testing |
| 2026-11-13 | First version of the external cells compared recovery's verdict with the service even when the kill timer had fired late, inside a later file write. No failure was seen, but an applied `fs.write` would have been reported as a wrong verdict | A timer-based kill can land in a different call than the one aimed at; check which call was actually cut off before judging it |
| 2026-11-13 | Full-size run with the bonus, `nix` preset, tests one at a time: external cells 1,600 runs, 0 invariant violations, 0 duplicated effects, 932 of 932 verdicts matching the service's record; the 24 original cells 4,800 runs, 4,791 killed, 0 violations, 0 duplicated effects, 3,895/3,895 verdicts correct, recovery p50 19.8 ms / p99 55.9 ms (`docs/results/recovery-ext-200-nix.md`, `recovery-200-nix-m7.md`) | Sending the same request again with the same key held at every kill point and for every kind of silence the fake service can produce. It says nothing about a service that ignores keys, and nothing yet about Ubuntu |
| 2026-11-13 | CI run <https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321> on the M7 commit `1bc27da` (Ubuntu 24.04, `fhs`): green. External cells: 1600 runs, 0 invariant violations, 0 duplicated effects, 942/942 cut-off calls classified correctly, 303 stops with the call left uncertain. The 24 original cells: 4800 runs, 4799 killed, 0 invariant violations, 0 duplicated effects, 3724/3724 classified correctly (`docs/results/recovery-ext-200-fhs.md`) | The bonus holds on the second system as well. One green run, as with D-072 |
| 2026-11-13 | Commit `75c9178` cites D-082, but the entry itself was missing from it: the script that wrote the entry stopped on an undefined variable, and the commit command ran anyway. The entry was added in the next commit, not by rewriting history | Chain steps so that a failed one stops the rest (`&&`), and look at the diff before committing. The same kind of slip as the M1 series (Findings, 2026-11-11) |
| 2026-11-13 | The one push of D-083: `git push origin main` sent `1bc27da..b122e09` (four commits); `git push origin m0 m1 m2 m3 m4 m5 m6 m7 v0.1` created nine new tags on the remote. Afterwards `git status -sb` showed `main` level with `origin/main` and `git ls-remote --tags origin` listed all nine. This row was written after the push, so the commit that holds it is local until Prince pushes it | A record of a push cannot be inside the push it describes; the last entry is always one commit behind |
| 2026-11-13 | Asked to verify the finished project, Claude ran all suites (pass) and then tried by hand what the suites do not: files a sandboxed command leaves behind that the runtime's own bookkeeping cannot handle. Expected a failed tool call. Got a stopped run for a non-UTF-8 file name and a `chmod 000` file, and for a `chmod 000` folder an execution that `recover` and `--resume` both refuse forever (B-009, B-010 in `docs/bugs.md`, both open). Also seen, and by design (D-040): `fs.write` cannot create a file in a folder that does not exist, and no shipped tool can make a folder | The post-`exec` scan, the checkpoint copy and the restore run in trusted code but on a tree shaped by untrusted code; their errors were treated as runtime failures. The security suite tests for escapes and the recovery suite for crashes of a well-behaved workspace; nothing tested the two together |
| 2026-11-14 | Fixing B-010 (a `chmod 000` folder), the same fault was expected only in workspaces a command had just left. A second test showed it also in a state every suite had called healthy: a read-only folder (mode 555) with one file copies without complaint, sits in a checkpoint, and then no rollback and no orphan-checkpoint removal can delete it. All 9 new tests in `tests/hardening.rs` failed on the old code and pass on the new (D-084 to D-086). With the new recovery rule for folder-making writes switched off, the P3 kills caught it 3 runs out of 3. Afterwards on `nix`: `make test` 90 passed, 0 failed; clippy 0 warnings; `RUNS=20 make eval-recovery` 480 + 160 runs, 0 violations, 0 duplicated effects (a quick look, not a result of record) | The dangerous leftovers are not only the ones that make a step fail at once; some pass every check and break a *later* step. Deleting is an operation with its own permissions, separate from reading and copying |
| 2026-11-14 | Prince asked where a real user would struggle. Claude used the built binary as a newcomer would, on the `nix` preset. Run and seen: (1) a patch from a run that added a binary file, made a script executable and changed a mode applied with `git apply` without any message, but the binary file was missing and both modes were lost; (2) a test importing a package from a `.venv` inside the repo ended with exit 2: the virtualenv is not used; (3) `--agent llm` with no server running copied the workspace, failed with `Connection refused`, named no environment variable, and left an execution that shows as `interrupted` with 0 calls; (4) a repo of 20,000 small files (79 MB) took 5.4 s for the 7-call demo script against 0.6 s for the demo repo, with 4 full checkpoints; (5) a policy parse error is printed twice; (6) without the `systemd-run` prefix the message names a missing `cpu` controller before it names the fix. Read, not run: there is no signal handler, so Ctrl-C ends the run without a word; nothing deletes old executions; the README does not say how to apply the patch or write a policy. A Ctrl-C test from a background shell was void (background jobs ignore SIGINT) | The suites prove the claims; none of them plays a newcomer. Features that would close these gaps are outside v1 and are in the parking lot (spec §22) until Prince decides. Items 1, 3, 5, 6 and the README gaps are defects or wording inside v1 |
| 2026-11-14 | First run on a project that is not the demo: an inventory service with real PyPI packages in a virtualenv, written by a separate AI agent that was told not to look at AgentOS. Expected the new dependency support to be the weak point. It worked at once (same `3 failed, 75 passed` as outside; after four tickets were fixed through the gateway, 79 passed, in the sandbox and again after `git apply` in a clone). What failed was elsewhere: the applied patch lacked the package's empty `__init__.py` (B-012, fixed, D-095); `.env.example` was denied; a 134 KB fixture and a gzip file could not be read; `.pytest_cache` showed up as a policy violation on every test run; a forgotten `activate` gave exit 4 with no hint. The local 4B model solved a one-line ticket in 5 calls and 33 s and gave up on a two-module ticket after one call, which the runtime records as `COMPLETED`. Record: `docs/results/real-repo-trial-nix.md` | A project nobody tuned for the tool finds different things from a test written by the tool's author: D-090's list of what a patch cannot carry was written from memory and was one item short. The limits that bite first are the file tools' (size, text only), not the sandbox's |
| 2026-11-14 | Re-running the benchmarks after this round was expected to be routine. `make bench` ended with a Python traceback about a missing data file. Three steps earlier the `fs.write` benchmark had died, and the script's `\|\| true` had hidden it: the new startup probe (D-093) started the benchmark binary a second time (B-013; fixed by D-096). The same code had passed CI on both jobs. Numbers after the fix, `docs/results/bench-nix-m8/`: sandboxed `true` p50 5.62 ms (direct 1.06), `fs.write` 4 KB p50 13.34 ms, demo end to end p50 481 ms against 354 ms direct. All lower than the v0.1 run, the direct ones too, so the machine was quieter; nothing can be said about the code getting faster. The 200-kill run on `nix` after this round: 4,800 + 1,600 runs, 0 violations (`docs/results/recovery-200-nix-m8.md`) | Benchmarks are also a test: they are the only programs here that use the library without being the `agentos` binary. A suite that never leaves the main binary cannot see what a library user sees. And a step that may fail quietly should not be followed by steps that need its output |

---

## Glossary

Plain meanings for terms used above. Add to it as new terms appear.

| Term | Plain meaning |
|---|---|
| ADR | Architecture Decision Record: a short note on one hard-to-undo decision |
| argv / `execve` | The list of words that make up a command, and the kernel call that runs it without a shell |
| Atomic rename | Replacing a file in one step, so readers see the old file or the new one, never half of each |
| Bind mount | Making a folder appear at a second place in the file tree |
| Capabilities | Root's power split into pieces that can be dropped one by one |
| `cgroup.kill` | A file in a cgroup; writing `1` kills every process in the group at once |
| cgroup v2 | Kernel feature that limits memory, CPU and process count for a group of processes |
| Checkpoint | A saved copy of the workspace at a known moment |
| Dual licensing | Offering the same code under two licenses; the person using it picks one |
| Content-addressed storage | Storing a file under the name of its own hash |
| FIFO (named pipe) | A special file that passes bytes from a writer to a reader; opening it waits for the other side |
| Advisory lock (`flock`) | A lock on a file that cooperating programs check; the kernel releases it when the holder dies |
| Blob | A stored chunk of bytes (file content, tool output), named by its hash |
| Execution | One run of one agent on one copy of a repo, with its own log and workspace |
| Context window | How much text a language model can take in at once; when it is full, generation stops |
| Event log | The ordered, add-only record of everything that happened |
| `fsync` | Asking the kernel to really write a file to disk before continuing |
| Function calling (tool calls) | An API feature where the model answers with a function name and arguments instead of text |
| GGUF | The file format llama.cpp uses for model weights |
| Glob | A path pattern: `*` matches part of a name, `**` any number of folders |
| Hash | A short fingerprint of some data; the same data always gives the same hash |
| Init (PID 1) | The first process in a PID namespace; it must collect dead children, and when it exits the kernel kills the rest |
| Idempotent | Doing it twice gives the same result as doing it once |
| Idempotency key | A label sent with a request so that sending it twice has the effect of sending it once |
| Permission bits (mode) | Nine switches on every file and folder: read, write and execute, each for the owner, the group and everyone else. `chmod 000` switches all off; `555` is read and execute for all, write for nobody |
| Search bit | The execute bit on a folder. Without it nobody (except root) can use any name inside the folder, even its owner |
| Virtualenv | A folder holding one project's Python packages and a `python3` that is a link to the system's Python; Python uses the folder's packages only when started through that link |
| AppArmor | A Linux security module that limits programs by profiles (permission files), one per program path. Ubuntu uses it to decide who may use user namespaces |
| Async-signal-safe | Safe to call in a process that was just forked from a program with several threads: it takes no lock and allocates no memory. Raw system calls are; `format!` is not |
| Intent | A log record written before an action, saying what is about to happen |
| Helper (re-exec) | Starting your own program a second time to do a job that needs a fresh, single-threaded process |
| Landlock | Kernel feature that lets a process give up its own access to files, permanently |
| Magic link | A special `/proc` symlink (such as `/proc/self/root`) that jumps straight to another place, ignoring normal path rules |
| Mount propagation | Whether mounts made in one namespace also appear in another; `private` turns that off |
| Namespace | A private view of some part of the system (mounts, process IDs, network, users) for one group of processes |
| `no_new_privs` | A flag that stops a process from ever gaining more rights, for example through setuid programs |
| `openat2` | A way to open a file that lets the kernel refuse any path leaving a given folder |
| Oracle (testing) | The part of a test that knows the right answer, ideally worked out without the code under test |
| Orphan checkpoint | A checkpoint folder on disk that no log record names; left by a crash, deleted by recovery |
| `fork` / `exec` | `fork` makes a copy of the running process; `exec` turns a process into another program. Starting a program is the two together, and the copy exists for a moment in between |
| Orphan process | A process whose parent has died; it keeps running under a new parent unless something stops it |
| p50 / p95 / p99 | The time that 50%, 95%, 99% of samples stayed under; p99 shows the slow cases the average hides |
| Page cache | Memory where the kernel holds file data before writing it to disk |
| Patch (unified diff) | A text file listing changed lines, which `patch` or `git apply` can apply to another copy |
| `pivot_root` | Swapping what a process sees as the root folder `/` |
| Policy | The rules file: what the agent may read, write and run |
| Prompt injection | Text in a file or web page that tricks the AI into following it as an instruction |
| `PR_SET_PDEATHSIG` | Ask the kernel to send this process a signal when its parent dies |
| Reconciliation | Finding out whether an uncertain action really happened |
| `RENAME_EXCHANGE` | A flag for `renameat2` that swaps two names in one atomic step |
| Reflink | A copy that shares disk blocks with the original until one of them changes |
| Mutation check | Breaking the code on purpose to see whether the tests notice |
| SPDX expression | A short standard spelling of a license choice, such as `MIT OR Apache-2.0`, that tools can read |
| seccomp | Kernel feature that blocks chosen system calls for a process |
| tmpfs | A filesystem that lives in memory |
| uid map | The table that says which user id inside a user namespace is which user id outside |
| TOCTOU | "Time of check to time of use": a bug where something changes between checking it and using it |
| WAL | Write-ahead log: changes go to a side file first, which makes crashes safe |
| Loopback (`127.0.0.1`) | The address that always means "this same computer"; nothing sent to it leaves the machine |
| SSRF | Server-side request forgery: tricking a trusted program into making a network request the attacker chose |
| Test double (fake) | A simple stand-in for a real outside system, built so tests can control and inspect it |
| HTTP status classes | 2xx: done. 4xx: the request was refused. 5xx: the server had a problem (the work may or may not have happened) |
| Roll forward | Finishing an interrupted action instead of undoing it; the only choice when undoing is impossible |
| Effectively-once | The result of sending a request possibly several times to a receiver that ignores repeats of the same key |
