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
