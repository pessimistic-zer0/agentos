# M8 — After the bonus: checking the finished project, and what a first user needs

| | |
|---|---|
| Weeks in the plan | None. The v1 plan ends at M6 and the bonus at M7. This is the work Prince asked for afterwards, on 2026-11-13 and 2026-11-14 |
| Started / finished | 2026-11-13 / 2026-11-14 |
| Commits | `0c29b2d` … the commit that adds this file (tag `m8`) |
| Status | Done with gaps (the last commits are not yet verified on `fhs`; see §2) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

> **Note, added 2026-11-14 after the tag:** Prince pushed `69cdbf0` and CI run 43612319559 is green on both jobs (D-097). The rows in §2 and the first line of §10 that say "not yet verified" describe the state when this document was written; they are now verified, including `make demo` on Ubuntu and the profile at `/usr/local/bin/agentos`.

## 1. Aim

M0 to M7 built the thing and proved its claims with suites. This milestone asks two different questions. First: is it really finished, if someone tries what the suites do not? Second: could a person other than its author use it on their own project?

The answers were "not quite" and "no". So this milestone fixes six bugs that only hands-on use found, makes the first messages a newcomer sees say what to do, makes the result (the patch) honest about what it leaves out, and adds the two things without which a real project cannot get past its first test run: packages from outside the repository, and a way to run on Ubuntu without switching off a system-wide protection.

It touches all five parts a little: the **gateway** (a failed command is undone instead of stopping the run), the **policy** (environment variables, checks on mounts), the **sandbox** (the startup check, which path a program is started through), **checkpoints and recovery** (deleting locked folders; folder-making writes). The **event log** is unchanged. No tool was added.

## 2. Requirements

There is no row in the v1 milestone table. The requirements are what Prince asked for, in his words, in order:

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| "Verify if everything is built correctly and the edge cases are handled" | Yes, and it was not all correct | All suites passed (`make test` 81 at the time). Hand-made probes then found B-009 and B-010 (`docs/bugs.md`); Findings row of 2026-11-13 |
| Fix B-009 and B-010 | Yes | D-085, D-086. `tests/hardening.rs`: six tests, all failing on the old code. `fhs`: CI run 43573592353 (D-087) |
| Let `fs.write` make missing folders | Yes | D-084. Three tests in `tests/hardening.rs`; with the new recovery rule switched off, the crash test failed 3 runs of 3 |
| "Think like an actual user … where a user might struggle" | Yes | Walk-through with the built binary and reading of bug trackers for similar tools: Findings row of 2026-11-14, commit `b20e809` |
| Fix the defects inside v1 (first messages, the LLM server check, the patch's limits, the README) | Yes | D-089, D-090, D-095, B-011, B-012. `tests/hardening.rs`, `tests/llm_agent.rs` |
| Feature 1: packages from outside the repo | Yes | D-091, D-092. `tests/deps.rs` (5 tests); the trial in `docs/results/real-repo-trial-nix.md` |
| Feature 2: an AppArmor profile for Ubuntu | Yes | D-093, D-094. CI job `fhs-apparmor` in run 43590153676: refused without the profile, ran the demo task and four suites with it |
| A document for this round, and a tag | Yes | This file; tag `m8` |
| Try it on a real project (a repository made by a separate agent, tested by hand) | Yes | `docs/results/real-repo-trial-nix.md`: four tickets fixed through the gateway, 79 tests pass in the sandbox and after `git apply`; a local 4B model solved one ticket. Found B-012 |
| Re-run the numbers that were stale | Yes on `nix` | Fault injection: `docs/results/recovery-200-nix-m8.md` (4,800 + 1,600 runs, 0 violations, 0 duplicated effects). Benchmarks: `docs/results/bench-nix-m8/summary.md`. Found B-013 |
| Close the small verification gaps (the profile at its real path; `make demo` on Ubuntu) | **Not yet verified** | Both are now steps in `.github/workflows/ci.yml`. They run after Prince pushes |
| All of it on the `fhs` preset | **Partly** | Up to `366c986`: CI run 43590153676, both jobs green (D-094). The commits after it (`bb60f5b` on: B-012, B-013 and D-096, the CI steps) are local |

Not met: the last two rows, for the reason given. Nothing was weakened or skipped to pass. One test helper changed: the mock LLM server ignores a connection that sends nothing (D-089); its assertions are as before.

## 3. What you need to know first

**Trusted code, untrusted data.** The runtime is trusted, the agent's commands are not. But after a command the runtime walks and copies the folder that command shaped. An error there is caused by the command, even though it shows up in trusted code. B-009 and B-010 are both this.

**Permission bits on a folder.** Read = list the names. Write = add and remove names. Execute, on a folder called "search" = use a name inside it. The owner is bound by them too; only root is not. So `chmod 000 dir` makes a folder that its own owner cannot empty, until the bits are set back (glossary: Permission bits, Search bit).

**A patch is text.** A unified diff holds changed lines. It has no way to hold a binary file's bytes, a file's mode, an empty folder, a symlink, or a new file with no lines. `patch` and `git apply` skip those silently.

**A virtualenv** is a folder with a project's Python packages and a `python3` that is a symlink to the system's Python. Python uses the folder's packages only when started through that link (glossary: Virtualenv).

**Environment variables** are named text values every process gets from the one that started it. The sandbox clears them so no secret leaks in; a project may still need a few of its own.

**AppArmor** limits programs by profiles, one per program path. Ubuntu uses it to decide who may use user namespaces (glossary: AppArmor).

**fork, then exec.** Starting a program on Linux is two steps: copy the current process (`fork`), then turn the copy into the new program (`exec`). Code can run in the copy between the two. In that gap only calls that take no lock are safe, because the copy has one thread and another thread of the original may have held a lock (glossary: Async-signal-safe).

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| Hand probes, 2026-11-13 (scratch scripts, not kept; the cases are now `tests/hardening.rs`) | Four kinds of file a command can leave: a non-UTF-8 name, a `chmod 000` file, a `chmod 000` folder inside and outside the write globs | The run stopped each time; for the folders, `recover` then failed for good | B-009, B-010 |
| Walk-through as a newcomer, 2026-11-14 (Findings row) | First-run mistakes; applying a patch; a test that needs a package; Ctrl-C; a 20,000-file repo | See the row: six things seen, four more read | D-088's list |
| `experiments/m8-venv/NOTES.md` | A virtualenv mounted read-only | Worked for `pytest`; for `python3 -m pytest` only by luck; a mount under `/tmp` broke the sandbox | D-091, D-092 |
| Trial on a project that is not the demo (`docs/results/real-repo-trial-nix.md`) | Six tickets on an inventory service with real PyPI packages, by hand and with a local 4B model | Four tickets fixed; one bug found; six limits of the file tools met | B-012; six lines in the parking lot |

## 5. What was built

| File | Purpose |
|---|---|
| `src/gateway.rs` | `undo_exec`: a command whose leftovers cannot be recorded fails and is rolled back (D-085). Deny check on folders a write would make; `created_dirs` in the intent (D-084). Policy variables added to a command's environment (D-092) |
| `src/checkpoint.rs` | `remove_tree` (D-086). `write_patch` returns a `PatchReport` and writes the `# agentos:` notes (D-090, D-095) |
| `src/tools/fs.rs` | `open_for_write` finds the deepest existing folder; `write` makes the missing ones (D-084) |
| `src/recovery.rs` | Roll back a cut-off write that was going to make folders; `remove_tree` for orphan checkpoints |
| `src/tools/exec.rs` | `link_chain_visible`: start a program through the path it was found at (D-091) |
| `src/policy.rs` | `[sandbox.env]`, reserved mounts and names, errors printed once (D-089, D-092) |
| `src/sandbox/ns.rs`, `mod.rs`, `cgroup.rs` | `probe_in_child` and `check_host` (D-093, D-096); `not_delegated` (D-089) |
| `src/agent/llm.rs`, `src/main.rs` | `check_reachable` before an execution exists; absolute state folder (B-011) |
| `src/runtime.rs` | Order of the startup checks; the `NOT in the patch` line |
| `policies/*.toml` | `[sandbox.env]` with `PYTHONDONTWRITEBYTECODE`; a header saying what to change for your own project |
| `packaging/apparmor/agentos` | The profile |
| `tests/hardening.rs` (17 tests), `tests/deps.rs` (5), one more in `tests/llm_agent.rs` | The tests for all of the above |
| `.github/workflows/ci.yml` | Job `fhs-apparmor`; `make demo` in the main job; `python3-venv` |
| `README.md` | "On your own project"; Ubuntu with the profile |
| `docs/results/` | `real-repo-trial-nix.md`, `recovery-200-nix-m8.{md,json}`, `recovery-ext-200-nix-m8.json`, `bench-nix-m8/` |

How the pieces fit: nothing new sits between the agent and the gateway. The changes are at the edges: what the runtime checks before it starts (`check_host`, the policy, the LLM server), what a command is given (`ro_mounts`, variables, the program path), what the runtime does when a command leaves a mess (`undo_exec`, `remove_tree`), and what the user gets at the end (the patch and its notes).

## 6. The code, explained

### 6.1 A command that leaves a mess fails; the run goes on (`gateway.rs`)

```rust
let reverted = match checkpoint::revert_outside(&work, &base, |rel| policy.writable(rel)) {
    Ok(reverted) => reverted,
    Err(e) => return Ok(Err(self.undo_exec(e)?)),
};
```

`run_exec` returns `Result<Result<ToolOutput, ToolError>>`. The outer `Result` means "the runtime broke" and ends the run; the inner one means "the tool failed" and is handed to the agent. Before, the scan's error went out through `?`, the outer way. Now `undo_exec` rolls the workspace back to the last checkpoint and turns the error into the inner kind. If the rollback itself fails, `?` still ends the run, with the intent open, which is the state `agentos recover` knows how to settle.

Why roll back and not repair: FAILED means "nothing was changed" (D-040). After a rollback that is true. A repair (rename the file, `chmod` it) would put content into the workspace that nobody wrote.

### 6.2 Deleting what a command locked (`checkpoint.rs`)

```rust
match std::fs::remove_dir_all(path) {
    Ok(()) => return Ok(()),
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
    Err(_) => {}
}
unlock(path)?;                      // owner rwx on every real folder in the tree
std::fs::remove_dir_all(path)
```

`unlock` changes a mode only on what `symlink_metadata` reports as a real folder, so a planted symlink is never followed. It relies on no sandboxed process being alive, which holds wherever it is called.

### 6.3 `fs.write` makes folders, after the intent (`tools/fs.rs`)

`open_for_write` first tries the folder as before. On "does not exist" it walks back to the deepest folder that does exist, still through `openat2(RESOLVE_BENEATH)`, and keeps the rest as names:

```rust
for k in (0..comps.len()).rev() {
    match open(&comps[..k].join("/")) {
        Ok(parent) => {
            if comps[k..].contains(&"..") { break; }
            return target(parent, comps[k..].iter().map(|c| c.to_string()).collect());
        }
        ...
```

Nothing is created here. The gateway checks the policy on the full path and the deny rules on each new folder, writes the intent with `created_dirs`, and only then `write` runs one `mkdirat` per folder through the handle of the folder above. `mkdirat` fails if the name exists in any form, including a symlink pointing at nothing. Recovery's side is one extra arm: file absent, but folders were going to be made, so roll back.

### 6.4 The patch says what it leaves out (`checkpoint.rs::write_patch`)

After `diff` has run, the two folder scans are compared once more for the five things text cannot carry: lines `Binary files … differ` in the diff's own output, files whose mode differs, new files with an execute bit, new files of size 0, new folders with nothing below them. Each becomes a line `# agentos: …` at the top of the patch; `patch` and `git apply` ignore text before the first file header. `runtime::finish` prints one line with the counts.

### 6.5 Which path a program is started through (`tools/exec.rs`)

```rust
let tidy = |p: &Path| Some(std::fs::canonicalize(p.parent()?).ok()?.join(p.file_name()?));
```

"Resolve the folder, keep the name." `/bin/pytest` becomes `/usr/bin/pytest`; `venv/bin/python3` stays the link it is. The loop then follows the link step by step and gives up (falling back to the fully resolved path, the old behaviour) if any step is not under a read-only mount.

### 6.6 The startup probe (`sandbox/ns.rs::probe_in_child`)

```rust
unsafe { probe.pre_exec(move || ns::probe_in_child(&uid_map, &gid_map)); }
```

`pre_exec` runs the closure in the forked child. The closure never returns: it makes the namespace, writes the id maps, changes a mount, brings up loopback, and calls `_exit` with 0 or the number of the step that failed. The program named in the `Command` is never started. Inside, only raw `libc` calls are used and the error number is turned into digits by hand, because `format!` allocates and the allocator's lock may be held by a thread that does not exist in the child.

The first version (D-093) started the program again with a hidden argument. That is the bug B-013: a library function assumed it lived in the `agentos` binary.

### 6.7 Rust seen for the first time here

- `Result<Result<T, E1>, E2>`: two kinds of failure with different consequences, kept apart by the type.
- `Command::pre_exec` and `unsafe fn`: the compiler cannot check the "only async-signal-safe calls" rule; the comment on the block says what it relies on.
- `c"/proc/self/setgroups"`: a C-string literal, ending in the zero byte the kernel expects.
- A field named `source` in a `thiserror` enum is treated as the error's cause; that is why the policy error was printed twice.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-084 | `fs.write` makes missing folders, after the intent; recovery rolls back if it finds the list and no file | An `fs.mkdir` tool; requiring each folder to match a write glob |
| D-085 | Leftovers the runtime cannot record: the call fails and is undone | Repairing the workspace; skipping the files; stopping the run |
| D-086 | `remove_tree` gives folders their owner bits back before deleting | Normalising modes in checkpoints; leaving the tree |
| D-087 | The first fixes count as verified on `fhs` (CI run 43573592353) | Replacing the results of record |
| D-088 | Fix defects; open scope for exactly two features | Defects only; the whole list |
| D-089 | First messages: user's input first, the fix first, nothing started before the checks pass | A real request as the LLM check; self-start under `systemd-run` |
| D-090, D-095 | The patch lists what it cannot carry (five kinds) | A git-format patch; an `apply` command |
| D-091 | A program is started through the path it was found at | Always the found path; virtualenv knowledge in `src/` |
| D-092 | `[sandbox.env]`, fixed text; four places a mount may not be; no `.pyc` files | Passing variables through from the host; variables set by the agent |
| D-093 | An AppArmor profile; check user namespaces by trying | The system-wide switch only; a confining profile |
| D-094 | The first-user work counts as verified on `fhs` (CI run 43590153676) | Running every suite in the profile job |
| D-096 | The probe runs in a forked child | A hidden argument every program must handle; probing in-process |

## 8. Problems hit

In the order they happened. Each is in `docs/bugs.md` or the Findings table.

1. **B-009, B-010.** Expected a failed tool call for odd leftovers; got a stopped run and, for a locked folder, an execution that could never be recovered. Found by hand. While fixing: a read-only folder (mode 555) breaks recovery too, with no hostile intent (`7d0ac1b`).
2. **The mock LLM server counted an empty connection as a request.** The new server check opens one. Two LLM tests failed until the mock ignored it (`f009931`).
3. **B-011.** Looking at the demo's output with a relative state folder, the run failed at its last step: `diff` was started in another folder (`67796a2`).
4. **`.pyc` files made every Python run carry patch notes.** The new note would have cried wolf on each run. The shipped policies now stop Python writing them (`210bc3b`).
5. **A mount under `/tmp` broke the sandbox with "File exists".** Found in the virtualenv experiment because the scratch folder was under `/tmp`. Now refused when the policy is loaded.
6. **B-012.** The first real-project run added a Python package; after `git apply` its empty `__init__.py` was missing. D-090's list had been written from memory (`bb60f5b`).
7. **B-013.** `make bench` failed at its last step with a missing file. Three steps earlier the write benchmark had died: the new startup check started the benchmark binary a second time. It had passed CI, because no test uses the library from another program (`15fab27`).
8. **The subagent that built the test repository ran into its usage limit** just before reporting. Its work was complete and was checked by hand instead.
9. **A Ctrl-C test proved nothing**: a background job in a non-interactive shell ignores SIGINT. The claim in the README ("nothing is printed") rests on reading the code: there is no signal handler.

## 9. How to run it

```sh
nix develop -c make test          # 105 tests; hardening.rs and deps.rs are part of it
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test hardening
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test deps
nix develop -c make eval-recovery # 200 kills per cell, about 11 minutes with the external cells
nix develop -c make bench         # into docs/results/bench-nix/ ; pass a folder to keep the old one
```

On your own project: README, "On your own project". In short: copy a policy, set the write paths and commands, add the virtualenv to `sandbox.ro_mounts`, activate it, run, read the patch, apply it.

On Ubuntu without the system-wide switch: the three `sudo` lines in the header of `packaging/apparmor/agentos`.

## 10. What this milestone does not do

- **The last commits are not verified on `fhs`** (see §2). In particular, which step of the new probe fails on Ubuntu without the profile has not been seen.
- **The file tools are still text only and whole-file**, with a 64 KB cap. The real-project trial met all three limits. Parked (spec §22).
- **No hint when a mounted virtualenv is not the active one**; `.env.*` also denies `.env.example`; caches such as `.pytest_cache` are reported as violations. Parked.
- **No clean-up command, no leaving folders out of the copy, no self-start under `systemd-run`, no message on Ctrl-C.** Parked.
- **The trial was one session by hand**, on one generated project, with one small model. It is a record, not a measurement.
- **Benchmarks were run on NixOS only**, as before.
- **The project name** is still open (D-023, D-068).

## 11. Check your understanding

1. Why did a file that a sandboxed command created cause an error in *trusted* code, and why was "runtime error" the wrong label for it?
2. After a command leaves a `chmod 000` folder, why can the folder's own owner not delete it? Which three bits are needed, and for what?
3. A read-only folder (mode 555) with one file in it copies without complaint. Which later step fails, and why did no suite notice?
4. Why does `fs.write` make folders only *after* the intent is written? What does recovery do if the runtime dies between making the folder and writing the file?
5. `git apply` accepted a patch and the result was still incomplete. Name five things a unified diff cannot carry.
6. Why is a virtualenv's `python3` started through the link and not through the file the link points at? What made it work "by luck" before?
7. Why may a policy set environment variables as fixed text, but not pass them through from the user's shell?
8. On Ubuntu, a program without a profile can create a user namespace. Why does the sandbox still fail, and how does the startup check find that out?
9. What is wrong with a library function that starts "the program I am" with a secret argument? How does the forked-child version avoid it?
10. Between `fork` and `exec`, why is `format!` forbidden and a raw `write` allowed?
11. The security suite and the recovery suite both passed while B-010 existed. What question does each suite ask, and which question did neither ask?
12. Interview level: the patch notes were first designed from memory and missed empty files. What would a design process look like that could not have missed them?
