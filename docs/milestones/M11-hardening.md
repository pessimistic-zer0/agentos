# M11 — Hardening for v1.0

| | |
|---|---|
| Weeks in the plan | v1.0 (D-114); no week plan |
| Started / finished | 2026-11-16 / 2026-11-16 |
| Commits | `8628ac0` (scope) … `fc31fbb` (tag `m11`, D-121); the 200-kill results and the last lines of this document are in `10cab11` and after |
| Status | Done |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it. Fill it in while working.

## 1. Aim

Three items from the v1.0 scope (D-114) that make the sandbox harder to misuse and cheaper to run. **A1:** a seccomp filter, so a sandboxed command can no longer reach parts of the kernel it has no use for. **A2:** a disk limit for the workspace. **A3:** build and cache folders (`filesystem.scratch`) kept out of the checkpoints. A1 and A2 touch the **sandbox**; A3 touches **checkpoints + recovery**.

Before the first item, CI on `main` was red: the TOCTOU race test's own check that the race happened often enough failed on the Ubuntu runner (D-115, §8).

## 2. Requirements

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| CI's red TOCTOU test fixed without lowering a bar | Yes, both presets (`fhs`: CI run 43844369387, D-119) | D-115; `cargo test --test security toctou` passes in 1 attempt (7,022 swaps); slowed by hand: passes in 3 attempts at 5 ms, fails after 5 at 60 ms |
| A1: namespaces, mounts, `bpf`, `ptrace`, `io_uring`, `perf_event_open`, key rings refused inside the sandbox | Yes, both presets (CI run 43844369387, D-119) | `seccomp_refuses_namespaces_mounts_and_kernel_interfaces`: fails before the change (`unshare-user OK`, `unshare-mount OK`, `ptrace OK`), passes after |
| A1: real test commands still work | On `nix` yes | Experiment `m11-seccomp`; `agentos run` with the second trial's Rust and npm policies gives the trial's results; `make test` 133 passed |
| A2: a command cannot take the workspace past `limits.workspace_mb` and leave it there | Yes, both presets (`fhs`: CI run 43849008533, D-121) | `tests/hardening.rs`: `a_command_that_fills_the_workspace_is_stopped_and_undone`, `one_file_cannot_grow_past_the_limit`, `fs_write_past_the_limit_writes_nothing`, `a_repository_larger_than_the_limit_is_refused_at_the_start`; all four fail with the limit switched off by hand. It is a measured limit, not a hard cap (D-117) |
| A3: no checkpoint holds a scratch path; the workspace keeps them | Yes, both presets (CI run 43849008533, D-121) | `a_cache_is_scratch_and_an_example_env_file_is_not_a_secret` (fails with the change switched off); unit test in `src/checkpoint.rs`; the Rust trial session: 86 MB before, 21 MB after |
| The five recovery invariants still hold with the new code on the killed paths | On `nix` yes | `make eval-recovery`: 4,800 runs, 0 violations, 0 duplicated effects, recovery p50 20.3 ms, p99 61.1 ms; external cells 1,600 runs, 0 violations (`docs/results/recovery-200-nix-m11.md`) |
| Both presets pass | `nix` yes; `fhs` yes for the TOCTOU fix and A1 (CI run 43844369387 on `caeb948`); A2 and A3 by CI run 43849008533 on `fc31fbb` (D-121): 138 tests, `make demo`, the 200-kill step with 0 violations, and the AppArmor job | `gh run view 43849008533` |

## 3. What you need to know first

**System call.** A program cannot touch a file, start a process or open a network connection by itself; it asks the kernel, through a numbered request called a system call (`openat` is 257 on x86_64, `unshare` is 272). Everything a sandboxed command does goes through these.

**seccomp.** A process can attach a filter to itself that the kernel runs on every one of its system calls, before doing the work. The filter sees the call's number and its first six arguments (`struct seccomp_data`) and answers "allow", "fail with this error code", or "kill". The filter is inherited by every child and survives `execve`; it can never be removed, only made stricter. An unprivileged process may install one only after setting `no_new_privs` (so a setuid program cannot be tricked by a filter into misbehaving), which the sandbox already did in M2.

**Classic BPF.** The filter is written in a tiny instruction set with one register: *load* a word from `seccomp_data`, *compare and jump forward*, *return* a verdict. No loops are possible, so the kernel can check it is safe. It cannot follow pointers: an argument that is an address of a struct in memory is invisible to it.

**Deny list or allow list.** A deny list names what is refused; an allow list names what is allowed. Docker's default profile is an allow list of about 350 calls. AgentOS uses a deny list (why: D-116).

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `experiments/m11-scratch-checkpoints/` | Three ways to checkpoint a 14 MB workspace that is all build folder: as today, copy then delete, move aside then copy | 14 MB per checkpoint as today, 1 MB with either; move-aside is faster without shared blocks | Both fix the size; copy-then-delete was chosen because it never touches the workspace (D-118) |
| `experiments/m11-disk-limit/` | Timed a size walk; raced a 2.3 GB/s writer against a watcher; tried `RLIMIT_FSIZE` | Walk: 2 µs per entry. Overshoot 20 to 40 MB at 20 to 100 ms. `RLIMIT_FSIZE` stops one file exactly | Measuring is cheap enough; the overshoot is speed × interval and must be stated; the per-file cap is worth adding |
| `experiments/m11-seccomp/` | The planned filter, by hand with `libc`, run on its own and in front of cargo, node, pytest, Python's threads, subprocess and multiprocessing, a shell pipe | Refused calls give `EPERM`, `clone3` `ENOSYS`; every toolchain gives the same results as without the filter | A deny list breaks none of the trial projects; `clone3` must say `ENOSYS`; no new crate is needed |

**Measured limit.** The kernel can cap a process's memory (cgroups) but, without root, not a folder's size. So the runtime looks: it adds up the size of every file in the workspace again and again while the command runs. What happens between two looks is not prevented, only caught at the next one. The one thing the kernel does cap without root is the size of a single file a process writes (`RLIMIT_FSIZE`).

## 5. What was built

| File | Purpose |
|---|---|
| `src/sandbox/seccomp.rs` | The list of refused calls, the BPF program, `install()`, two unit tests |
| `src/sandbox/ns.rs` | `init` (the sandbox's PID 1) installs the filter right after Landlock |
| `src/checkpoint.rs` | `create` takes a scratch test; `drop_scratch` empties scratch paths from the temporary copy; a unit test |
| `src/recovery.rs` | Loads the saved policy when it must take a checkpoint itself |
| `src/policy.rs` | `limits.workspace_mb` (optional, default 2048) |
| `src/sandbox/mod.rs` | `disk_usage`, `DiskLimit`, `Stopped`; the watchdog thread now also measures |
| `src/gateway.rs` | A command over the limit is rolled back and FAILED; `write_fits` for `fs.write` |
| `src/runtime.rs` | An over-size repository copy is refused at the start and by `check` |
| `tests/hardening.rs` | Four disk-limit tests |
| `tests/security/main.rs` | `seccomp_refuses_namespaces_mounts_and_kernel_interfaces`; the TOCTOU test split into `toctou_attempt` + a loop (D-115) |

Where the filter sits in the start of a command (from `ns.rs`):

```text
H  helper: joins the cgroup, unshare(user, mount, pid, net)
└─ C  PID 1: new root, pivot_root, loopback, drop capabilities,
      no_new_privs, Landlock, seccomp            <- new, last step
      └─ G  execve(the allowed program)          inherits the filter
```

## 6. The code, explained

**`seccomp::program()`** builds the filter as a `Vec<libc::sock_filter>`. Each `sock_filter` is one instruction: `code` (what to do), `jt`/`jf` (how many instructions to skip if a comparison is true/false), `k` (the constant).

```rust
load(OFF_ARCH),
jeq(AUDIT_ARCH, 1, 0),
ret(libc::SECCOMP_RET_KILL_PROCESS),
load(OFF_NR),
```

First the architecture. A 64-bit kernel also accepts 32-bit system calls, which have *different numbers*: 272 might mean something else there. If the architecture is not ours, skip nothing and kill; if it is, skip the kill. Then load the call number. On x86_64 one more pair refuses the x32 ABI, which reuses the 64-bit numbers with bit 30 set.

```rust
for nr in denied() {
    p.extend([jeq(nr as u32, 0, 1), ret(errno(libc::EPERM))]);
}
```

For each refused call: if the number matches, fall through to "return EPERM"; if not, skip that return and test the next one. `errno(e)` packs the error code into the verdict (`SECCOMP_RET_ERRNO | e`).

```rust
p.extend([jeq(libc::SYS_clone3 as u32, 0, 1), ret(errno(libc::ENOSYS))]);
p.extend([
    jeq(libc::SYS_clone as u32, 0, 3),
    load(OFF_ARG0_LO),
    jset(NS_FLAGS, 0, 1),
    ret(errno(libc::EPERM)),
    ret(libc::SECCOMP_RET_ALLOW),
]);
```

Starting a process or a thread uses `clone` or `clone3`, and both can also ask for new namespaces. `clone`'s flags are its first argument, a plain number, so the filter loads it and refuses it if any namespace flag is set (`jset` tests "any of these bits"). `clone3`'s flags sit in a struct in memory, which BPF cannot read. Refusing it with `EPERM` would break every program; answering `ENOSYS` ("this kernel does not have it") makes glibc and Rust's standard library retry with `clone`, which the filter can inspect. The last instruction allows everything that got this far.

**`seccomp::install()`** hands the program to the kernel with `prctl(PR_SET_SECCOMP, SECCOMP_MODE_FILTER, &prog)`. The kernel copies it, so the `Vec` may be dropped afterwards. This is the first `unsafe` in the file; its comment says what it relies on.

**Why in C and after Landlock.** Everything before it (mounting, `pivot_root`, Landlock's own system calls) uses calls the filter refuses, so it must be last. C then only forks, waits and writes its status, none of which are refused.

**The test.** A Python script calls each refused system call through `ctypes` with harmless arguments and prints the error name. It requires `EPERM` exactly, because several of these calls fail anyway without a filter (bad arguments give `EFAULT` or `EINVAL`), and "it raised" would prove nothing. `PTRACE_TRACEME` runs in a forked child: if it succeeds (no filter), the process is traced by the sandbox's init and stops at its next signal, which hung the first version of the test for 20 s.

**The watchdog loop (`sandbox::run`).** One thread already waited for "the command ended" or "the time limit passed". It now wakes every `interval`, calls `disk_usage`, and kills the cgroup if the result is over. `interval` starts at 50 ms and becomes ten times the duration of the last walk if that is longer, so measuring never takes more than about a tenth of a core. `recv_timeout` on a channel is how a Rust thread sleeps *until either* a message arrives *or* a time passes.

**`disk_usage`** adds `st_blocks × 512` (what a file really uses on disk; a "sparse" file can claim more than it uses) and does not follow symlinks. Errors are skipped: files vanish while a command runs, and this is a measurement, not a record.

**`Gateway::run_exec`** measures once more after the command ends, because it may have ended between two looks. Over the limit either way: restore the last checkpoint, fail the call. That keeps the rule that FAILED means "nothing changed".

**`checkpoint::drop_scratch`.** After `cp -a` has filled `.tmp-cp-000N`, the copy is scanned (the same `scan` the revert uses; it never follows a symlink) and every path the policy calls scratch is deleted from it. The scan is sorted, so a folder comes before its contents; once a folder is deleted its contents are skipped. Only then is the copy renamed to its real name. Because all of this happens on the copy, a kill at any moment leaves what it always left: a `.tmp-` folder that recovery deletes.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-114 | The v1.0 scope: eleven items, milestones M11 to M14 | A smaller list; hardening and release only |
| D-115 | The TOCTOU test repeats the run (up to five) and adds the counts; bars unchanged | Lowering the bar; spinning instead of sleeping; rerunning CI |
| D-118 | Scratch paths deleted from the checkpoint copy before it is renamed into place | Moving caches aside; a Rust copier; mounting; carrying caches across a rollback |
| D-119, D-121 | CI runs 43844369387 and 43849008533 verify M11 on `fhs`; `m11` is tagged on the commit CI ran | Tagging a later, unrun commit |
| D-117 | `limits.workspace_mb`: measured every 50 ms, `RLIMIT_FSIZE` per file, the command undone when over | tmpfs, quotas, loop image (ADR 0010); checking only at the end; counting checkpoints |
| D-116 | seccomp as a hand-written deny list in C after Landlock; `EPERM`, `ENOSYS` for `clone3` | Allow list; a seccomp crate; killing; a policy switch; installing in G |

## 8. Problems hit

1. **CI red before any v1.0 work** (run 43742105191, `9cec493`). The TOCTOU test failed its own precondition: 925 swaps, under 1,000, with 136 calls inside and 164 refused, so the race did happen. Not reproducible locally even on one CPU. Cause: the swapper's 150 µs sleep lasts longer on the CI runner. Fixed by repeating the run (D-115), commit `26ebc31`. Findings row 2026-11-16.
2. **The seccomp test hung for 20 s on its first run without the filter.** `PTRACE_TRACEME` succeeded and stopped the Python process at the next signal. Moved into a forked child.
3. **Without the filter, `clone(CLONE_NEWUSER)` already gave `EPERM`** while `unshare(CLONE_NEWUSER)` worked. Not looked into; the test needs `EPERM` for both, and the `unshare` case is the one that showed the gap.

4. **Commit `048e7e5` has four failing tests.** The disk-limit tests' helper required that the preset has no `workspace_mb` line; the presets got one as the last edit, and the commit was made before the re-run's result was read. Fixed in the following commit. Findings row 2026-11-16.

## 9. How to run it

```sh
nix develop -c make test                     # 138 passed on nix
systemd-run --user --scope -p Delegate=yes cargo test --test security seccomp -- --nocapture
cd experiments/m11-seccomp && nix develop ../.. -c cargo build --offline && target/debug/m11-seccomp selftest
```

## 10. What this milestone does not do

- The filter is a deny list. Every call not on it is allowed, which is more kernel surface than an allow list leaves.
- The disk limit is measured: a fast writer passes it by about speed × 50 ms before it is stopped. It bounds the workspace, not the execution: checkpoints add up and are never pruned.
- After a rollback the caches are gone; the next build starts cold. Without shared blocks (ext4) the cache is still copied once per checkpoint before being deleted, which costs time, not space.
- Checkpoints are still never pruned.
- Only x86_64 has been built and run; the aarch64 numbers come from `libc` and are untested.

## 11. Check your understanding

1. What is a system call, and why can a seccomp filter see `clone`'s flags but not `clone3`'s?
2. Why does the filter answer `clone3` with `ENOSYS` and not `EPERM`?
3. What is the difference between a deny list and an allow list, and what does AgentOS give up by using a deny list?
4. Why must the filter check the architecture first?
5. Why is the seccomp filter installed after Landlock, and why in the sandbox's init and not in the command?
6. The seccomp test asks for `EPERM` exactly. What would be wrong with "the call raised an error"?
7. Why can a folder's size not be capped without root in a way that survives `kill -9`? (ADR 0010)
8. How far past `workspace_mb` can a command get, and what decides it?
9. What does `RLIMIT_FSIZE` cap, and what does it not?
10. Why is a command that went over the limit undone and not just stopped?
11. Why are scratch paths deleted from the copy and not moved out of the workspace first?
12. The TOCTOU test failed on CI with nothing wrong in the runtime. What was wrong, and why was lowering the bar not the fix?
