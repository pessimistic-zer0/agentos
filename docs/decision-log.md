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
- **Status:** Taken (temporary; to be replaced in M2)
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

**What this means for the code.** `runtime::lock_execution` and `is_locked`. `File::try_lock` became stable in Rust 1.89, so `Cargo.toml` sets `rust-version = "1.89"`.

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
| 2026-11-11 | `nix develop` on the latest `nixos-unstable` stalled for 22 minutes downloading a new compiler set over a slow link (see D-029) | Pin to what is already on disk when the network is slow; a lock file makes either choice reproducible |
| 2026-11-11 | With `RUST_BACKTRACE=1` set in the dev shell, every `anyhow` error from the M0 program printed a 12-frame backtrace under a one-line message | `anyhow` captures a backtrace whenever that variable is set; leave it unset by default and set it only when debugging |
| 2026-11-11 | The demo patch contains "Binary files … differ" lines for pytest's `__pycache__/*.pyc` files. `patch -p1 --dry-run` and `git apply --check` both still accept it | Running tests changes more than the source; harmless here, but it shows why D-009 (revert changes outside the write globs) is needed |
| 2026-11-11 | M1 was committed as a series made by a script. Two mistakes: commits `13da7df`, `0a2c113` and `c52f417` dropped entry D-032 from this log again, and `c52f417` does not build because `src/tools/mod.rs` was not staged. The build check had run on the working folder, not on the commit. Both were fixed in the next commit, not by rewriting history | Check each commit from a clean checkout (`git worktree add`), not from the working folder; prefer committing as the work happens over rebuilding a series afterwards |
| 2026-11-11 | Inside `unshare -Urmpn`, `~/.ssh` was still readable and `/sys/class/net` still listed the host's network cards (see `experiments/m2-unshare`) | A new mount namespace is a copy of the host's mounts and hides nothing by itself; files disappear only after `pivot_root` into a root built from the allowed folders. Mount no `/sys` |
| 2026-11-11 | The first Landlock experiment failed before running anything: spawning a child returned `EACCES` because `Command::output()` opens `/dev/null` and Landlock refused it (found with `strace`) | Landlock is allow-only; the sandbox must list the few device files ordinary programs expect |

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
| Advisory lock (`flock`) | A lock on a file that cooperating programs check; the kernel releases it when the holder dies |
| Blob | A stored chunk of bytes (file content, tool output), named by its hash |
| Execution | One run of one agent on one copy of a repo, with its own log and workspace |
| Event log | The ordered, add-only record of everything that happened |
| `fsync` | Asking the kernel to really write a file to disk before continuing |
| Glob | A path pattern: `*` matches part of a name, `**` any number of folders |
| Hash | A short fingerprint of some data; the same data always gives the same hash |
| Idempotency key | A label sent with a request so that sending it twice has the effect of sending it once |
| Intent | A log record written before an action, saying what is about to happen |
| Landlock | Kernel feature that lets a process give up its own access to files, permanently |
| Namespace | A private view of some part of the system (mounts, process IDs, network, users) for one group of processes |
| `no_new_privs` | A flag that stops a process from ever gaining more rights, for example through setuid programs |
| `openat2` | A way to open a file that lets the kernel refuse any path leaving a given folder |
| Page cache | Memory where the kernel holds file data before writing it to disk |
| Patch (unified diff) | A text file listing changed lines, which `patch` or `git apply` can apply to another copy |
| `pivot_root` | Swapping what a process sees as the root folder `/` |
| Policy | The rules file: what the agent may read, write and run |
| Prompt injection | Text in a file or web page that tricks the AI into following it as an instruction |
| Reconciliation | Finding out whether an uncertain action really happened |
| Reflink | A copy that shares disk blocks with the original until one of them changes |
| seccomp | Kernel feature that blocks chosen system calls for a process |
| tmpfs | A filesystem that lives in memory |
| TOCTOU | "Time of check to time of use": a bug where something changes between checking it and using it |
| WAL | Write-ahead log: changes go to a side file first, which makes crashes safe |
