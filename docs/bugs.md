# Bugs found by the test suites

Short notes on bugs that the security suite or the fault-injection harness
caught in AgentOS itself, written when they were found. Each one: what the
test did, what went wrong, why, and the fix. (Mistakes in the tests themselves
are in the Findings table of `decision-log.md`, not here.)

## B-001 — cgroup setup raced when several runtimes started at once
- **Found:** 2026-11-11, by the security suite (20 of 25 tests failed on the first full run).
- **Symptom:** `cannot create a child cgroup … File exists` and `cannot enable controllers … Device or resource busy`.
- **Cause:** `cargo test` starts many `agentos` processes in one delegated scope at the same moment. Each one created the leaf group (the second `mkdir` failed), and each one did "move all processes to the leaf, then enable controllers" once. A process spawned into the scope between those two steps made the enable fail: the kernel refuses to enable controllers while any process sits in the parent group.
- **Fix:** an existing leaf is fine; "move everyone, enable controllers" is retried until the controllers are on (`sandbox/cgroup.rs::CgroupRoot::init`). See D-049.
- **Lesson:** setup code that is correct alone can still race with a copy of itself.

## B-002 — the run crashed at the end when the agent created a symlink
- **Found:** 2026-11-11, by `patch_does_not_follow_symlinks_to_host_files`.
- **Symptom:** `Error: diff failed: diff: …/cp-0000/src/leak.py: No such file or directory`, after all tool calls had completed.
- **Cause:** `diff -ruN --no-dereference` cannot describe a symlink that exists on one side only; GNU diff 3.12 prints an error and exits 2, which the runtime treated as fatal.
- **Fix:** one-sided symlinks are detected by the runtime and listed in the patch as comment lines; diff's exit code 2 is accepted only when its complaints are exactly those paths (D-050).
- **Lesson:** the hostile case (a planted link) was handled safely, but not *gracefully*: a denial-of-service on the final step. Test the unhappy path all the way to the end of the run.

## B-003 — a non-delegated cgroup passed the startup check, then failed mid-run
- **Found:** 2026-11-11, by running the demo by hand without `systemd-run`.
- **Symptom:** call #1 worked; call #2 (`exec`) died with `writing 200000 100000 to …/cpu.max: Permission denied`. The terminal's own cgroup had been rearranged (its processes moved into a leaf group) and a stray group was left behind.
- **Cause:** the startup check treated "I can create a child group" as proof of delegation. A user can do that in any of their scopes; systemd just does not hand the controllers over.
- **Fix:** the runtime asks systemd (`Delegate=` property) before touching anything, and probes by creating a group with all three limits (D-049). The same command now stops at startup with a message naming the fix, and changes nothing.
- **Lesson:** a startup check must exercise the exact operation that will be needed later, and must not leave changes behind when it fails.

## B-004 — `fs.write` always failed with "Invalid argument"
- **Found:** 2026-11-11, by the first sandboxed run of the demo script.
- **Symptom:** `#6 fs.write src/stats.py … FAILED: Invalid argument`.
- **Cause:** the parent folder is opened with `O_PATH`. The helper added `O_NOCTTY` to every open. Plain `open` ignores unknown flag combinations; `openat2` is strict and rejects `O_PATH` combined with anything except `O_CLOEXEC`, `O_DIRECTORY` and `O_NOFOLLOW`.
- **Fix:** `beneath::open_beneath` adds `O_NOCTTY` only when `O_PATH` is not set.
- **Lesson:** `openat2` validates its arguments where `open` silently ignores them. That strictness is a feature, and it means the experiment (which never used `O_PATH` with `openat2`) did not cover the real use.

## B-005 — `fs.read` of a FIFO hung the runtime forever
- **Found:** 2026-11-12, by `fs_read_of_a_fifo_planted_by_a_command_does_not_hang_the_runtime` (added in M5; it failed on its first run by timing out at 20 s).
- **Symptom:** the run never ended. No error, no log record after the `tool.requested`.
- **Cause:** a command run by `exec` may create a FIFO (named pipe) inside the write globs. `fs.read` opened the path with a plain `O_RDONLY`. Opening a FIFO for reading blocks until some process opens it for writing, and nobody ever does: every sandboxed process is dead by then. The path stayed inside the workspace, so `openat2` rightly had no objection. Confinement was intact; availability was not.
- **Fix:** `fs.read` opens with `O_NONBLOCK` and refuses anything that is not a regular file (`tools/fs.rs`). `fs.write` already refused non-regular targets. The same run now records two FAILED calls ("not a regular file") and finishes.
- **Lesson:** "the path is inside the workspace" is not the same as "the thing at that path is safe to open". Files are not the only things with names. This is a denial of service by the agent against its own run, so the damage is small, but an unattended runtime that hangs silently is exactly what the watchdog exists to prevent for commands, and the file tools had no such guard.

## B-006 — "the latest execution" could be the wrong one
- **Found:** 2026-11-12, by the first complete run of `demo.sh` (not by a test suite; recorded here because it is a runtime bug).
- **Symptom:** after a deliberately killed run, the demo's `status`, `recover` and `run --resume` reported "completed" and "already finished; nothing to recover": they were looking at the earlier, finished run.
- **Cause:** execution ids were `YYYYMMDD-HHMMSS-xxxxxx`. `agentos trace` and `status` without an id, and the demo script, take the last id in sorted order as the latest. Two runs started within the same second sorted by their random suffix. The unit test for ids only checked the format.
- **Fix:** ids now include milliseconds (`YYYYMMDD-HHMMSSmmm-xxxxxx`), with a test that two ids made 3 ms apart sort in order (D-067). The demo script picks the second execution by "not the first one" instead of by order.
- **Lesson:** nothing was damaged (recovery refused to touch a finished run), but a person following the README would have been told a crashed run was fine. The claim "ids sort by start time" had been written down in D-036 and never tested with two ids.

## B-007 — a checkpoint copy outlived the killed runtime
- **Found:** 2026-11-12, by the fault-injection harness on the first CI run (Ubuntu 24.04, `fhs` preset): 1 violation in 4,800 runs, cell `P5 exec`. The `nix` runs on the development machine (two full runs, 9,600 kills) never hit it. Record: `docs/results/recovery-200-fhs-run1-failed.md`.
- **Symptom:** `leftover after recovery: checkpoints on disk [".tmp-cp-0002", "cp-0000", "cp-0001"], named in the log ["cp-0000", "cp-0001"]`. A half-made checkpoint folder existed *after* recovery had deleted all of them.
- **Cause:** a checkpoint is made by a child process, `cp -a`. A child does not die when its parent is killed; it is handed to another parent and carries on. Killed at P5, the runtime died, but its `cp` lived. If the kill landed before `cp` had created its folder, recovery found nothing to delete, finished, and *then* the orphaned `cp` created `.tmp-cp-0002` and filled it. The window is the time between starting `cp` and its first `mkdir`: well under a millisecond, so it takes thousands of kills, a slower machine, or luck. The sandbox helper already had the cure for the same problem (`PR_SET_PDEATHSIG`, D-042, D-055); the checkpoint copy did not.
- **Why it matters beyond a stray folder:** the orphan is a process still reading the workspace and writing next to the checkpoints while recovery and the resumed run work on them. The harness saw only the mild outcome (a leftover). A resumed run that takes the same checkpoint number deletes the leftover `.tmp-` folder and copies again, at the same time as the orphan; that was not observed, and the fix removes the possibility.
- **Reproduced:** `checkpoint_copy_dies_with_the_runtime` in `tests/recovery/main.rs` puts a wrapper script named `cp` first in `PATH` that sleeps 0.4 s and then runs the real `cp`. With that, the old code fails every time with the same message as CI.
- **Fix:** the `cp` child sets `PR_SET_PDEATHSIG(SIGKILL)` before it becomes `cp`, and stops if its parent has already changed (`checkpoint::run_cp`, D-070). The kernel now kills the copy at the moment the runtime dies. The same function is used for rollback copies, so recovery's own `cp` is covered when recovery is killed (P8).
- **Lesson:** "kill the process" and "stop the work" are different things as soon as the process has children. Every child the runtime starts needs an answer to "what happens to you if I die?". Also: the bug needed a second machine to show itself. Passing 9,600 runs on one laptop was evidence, not proof; that is why the `fhs` box in the definition of done was left open instead of assumed.

## B-008 — recovery was refused because a dead runtime's lock lived on for a moment
- **Found:** 2026-11-13, by the fault-injection harness on the second CI run (Ubuntu 24.04, `fhs`): 3 violations in 4,800 runs. **Introduced by the fix for B-007.** Record: `docs/results/recovery-200-fhs-run2-failed.md`.
- **Symptom:** `recover failed: Error: execution … is in use by another agentos process`, straight after the runtime had been killed.
- **Cause:** the lock that keeps two runtimes off one execution (`flock`, D-039) belongs to the open lock file. When a process forks, the child gets a copy of every open file, including that one. The copy is closed when the child turns into another program (`exec`), but between `fork` and `exec` the child holds the lock as well. The B-007 fix added a step that runs in exactly that gap, and adding it makes Rust start the child with plain `fork` instead of a faster path in which the child runs at once. On a busy machine a forked child can wait several milliseconds for its first turn on a CPU. If the runtime was killed in that time, the runtime was dead, but its not-yet-started child still held the lock, and recovery (started immediately by the harness) was refused. When the child finally ran, it noticed its parent was gone and exited, releasing the lock.
- **Reproduced:** on the development machine by running the harness with 64 workers instead of 8: 4 violations in 1,440 runs with the B-007 fix, 0 in 2,880 at tag `v0.1` (`docs/results/b008-stress.md`).
- **Fix:** `runtime::lock_execution` retries for up to 3 seconds when the lock is held, before reporting "in use" (D-071). A runtime that is really alive still holds the lock after 3 seconds, so the protection is unchanged; only the error for that case comes later. New test: `recovery_waits_for_a_lock_that_is_about_to_be_released`.
- **Lesson:** a fix is a change, and a change in crash-handling code needs the same full run as the original. The local 200-kill run after B-007 was clean; it took a slower, busier machine to show the new window. Also a fact about Unix worth keeping: `fork` copies open files, so anything tied to an open file (a lock, a pipe end) is briefly shared with every child, whatever close-on-exec says.

## B-009 — a sandboxed command could crash the run with a file the runtime cannot handle
- **Found:** 2026-11-13, by hand, while checking edge cases after v0.1 and M7. No suite covered it. Fixed 2026-11-14.
- **What was done:** one `exec` step whose code leaves an awkward file in `/work`, then any second step. Three variants, each on `examples/demo-repo` with the `nix` preset plus `["python3"]` in `exec.allow`:
  1. a file whose name is not valid UTF-8 (`open(b"/work/src/bad\xff.py", "w")`);
  2. a file with no permission bits (`chmod 0` on `src/locked.py`);
  3. a folder with no permission bits that holds a file (see B-010, which is worse).
- **Symptom:** the run stops with a runtime error instead of a tool result: `Error: non-UTF-8 name in …/work/src` (1), `Error: cp -a … failed: cp: cannot open '…/work/src/locked.py' for reading: Permission denied` (2). The call has an intent and no COMPLETED or FAILED record.
- **Cause:** after an `exec`, the runtime walks the workspace (`checkpoint::scan`, for the D-009 revert) and then copies it (`checkpoint::create`). Both treat any error as "the runtime itself broke" and return it up through `Gateway::handle`, which ends the run. But the workspace was shaped by untrusted code, so these errors are the command's doing, not the runtime's.
- **How bad:** not an escape. Nothing leaves the sandbox and the original repo is untouched. For variants 1 and 2, `agentos recover <id>` works: it rolls the workspace back to the last checkpoint and marks the call NOT APPLIED. What is lost is the run: hostile or merely odd code in the repository (a test that makes a `chmod 000` fixture, a build step that writes a Latin-1 file name) stops the agent, and the agent is never told why.
- **Fix:** a failure of the post-`exec` bookkeeping is now a failure of the call: the workspace is rolled back to the last checkpoint, `tool.failed` is written with the reason, and the agent gets it (`gateway.rs::undo_exec`, D-085). New tests: the four `…_fails_the_call_not_the_run` tests in `tests/hardening.rs`.
- **Lesson:** "runtime error" and "tool error" were split by *where* the code runs, but the right split is *who caused it*. Trusted code that reads untrusted data has to expect any shape of data.

## B-010 — a sandboxed command could make an execution impossible to recover
- **Found:** 2026-11-13, by hand, same session as B-009. Fixed 2026-11-14.
- **What was done:** one `exec` step: `os.mkdir("/work/src/d")`, write a file into it, `os.chmod("/work/src/d", 0)`. The same with a folder outside the write globs (`/work/docs`).
- **Symptom:** the run stops with `Error: reading …/work/src/d … Permission denied`. Then `agentos recover <id>` and `agentos run --resume <id>` both fail with `Error: Permission denied (os error 13)`, every time. A folder `work.restore-tmp` is left next to `work/`.
- **Cause:** recovery rolls back by swapping a fresh copy of the checkpoint into place and then deleting the old workspace (`checkpoint::restore`, `remove_dir_all`). The swap works, so `work/` is in fact restored. Deleting the old tree fails: nobody without root can list or empty a folder with mode `000`, not even its owner, until the mode is changed back. Every later attempt starts by deleting that leftover (`restore`, `remove_leftovers`) and fails at the same place.
- **How bad:** not an escape, but it breaks the claim that an interrupted execution can always be recovered: code inside the sandbox decides whether recovery works. Before the fix the only way out was by hand: `chmod -R u+rwx <execution>/work.restore-tmp`, then `agentos recover <id>`.
- **A second form, found while fixing:** a read-only folder (mode `555`) holding a file can be copied, so the call completes and the folder is in a checkpoint. Any later rollback, or the removal of an orphan checkpoint, then failed the same way. No hostile intent is needed for this one.
- **Fix:** `checkpoint::remove_tree` deletes such trees: if the plain delete fails, it gives the owner read, write and search permission on every folder in the tree (never following a symlink) and deletes again (D-086). With D-085, a `chmod 000` folder also no longer survives the call that made it. New tests in `tests/hardening.rs`: `recovery_works_after_a_kill_while_the_workspace_holds_a_locked_folder`, `recovery_works_when_a_checkpoint_holds_a_read_only_folder`.
- **Lesson:** the fault-injection harness kills the runtime at chosen points but always with a well-behaved workspace. Recovery also has to survive a workspace made by an enemy. The 32 security tests ask "did anything get out?"; none of them asks "can the run still be recovered afterwards?".

## B-011 — a relative state folder made every run fail at its last step
- **Found:** 2026-11-14, by hand, while looking at the demo's output with `AGENTOS_STATE_DIR=target/tmp/demo-look`. Fixed the same day.
- **Symptom:** all tool calls worked; then `Error: diff failed: diff: target/tmp/…/checkpoints/cp-0000: No such file or directory`. No patch was written and the execution was left unfinished.
- **Cause:** the patch is made by running `diff` from inside the execution folder (`checkpoint::write_patch` sets the working folder, so that the paths in the patch are short). The two folders to compare were passed as given. With a relative state folder they were relative to where `agentos` was started, and `diff`, started somewhere else, looked for them in the wrong place.
- **Why no suite saw it:** every test passes an absolute `--state-dir`, and the default (`~/.local/share/agentos`) is absolute.
- **Fix:** the state folder is made absolute once, in `main.rs`, before anything uses it. New test: `a_relative_state_dir_works` in `tests/hardening.rs` (both `--state-dir` and `AGENTOS_STATE_DIR`).
- **Lesson:** a path is only meaningful together with the folder it is relative to. Anything that starts another program in another folder must hand it absolute paths, and the cheapest place to make them absolute is where they enter the program.

## B-012 — an empty new file was silently missing from the patch
- **Found:** 2026-11-14, in the first trial on a project that is not the demo (`docs/results/real-repo-trial-nix.md`). Fixed the same day.
- **What was done:** the agent added a Python package: `src/inventory/alerts/__init__.py` (empty) and `low_stock.py`. The run completed, the tests passed in the sandbox, the patch applied with `git apply` without a message.
- **Symptom:** after applying, `src/inventory/alerts/` held `low_stock.py` and no `__init__.py`. The run had printed no `NOT in the patch` line.
- **Cause:** for a new file with nothing in it, `diff -N` prints its header line and no lines of content, and `patch` and `git apply` create nothing from that. D-090 had listed four things a unified diff cannot carry (binary files, modes, empty folders, symlinks) and missed this fifth one.
- **Why it matters:** an empty `__init__.py` is how most Python packages start. Here the tests still passed after applying, because Python accepts a folder without it as a "namespace package"; with other tools (packaging, some linters, older layouts) the package would be broken, and nothing would have said why.
- **Fix:** `checkpoint::write_patch` counts new empty files (and deleted empty files) in `LeftOut.empty_files` and writes `# agentos: empty new file not in this patch: <path> (create it: touch <path>)`. `tests/hardening.rs::the_patch_lists_what_it_cannot_carry` covers it and checks that `patch` indeed does not create the file.
- **Lesson:** "what can this format not carry?" was answered from memory in D-090, and the answer was one item short. A list like that should come from trying each kind of change, which is what the real project did by accident.

## B-013 — the new startup check broke every program that uses AgentOS as a library
- **Found:** 2026-11-14, by `make bench`, one day's work after the check was written (D-093) and after it had been pushed and passed CI on both jobs. Fixed the same day. **Introduced by D-093.**
- **Symptom:** the `fs.write` benchmark stopped with `the AgentOS sandbox cannot start: user namespaces cannot be used (… panicked at benches/fs_write.rs …)`. The benchmark script hides a failed step, so the first visible sign was a missing data file at the very end.
- **Cause:** the check started "the program I am" with the hidden argument `__probe-userns` and looked at its exit code. That is right when the program is `agentos`, whose `main` knows the argument. A benchmark, a test binary or any other program that links the library does not: it ran *itself* again from the top, failed for its own reasons, and the check read that as "user namespaces do not work".
- **Why no suite saw it:** every integration test drives the `agentos` binary. Nothing in `make test` creates an execution from another program; only the benchmarks do, and neither `make test` nor CI runs them.
- **Fix:** the probe no longer starts a program. Its steps run in a forked child between `fork` and `exec` (`Command::pre_exec`), with raw system calls only, and the child leaves with the number of the failed step (`sandbox/ns.rs::probe_in_child`, D-096). New test: `tests/hardening.rs::the_startup_check_works_from_a_program_that_is_not_agentos` (the test binary is such a program).
- **Lesson:** "start myself again" is a trick that belongs to a binary, and it was put into a library function. The existing `__sandbox-init` helper has the same shape but takes the binary's path from the caller on purpose. Also: a script that swallows a failed step (`|| true`) turns a clear error into a puzzle three steps later.

## B-014 — on Ubuntu, an *active* virtualenv got the "not active" note
- **Found:** 2026-11-15, by CI run 43690755406, the first run of M9 and M10 on Ubuntu 24.04. Both jobs failed in `tests/deps.rs::a_mounted_virtualenv_that_is_not_active_gets_a_note`. Fixed the same day. **Introduced by D-101.**
- **Symptom:** with the virtualenv activated, agentos still printed `note: sandbox.ro_mounts has /usr, which holds its own pytest (/usr/bin/pytest), but the pytest found on PATH is …/venv/bin/pytest … activate it`. The note pointed at `/usr`, which is not a virtualenv, and told the user to do what they had done.
- **Cause:** the note's test was "a mounted folder has `bin/<program>`, and the program found on `PATH` is a different file". On Ubuntu `/usr` is one of the policy's read-only mounts and `/usr/bin/pytest` exists, so the test is true for every user with an active virtualenv: the virtualenv's `pytest` is found first, as it should be, and `/usr/bin/pytest` is "a different file".
- **Why it passed locally:** on NixOS the only shipped mount is `/nix/store`, which has no `bin` folder. The case could not occur on the development machine.
- **What else the failure hid:** `cargo test` stops at the first test file that fails. On the main job, `tests/external.rs`, `hardening.rs`, `llm_agent.rs`, the recovery and security suites, `make demo` and the 200-kill step did not run; on the AppArmor job the security and vertical-slice suites did not run. So this run says nothing about them, good or bad. `tests/apply.rs` (6) and `tests/benign.rs` did pass on Ubuntu.
- **Fix:** the note is given only when the mounted folder's `bin` is **not on `PATH` at all**, which is what "not activated" means. A second copy of a program that is on `PATH` and merely comes later is normal (D-109). New test, which reproduces the Ubuntu shape on any system with two mounted folders: `a_second_copy_of_a_program_later_on_path_is_not_worth_a_note`. With the fix taken out it fails with the CI's message.
- **Lesson:** D-101's rule was written from the one layout at hand. "Both presets must pass" is in the project rules for exactly this reason, and the `fhs` preset is only ever exercised by CI. A warning has to be tested for when it must stay silent as carefully as for when it must speak.

## B-015 — a valid file call could fail at random with "Try again" on a busy machine
- **Found:** 2026-11-26, by CI run 45726709554 (job `fhs`, Ubuntu 24.04): `tests/rodirs.rs::the_file_tools_read_it_from_the_repository_and_never_write_it` got `` `vendor/out`: Try again `` (EAGAIN) where it expected a denial. Fixed the same day. Present since M2: every `openat2` call had it.
- **Symptom:** a file tool call on a path whose resolution goes through `..` (written by the agent, or inside a symlink) failed now and then with `Try again`. It failed closed (an error, never an escape), so no confinement was lost; but a correct call could fail, and recovery could fail in the same way.
- **Cause:** `openat2` with `RESOLVE_BENEATH` returns `EAGAIN` when a rename or a mount *anywhere on the system* happens while it resolves a `..`: it cannot then prove the path stayed beneath its root. The manual says the caller may retry. norn never did. A CI runner with many sandboxes (each with its own mounts) running at once makes the race common enough to see.
- **Reproduced:** a unit test that opens `a/b/../f.txt` 20,000 times while another thread renames a file elsewhere: 133, 133 and 145 failures in three runs without the fix; 0 in five runs with it (`beneath::tests::a_rename_elsewhere_never_makes_an_open_fail`).
- **Fix:** `beneath::open_beneath` retries on `EAGAIN`, up to 1,000 times. A retry resolves the whole path again under the same flags, so it is exactly as confined.
- **Probably also:** the unexplained failure of `toctou_symlink_swap_during_writes_and_reads_never_escapes` in the AppArmor job of CI run 45723220958 ("the run did not finish even after 400 restarts", all within seconds). That test renames names back and forth without pause through symlinks that contain `..`, so recovery's own opens would get `EAGAIN` nearly every time. Not proven; to be confirmed by later CI runs.
- **Lesson:** a system call's error list is part of its contract. `EAGAIN` from `openat2` was in the manual from the start; the suites missed it because a single test machine rarely races itself.
