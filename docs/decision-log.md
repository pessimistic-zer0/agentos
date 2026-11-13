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
- **Status:** Taken
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

---

## Still open

| Question | Notes |
|---|---|
| Project name | See D-023. Still open at v0.1 (D-068) |
| ~~License~~ | Answered by D-069: dual MIT OR Apache-2.0 |
| ~~Public repo from day one, or after v0.1?~~ | Answered by events: Prince pushed `main` to GitHub on 2026-11-12, after v0.1 was tagged locally |
| Does the `fhs` preset pass on Ubuntu? | Not yet. Prince pushed on 2026-11-12. The first CI run passed lint and `make test` with `fhs`, and failed the 200-kill suite with 1 violation (B-007, D-070). The second run (with the B-007 fix) failed with 3 violations of a new kind (B-008, D-071), brought in by that fix. Still "not yet verified" until a run is green |
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
