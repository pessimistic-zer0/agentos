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
