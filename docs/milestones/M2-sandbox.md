# M2 — Sandbox

| | |
|---|---|
| Weeks in the plan | 3–5 |
| Started / finished | 2026-11-11 / 2026-11-11 |
| Commits | `11b4863` … the commit that adds this file (tag `m2`) |
| Status | Done with gaps (the `fhs` preset is not yet verified; see §2) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Put the cage around `exec`, and make the file tools escape-proof. After M1, a command run by the agent could read and write anything the user could. After M2, a command sees only a copy of the repo at `/work` plus read-only system folders, has no network, no home folder, no special rights, and limited memory, CPU and process count. The file tools now let the kernel refuse any path that leaves the workspace. This milestone is the **sandbox** part, and it tightens the **gateway** and **policy** parts (write globs now bind code run by commands).

## 2. Requirements

Exit criterion from the v1 table: *"20 adversarial cases, 0 escapes."*

All commands run inside `nix develop`. Suite command: `systemd-run --user --scope -p Delegate=yes cargo test`.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| ≥ 20 adversarial cases, 0 escapes (`nix` preset) | Yes | `cargo test --test security` → `25 passed; 0 failed`. Passed on six consecutive full runs |
| Same on the `fhs` preset | **Not yet verified** | Needs the Ubuntu CI run, which happens only after Prince pushes (D-027). `AGENTOS_TEST_PRESET=fhs` selects it |
| User, mount, PID, network namespaces | Yes | Probe inside the sandbox: pid 2, processes `[1, 2]`, 11 mounts, "Network is unreachable"; tests in groups 2, 4, 6 |
| cgroup v2 limits: memory, pids, cpu; watchdog kills the cgroup | Yes | `fork_bomb_is_stopped_by_pids_max`, `memory_exhaustion_is_stopped_by_memory_max`, `infinite_loop_is_killed_at_the_time_limit` |
| Landlock | Yes | `exec_cannot_mount_even_in_a_nested_user_namespace`; probe: `os.listdir("/")` → Permission denied |
| `openat2` for the file tools | Yes | 7 tests in group 1; unit test `escapes_are_refused_by_the_kernel` |
| Environment scrubbed (PATH, HOME, LANG only) | Yes | `exec_sees_no_host_secrets_in_environment_or_proc`: a secret in the runtime's environment is absent inside and absent from the whole execution folder |
| Capabilities dropped, `no_new_privs` | Yes | `exec_has_no_capabilities_and_cannot_gain_root` |
| Denied file unreadable from code run by a command (D-008) | Yes | `exec_cannot_read_a_denied_file`, `conftest_cannot_read_a_denied_file` |
| Changes outside write globs reverted and logged (D-009) | Yes | `exec_changes_outside_write_globs_are_reverted`, `exec_cannot_replace_a_folder_with_a_symlink_to_escape_the_revert` |
| Log and checkpoints invisible in the sandbox (rule 9) | Yes | `exec_cannot_read_or_change_the_event_log_and_checkpoints`, `fs_tools_cannot_reach_log_or_checkpoints` |
| Both presets exist; no distro assumption in `src/` | Yes | `policies/nix.toml`, `policies/fhs.toml`; a wrong preset stops with: `allowed program pytest is at /nix/store/…, which is not under any sandbox.ro_mounts entry (/usr, /bin, /lib, /lib64); use the policy preset for this system` |
| Startup checks with a clear message | Yes | Run without `systemd-run`: stops before creating anything with `this machine cannot run the AgentOS sandbox: - cgroup delegation: … run agentos under a delegated cgroup, e.g. systemd-run --user --scope -p Delegate=yes agentos run ...` |
| M1 behaviour still works in the sandbox | Yes | `cargo test --test vertical_slice` → 4 passed (the demo is fixed inside the sandbox) |
| The four experiments were run first | Yes | `experiments/m2-*`, each with `NOTES.md` |
| Each commit builds | Yes | Each of the three commits built in a clean `git worktree` |

Not met: the `fhs` row. It is the reason for "Done with gaps". Nothing was faked or skipped to pass. The TOCTOU race test and the benign suite are planned for M5 by the v1 table and are not part of this exit criterion.

## 3. What you need to know first

**Namespace.** A private view of one part of the system for a group of processes. Four are used: *user* (a private table of user ids, in which the creator holds all "capabilities"), *mount* (a private list of what is mounted where), *PID* (a private numbering of processes, starting at 1), *network* (a private set of network interfaces; a new one has only a loopback that is down).

**A new mount namespace hides nothing.** It starts as a copy. `experiments/m2-unshare` showed `~/.ssh` was still readable. Files disappear only when the process's root folder is replaced.

**`pivot_root`.** Swaps what a process sees as `/`. After the swap, the old root is detached, and there is no path back to it.

**Bind mount.** Making an existing folder appear at a second place. The workspace on the host appears at `/work` inside.

**Capabilities.** Root's power, cut into about 40 pieces. A process that creates a user namespace gets all of them *inside that namespace*: enough to mount things there, not enough to touch the host. The sandbox uses them for setup and then drops them all.

**`no_new_privs`.** A one-way switch: from now on this process and its children can never gain rights, for example by running a setuid program.

**cgroup v2.** A tree of folders under `/sys/fs/cgroup`. Each folder is a group of processes; files in it set limits (`memory.max`, `pids.max`, `cpu.max`). "No internal processes": a group that passes controllers to child groups may not contain processes itself. *Delegation* means systemd gives a user ownership of one subtree.

**Landlock.** A kernel feature by which a process gives up its own file access, permanently, except for an allow-list. Children inherit it.

**`openat2` with `RESOLVE_BENEATH`.** Open a path starting from a folder handle, and let the kernel refuse if any step leaves that folder. Check and open happen in one call, which is what defeats TOCTOU races.

**File descriptor (fd).** A number that stands for an open file or folder. Holding an fd means holding the thing itself, not its name: renaming or replacing the path afterwards does not change what the fd points to.

Glossary: `docs/decision-log.md`.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m2-unshare` | `unshare -Urmpn --fork --mount-proc sh` | uid 0, pid 1, all capabilities, no network; **`~/.ssh` still readable**; `/sys` still showed host network cards | Namespaces alone hide no files; build a new root; mount no `/sys` |
| `m2-pids` | Fork bomb under `pids.max=32` in a delegated scope | 31 forks ok, 14922 refused; `cgroup.kill` emptied the group | Limit works; controllers can be enabled only after moving processes to a leaf |
| `m2-openat2` | 8 paths with `openat` vs `openat2(RESOLVE_BENEATH)` | `openat` opened all 8; `openat2` refused the 5 escapes with `EXDEV` | The kernel check catches what a text check cannot; `nix` already wraps `openat2` |
| `m2-landlock` | The `landlock` crate with a read-only and a read-write folder | Everything else refused, including `/dev/null`, which broke spawning a child | List the device files explicitly |

Also read first: bubblewrap's `main()` (the order of `unshare`, uid maps, `pivot_root`), as the plan asks.

## 5. What was built

| File | Purpose |
|---|---|
| `src/sandbox/mod.rs` | `run`: start the helper, watchdog, collect output and status; `check_host`: startup checks |
| `src/sandbox/ns.rs` | The helper: namespaces, uid map, new root, loopback, drop privileges, exec; PID-1 init |
| `src/sandbox/cgroup.rs` | Delegated cgroup setup, per-exec groups with limits, `cgroup.kill`, cleanup |
| `src/sandbox/landlock.rs` | The Landlock allow-list; kernel ABI query |
| `src/sandbox/beneath.rs` | `openat2` wrappers; fd → real path; text cleanup for policy matching only |
| `src/tools/fs.rs` | Rewritten on `openat2` and file handles |
| `src/tools/exec.rs` | Now only program lookup (must be under a sandbox mount) and output capture |
| `src/gateway.rs` | `decide` opens paths and checks the resolved path; `run_exec` uses the sandbox, then reverts |
| `src/checkpoint.rs` | `revert_outside` (D-009); patch notes for symlinks |
| `src/runtime.rs`, `src/main.rs` | `prepare_sandbox` at startup; the hidden `__sandbox-init` entry |
| `tests/security/main.rs` | 25 adversarial tests |
| `docs/bugs.md` | Four bugs the suite found in the runtime |
| `experiments/m2-*` | The four experiments |

One `exec` call, end to end:

```text
runtime (many threads)                         kernel objects
  gateway.run_exec
   ├─ cgroup: mkdir x-<exec>-c<call>, write memory.max / pids.max / cpu.max
   ├─ spawn  agentos __sandbox-init <json>     (stdout/stderr pipes, status pipe on fd 3)
   │     H: write 0 > cgroup.procs             joins the cgroup
   │        unshare(user|mount|pid|net)        new namespaces
   │        write setgroups, uid_map, gid_map
   │        fork ─► C (PID 1): build_root → pivot_root ×2 → lo up
   │                  drop capabilities, no_new_privs, Landlock
   │                  fork ─► G: execve(pytest …)   ← the command
   │                  reap children; write "exit N" to fd 3
   ├─ watchdog thread: at call_timeout_s → write 1 > cgroup.kill
   ├─ wait; read output; destroy the cgroup (kills leftovers)
   └─ revert_outside(work, checkpoint, write globs) → policy.violation event
```

## 6. The code, explained

### 6.1 `beneath.rs` — the kernel keeps paths inside

```rust
pub fn open_beneath(root: impl AsFd, path: &str, flags: OFlag, extra: ResolveFlag) -> nix::Result<OwnedFd> {
    let flags = if flags.contains(OFlag::O_PATH) { flags | OFlag::O_CLOEXEC } else { flags | OFlag::O_CLOEXEC | OFlag::O_NOCTTY };
    let how = OpenHow::new().flags(flags).mode(Mode::empty())
        .resolve(ResolveFlag::RESOLVE_BENEATH | ResolveFlag::RESOLVE_NO_MAGICLINKS | extra);
    openat2(root, if path.is_empty() { "." } else { path }, how)
}
```

`root` is a handle on the workspace folder. `RESOLVE_BENEATH` makes the kernel fail with `EXDEV` if resolution would step outside it. `RESOLVE_NO_MAGICLINKS` refuses `/proc/…/root`-style jumps. `OwnedFd` is Rust's type for "a file descriptor I own"; it is closed automatically when dropped. `impl AsFd` means "anything that can lend a file descriptor".

The first line is bug B-004: `openat2` rejects `O_PATH` combined with `O_NOCTTY`, where plain `open` would ignore it.

`fd_path` reads `/proc/self/fd/N` to learn where an opened handle really is. That is reading a name the kernel reports, after the kernel already enforced confinement. `lexical_rel` cleans a path as text; its comment says it is used only to match deny rules on the requested name.

### 6.2 `tools/fs.rs` — check and use the same handle

```rust
match beneath::open_beneath(&root, path, flags, ResolveFlag::empty()) {
    Ok(fd) => Ok(Opened::Inside { rel: resolved_rel(work, &fd)?, fd }),
    Err(e) if beneath::is_escape(e) => Ok(Opened::Outside(e)),
    Err(e) => Err(errno(path, e)),
}
```

The gateway receives the open handle and the path the kernel resolved (`rel`). It checks the policy on `rel`, then reads from that same `fd`. If the agent names `notes`, a symlink to a denied file, `rel` is the denied file's path and its deny rule fires.

`fs.write` never opens the target by full path:

```rust
let fd = openat(&t.parent, tmp.as_str(),
    OFlag::O_WRONLY | OFlag::O_CREAT | OFlag::O_EXCL | OFlag::O_NOFOLLOW | OFlag::O_CLOEXEC, mode)?;
... write, fsync ...
renameat(&t.parent, tmp.as_str(), &t.parent, t.name.as_str())
```

`t.parent` is the folder's handle, opened beneath the workspace. `O_EXCL` fails if the temp name already exists (so it cannot be a planted link); `renameat` replaces the *name* in that folder and never follows a symlink sitting there. A symlink swapped into the path after the folder was opened changes nothing: the handle still points at the real folder.

### 6.3 `cgroup.rs` — limits and a clean kill

`CgroupRoot::init` in order: read `/proc/self/cgroup`; check the three controllers are offered; ask systemd whether the unit is delegated (stop if it says no, before changing anything); create the leaf; then loop "move every process into the leaf, enable controllers" until the controllers are on; finally probe by creating and removing a group with all limits.

```rust
cg.write("memory.max", &(limits.memory_mb * 1024 * 1024).to_string())?;
if cg.dir.join("memory.swap.max").exists() { cg.write("memory.swap.max", "0")?; }
cg.write("pids.max", &limits.pids.to_string())?;
cg.write("cpu.max", &format!("{} 100000", limits.cpu_percent * 1000))?;
```

`cpu.max` is "quota period" in microseconds: `200000 100000` allows 200 ms of CPU per 100 ms, i.e. two cores. Without `memory.swap.max = 0`, a process over its memory limit would be pushed to swap instead of killed.

`destroy` writes `cgroup.kill`, waits for `populated 0` in `cgroup.events`, then removes the folder. `oom_killed` and `pids_limit_hit` read the group's event counters so the agent is told *why* a command died.

### 6.4 `ns.rs` — building the cage

Process H (`helper`):

```rust
nix::sys::prctl::set_pdeathsig(Signal::SIGKILL)?;
if getppid().as_raw() != spec.parent_pid { bail!("runtime exited before the sandbox started"); }
fs::write(spec.cgroup.join("cgroup.procs"), "0")?;                // 1. join the cgroup
unshare(CLONE_NEWUSER | CLONE_NEWNS | CLONE_NEWPID | CLONE_NEWNET)?;  // 2. namespaces
fs::write("/proc/self/setgroups", "deny")?;                       // 3. maps
fs::write("/proc/self/uid_map", format!("{0} {0} 1", spec.uid))?;
fs::write("/proc/self/gid_map", format!("{0} {0} 1", spec.gid))?;
match unsafe { fork() }? { ... }                                  // 4. child = PID 1
```

Order matters at each step. The cgroup is joined first so nothing the command ever starts escapes the limits. `set_pdeathsig` is followed by a parent check, because if the runtime died just before the call, the signal would never come. `setgroups` must be `deny` before `gid_map` or the kernel refuses the map. `unshare(CLONE_NEWPID)` does not move the caller; only its *next child* is in the new PID namespace, hence the `fork`.

`fork` is `unsafe` in Rust because in a multi-threaded process the child may only call a few safe functions. H is single-threaded, which the `SAFETY:` comment states.

Process C (`init` → `build_root`), following bubblewrap:

1. `mount(NULL, "/", MS_REC|MS_PRIVATE)`: stop mount changes travelling to or from the host.
2. Mount a tmpfs on `/tmp`; create `newroot` and `oldroot` in it.
3. `pivot_root("/tmp", "/tmp/oldroot")`: the tmpfs is now `/`, the host's root is at `/oldroot`.
4. Bind `/oldroot/<workspace>` to `/newroot/work`; each policy folder to the same path under `/newroot`, then remount read-only.
5. `/newroot/dev`: tmpfs plus five device files bound from the host (a user namespace cannot create device files).
6. Mount a new `proc` on `/newroot/proc`: it shows only this PID namespace.
7. Detach `/oldroot`.
8. `chdir("/newroot"); pivot_root(".", "."); umount2(".", MNT_DETACH)`: `/newroot` becomes `/`.
9. Remount `/` read-only.

```rust
fn remount(dst: &Path, add: MsFlags) -> Result<()> {
    let cur = statvfs(dst)?.flags();
    let mut flags = MsFlags::MS_REMOUNT | MsFlags::MS_BIND | add;
    for (st, ms) in [(FsFlags::ST_RDONLY, MsFlags::MS_RDONLY), (FsFlags::ST_NOSUID, MsFlags::MS_NOSUID), ...] {
        if cur.contains(st) { flags |= ms; }
    }
    mount(None::<&str>, dst, None::<&str>, flags, None::<&str>)
}
```

Inside a user namespace the kernel refuses a remount that would *clear* a flag the host set (for example `nosuid` on `/nix/store`). So the current flags are read and kept, and only new restrictions are added. Forgetting this gives a confusing `EPERM`.

Then `loopback_up()` (an `ioctl` that sets the `IFF_UP` flag on `lo`), `drop_privileges()` (bounding set, ambient set, `no_new_privs`, `capset` to zero), `landlock::restrict`, and a second `fork`. G calls `close_range(4, MAX)` so no stray file descriptor from the runtime leaks in, then `execve`. C stays in `reap_until`: `waitpid(None)` collects every dead child (orphans are re-parented to PID 1) until the command itself ends, then writes `exit N` to fd 3.

Why fd 3 and not stderr: the command's stderr belongs to the command. fd 3 is marked close-on-exec, so a successful `execve` closes G's copy silently; if `execve` fails, G is still our code and writes `exec-failed: …`.

### 6.5 `sandbox/mod.rs` — the runtime's side

```rust
unsafe {
    cmd.pre_exec(move || {
        ... if libc::dup2(w, ns::STATUS_FD) < 0 { return Err(std::io::Error::last_os_error()); }
        Ok(())
    });
}
```

`pre_exec` runs a closure in the child between `fork` and `exec`. Only "async-signal-safe" calls are allowed there (the runtime has threads); `dup2` and `fcntl` are. It places the pipe's write end on fd 3.

The watchdog is the M1 pattern, but its action is now `cg.kill()`. After `wait`, the group is destroyed whatever happened, so no process outlives the call. `run` turns the status lines into an `ExecOutput`, or into a `ToolError` if setup failed.

`check_host` gathers *all* problems (user namespaces disabled, Ubuntu's AppArmor restriction with the one-line fix, no Landlock, no `openat2`, no delegation) and the runtime prints them together.

### 6.6 `landlock.rs`

```rust
Ruleset::default()
    .set_compatibility(CompatLevel::BestEffort)
    .handle_access(AccessFs::from_all(abi))?
    .create()?
    .add_rules(path_beneath_rules(&ro, AccessFs::from_read(abi)))?
    .add_rules(path_beneath_rules(["/work", "/tmp"], AccessFs::from_all(abi)))?
    .add_rules(path_beneath_rules(DEVICES, AccessFs::ReadFile | AccessFs::WriteFile))?
    .add_rules(path_beneath_rules(["/proc"], AccessFs::ReadFile | AccessFs::ReadDir))?
    .restrict_self()?;
```

`handle_access` says which kinds of access the ruleset governs (all of them); each `add_rules` punches a hole. This is a *builder*: each method returns the object so calls chain, and `?` stops at the first error. `from_read` includes execute.

### 6.7 `checkpoint.rs::revert_outside` — undoing what was not allowed

`scan` lists every path in a tree with its kind, mode and size (never following links). Step 1 removes paths that are new or changed kind and not writable, deepest first. Step 2 restores paths that are missing or different, shallowest first. Every change goes through `parent_of`:

```rust
let fd = beneath::open_beneath(work_root, dir,
    OFlag::O_PATH | OFlag::O_DIRECTORY, ResolveFlag::RESOLVE_NO_SYMLINKS)?;
```

and then acts on `/proc/self/fd/<fd>/<name>`, i.e. *through the handle*. If the attacker turned `tests` into a link to a host folder, opening `tests` with `RESOLVE_NO_SYMLINKS` fails; the code first removes the link (an entry in the real workspace root), recreates the folder, and only then restores files into it. The test for this plants exactly that link and checks the host folder stays empty.

### 6.8 `gateway.rs` — what changed

`decide` now returns one of four outcomes (`Allow`, `Deny`, `Invalid`, `Fail`) instead of nested `Result`s, and `Approved` carries open handles. `run_exec` builds the `SandboxSpec`, creates the cgroup, runs, destroys the cgroup, reverts, and logs `policy.violation` if anything was reverted.

### 6.9 `tests/security/main.rs`

Helpers: `policy_text` (the preset plus `python3` and chosen limits), `attack` (run Python as one `exec`), `py`/`PROBE` (prints `ESCAPED`/`BLOCKED`), `assert_blocked` (no ESCAPED, and every expected BLOCKED present), `assert_nowhere` (grep the whole execution folder for a canary string).

| Group | Tests |
|---|---|
| 1. File tools | `..` traversal; absolute paths; symlinks out (absolute, relative, to `/`); `/proc` magic link; write through a symlinked folder; write over a symlink; reaching `../log.db` and checkpoints |
| 2. Reading from code | denied `.env` (direct and via `conftest.py`); host home, `~/.ssh`, original repo, `/etc/passwd`; the event log and state dir; environment and `/proc/*/environ` |
| 3. Writing from code | changes outside write globs reverted; folder replaced by a symlink |
| 4. Network | TCP, UDP, IPv6, DNS, the host's loopback listener; host abstract unix socket |
| 5. Resources | fork bomb; memory hog; infinite loop; a daemon that outlives its parent |
| 6. Privileges | capabilities and `setuid(0)`; `mount` directly and in a nested user namespace; signalling a host pid; private `/tmp` |
| 7. Patch | symlink to a host file does not leak its content |

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-042 | Sandbox set up by re-running the agentos binary; H → C (PID 1) → G; status on fd 3 | fork in the runtime; a second binary; command as PID 1 |
| D-043 | Identity uid map; all capabilities dropped explicitly | Map to root inside |
| D-044 | New root on a tmpfs, bubblewrap's double `pivot_root`; no `/home`, `/etc`, `/sys`; loopback up | `chroot`; hide parts of the host root |
| D-045 | Landlock for ABI 5, best effort, refuse if not enforced | Require newest ABI; Landlock network/signal scoping |
| D-046 | `libc` as a direct dependency for five raw calls | Extra crates per call |
| D-047 | File tools use `openat2` handles; policy on the resolved path (replaces D-038) | `RESOLVE_NO_SYMLINKS` everywhere; canonicalize-then-check |
| D-048 | Revert outside write globs by content, through no-symlink handles | Plain path operations; mtime comparison; full rollback |
| D-049 | cgroup setup: systemd check, leaf for all processes, retry, probe, `cgroup.kill` | Move only the runtime; trust `mkdir`; kill by pid |
| D-050 | One-sided symlinks listed in the patch as comments | Drop `--no-dereference`; `git diff` |
| D-051 | Security tests: `python3` allowed in the test policy; ESCAPED/BLOCKED protocol | Only `conftest.py`; calling sandbox functions directly |

## 8. Problems hit

In order. Bugs in the runtime are also in `docs/bugs.md`; everything is in the Findings table.

1. **Landlock experiment could not spawn a child.** `EACCES` on spawn; `strace` showed `openat("/dev/null")`. Fixed by allowing the device files. Commit `11b4863`.
2. **B-004: `fs.write` → "Invalid argument".** `openat2` rejects `O_PATH | O_NOCTTY`. Found on the first sandboxed demo run (call #6 failed, so the final pytest still failed). Fixed in `beneath.rs`. Commit `bf899fd`.
3. **The probe script crashed twice.** `os.listdir("/")` and `os.listdir("/dev")` are refused by Landlock. Not a bug: only listed folders are readable.
4. **Test would not compile.** `SocketAddr::from_abstract_name` needs `use std::os::linux::net::SocketAddrExt` (a Linux-only extension trait).
5. **B-001: 20 of 25 security tests failed on the first full run** with `File exists` / `Device or resource busy`: concurrent runtimes raced in cgroup setup. Fixed with tolerate-and-retry. Commit `bf899fd`.
6. **Attack snippets lacked `import os`.** Test bug; the tests failed loudly (exit code 1), as the BLOCKED-line rule intends.
7. **Wrong test expectation.** `chmod src` is an allowed change because `src/**` matches `src`. The test now uses a folder outside the globs. The code was right.
8. **B-002: run crashed writing the patch** when the agent added a symlink (diff exit 2). Fixed with patch comments (D-050). Commit `2b9e74d`.
9. **Old M1 assertion.** It expected the text-check message; updated to the `openat2` rule name.
10. **B-003: non-delegated scope half-worked.** A manual run without `systemd-run` passed the startup check, rearranged the terminal's cgroup, and failed at call #2. The terminal's cgroup was restored by hand; the runtime now asks systemd first and probes. The `user.delegate` xattr was tried as a signal and found unset here. Commit `bf899fd`.

## 9. How to run it

```sh
nix develop
systemd-run --user --scope -p Delegate=yes cargo test          # 16 unit + 25 security + 4 slice: all ok
systemd-run --user --scope -p Delegate=yes cargo run -- --state-dir /tmp/agentos-demo run \
    --repo examples/demo-repo --policy policies/nix.toml \
    --agent scripted --script examples/scripts/fix-demo.toml   # same 7 lines as M1, now sandboxed
cargo run -- --state-dir /tmp/agentos-demo trace
```

Without `systemd-run` the run stops at once with the message quoted in §2; plain `cargo test` therefore fails the tests that need the sandbox.

On Ubuntu (not yet verified): use `policies/fhs.toml`; Ubuntu 23.10+ also needs `sudo sysctl -w kernel.apparmor_restrict_unprivileged_userns=0`, which the startup check prints when it applies.

## 10. What this milestone does not do

- **`fhs` preset unverified** until CI runs on Ubuntu after a push.
- **No per-call checkpoints or recovery.** The revert compares against `cp-0000`; a crash still leaves the execution interrupted. M3.
- **No TOCTOU race test, no benign suite, fewer than 30 cases.** M5 (v1 table).
- **No seccomp filter** (cut from v1). Commands can call any syscall the kernel allows an unprivileged process; isolation rests on namespaces, Landlock, capabilities and cgroups.
- **No disk-size limit** on the workspace (D-010). Sandbox `/tmp` is limited by the memory cgroup.
- **Nested user namespaces are possible** inside the sandbox. They grant nothing over the outer namespaces, and Landlock blocks mounting (tested), but it is more kernel surface. Stated here rather than hidden.
- **Sub-mounts under a read-only folder** keep their own mount flags (the remount is not recursive); Landlock still limits them to read-only access.
- **Kernel bugs** are out of scope (threat model).

## 11. Check your understanding

1. `unshare -Urmpn` gives a new mount namespace, yet `~/.ssh` was still readable. Why, and what makes it unreadable in AgentOS?
2. Why is the sandbox set up by re-running the binary instead of calling `unshare` in the runtime?
3. Put in order and justify: write `uid_map`, join the cgroup, `unshare`, `fork`. What breaks if the cgroup is joined last?
4. Why is there a process C between the helper and the command? What two jobs does PID 1 have?
5. What does each of the two `pivot_root` calls achieve, and why is the host root detached in between?
6. Why does `remount` read the current flags first? What error appears without that?
7. What does `RESOLVE_BENEATH` return for `../x`, and why is "check the path, then open it" not equivalent?
8. In `fs.write`, a symlink is swapped into the path after the policy check. Why does the write still land inside the workspace?
9. The policy is checked on the *resolved* path. Give an example where the requested and resolved paths differ and the difference matters.
10. Why must `revert_outside` never use plain path operations? Describe the attack the symlink test performs.
11. Why does the watchdog write to `cgroup.kill` instead of sending SIGKILL to a pid?
12. The startup check once passed on a non-delegated scope. What did it test, what should it have tested, and what general rule follows?

## Notes added after the milestone

The text above is left as it was written. Later changes are listed here.

- **2026-11-13, `fhs` preset.** Every "not yet verified" about the `fhs` preset above describes the state when this document was written. Prince pushed on 2026-11-12. The first two CI runs failed the 200-kill suite and found bugs B-007 and B-008 (`docs/bugs.md`, D-070, D-071). The third, <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> on commit `c3c9502` (Ubuntu 24.04), is green: lint, all test suites (32 security cases, the benign suite) and 4,800 kills with 0 violations. Results of record: `docs/results/recovery-200-fhs.md`, `docs/results/security-fhs.md`. See D-072. Still not run on Ubuntu: the benchmarks and `make demo`.
