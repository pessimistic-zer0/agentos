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

---

## Still open

| Question | Notes |
|---|---|
| Project name | See D-023 |
| License | MIT or Apache-2.0; Apache-2.0 adds a patent grant |
| Public repo from day one, or after v0.1? | Not discussed yet |
| Which LLM for the demo: local llama.cpp server or a hosted API? | Either works with an OpenAI-compatible endpoint |

---

## Findings

Bugs, failed experiments and surprises, newest last. Each one: date, what was expected, what happened, what was learned.

| Date | Finding | Learned |
|---|---|---|
| 2026-11-10 | Git snapshot with a separate `GIT_DIR` did not restore the folder exactly (see D-007) | Test a mechanism before building on it; "git has everything" is false for ignored files, empty directories, modes and nested repos |
| 2026-11-10 | The planned `/usr` and `/lib` mounts do not exist on the development machine (see D-012) | Distro layout is configuration, not code |

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
| cgroup v2 | Kernel feature that limits memory, CPU and process count for a group of processes |
| Checkpoint | A saved copy of the workspace at a known moment |
| Content-addressed storage | Storing a file under the name of its own hash |
| Event log | The ordered, add-only record of everything that happened |
| `fsync` | Asking the kernel to really write a file to disk before continuing |
| Hash | A short fingerprint of some data; the same data always gives the same hash |
| Idempotency key | A label sent with a request so that sending it twice has the effect of sending it once |
| Intent | A log record written before an action, saying what is about to happen |
| Landlock | Kernel feature that lets a process give up its own access to files, permanently |
| Namespace | A private view of some part of the system (mounts, process IDs, network, users) for one group of processes |
| `no_new_privs` | A flag that stops a process from ever gaining more rights, for example through setuid programs |
| `openat2` | A way to open a file that lets the kernel refuse any path leaving a given folder |
| Page cache | Memory where the kernel holds file data before writing it to disk |
| `pivot_root` | Swapping what a process sees as the root folder `/` |
| Policy | The rules file: what the agent may read, write and run |
| Prompt injection | Text in a file or web page that tricks the AI into following it as an instruction |
| Reconciliation | Finding out whether an uncertain action really happened |
| Reflink | A copy that shares disk blocks with the original until one of them changes |
| seccomp | Kernel feature that blocks chosen system calls for a process |
| tmpfs | A filesystem that lives in memory |
| TOCTOU | "Time of check to time of use": a bug where something changes between checking it and using it |
| WAL | Write-ahead log: changes go to a side file first, which makes crashes safe |
