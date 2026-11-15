# M10 — `agentos apply`

| | |
|---|---|
| Weeks in the plan | None. Held in M9 (D-098), approved by Prince on 2026-11-15 |
| Started / finished | 2026-11-15 / 2026-11-15 |
| Commits | The commit that adds this file (tag `m10`) |
| Status | Done with gaps (not yet verified on `fhs`) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

> **Note, added 2026-11-15 after the tag:** verified on `fhs` by CI run 43691133260 on `d98eab1` (D-110). The first run, 43690755406, failed on a false note that only Ubuntu could show (B-014, fixed by D-109). The lines below that say "not yet verified" describe the state when this document was written.

## 1. Aim

A run ends with a patch, and M8 taught the patch to list what it cannot carry. The user still had to carry those things over by hand. This milestone adds the command that does it: `agentos apply <id>` copies the result of a finished execution into the repository.

It is the first and only place where AgentOS writes into the user's repository. So most of the milestone is about refusing. It touches **checkpoints** (the same two folder trees the patch is made from) and the **event log** (one new record). The gateway, the policy, the sandbox and recovery are unchanged. No tool is added: the agent cannot ask for an apply.

## 2. Requirements

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| Prince's word that an explicit command is within design rule 10 | Yes | "go ahead with apply too", 2026-11-15; D-108. Rule 10 in CLAUDE.md now says "a run never modifies the original repo" |
| Binary files, empty files, modes, empty folders and symlinks come across | Yes | `tests/apply.rs::apply_copies_everything_the_run_changed_and_nothing_else`: afterwards the repository equals the workspace |
| Refuse if the repository changed since the copy, and change nothing | Yes | `apply_refuses_a_repository_that_changed_since_the_copy`: both conflicts named, digest unchanged; an unrelated edit does not matter |
| Never write through a symlink or outside the target | Yes | `apply_does_not_write_through_a_symlink_in_the_repository` |
| Only a finished execution | Yes | `apply_needs_a_finished_execution`: refused, then resumed, then applied; the repository's own tests pass |
| Files the run never saw are left alone | Yes | The denied `.env` is byte-identical after apply (first test); caches are not applied (`a_run_that_changed_nothing_…`) |
| A dry run; another target folder; doing it twice | Yes | `--dry-run`, `--to`; the second apply reports "already holds the result" |
| A record in the log | Yes | `result.applied` event; a line in `agentos trace` |
| On the real project | Yes | `docs/results/real-repo-trial-nix.md`, last part: `__init__.py` arrives, 79 tests pass in the clone |
| Nothing else got worse | Yes on `nix` | `make test`: 124 passed; clippy clean |
| Same on the `fhs` preset | **Not yet verified** | Needs a push and a green CI run |

## 3. What you need to know first

**What the rule protects.** "The original repo is never modified" exists so that an agent, or a bug in the runtime, cannot damage the user's work. A command that only the user can run, after the run is over, does not weaken that. A flag that applied automatically at the end of a run would.

**Check everything, then act.** If the tenth file cannot be written, the first nine should not have been. So all the questions are asked first, for every path, and only then is anything written.

**Three states of one path.** What the run started from (the base checkpoint), what the run made (the workspace), what the repository holds now. Apply is allowed for a path if "now" equals "started from". It is already done if "now" equals "made". Anything else is the user's own change, and merging is the user's job.

## 4. Experiments

None separate. The safe-write pattern (a handle on the parent folder, opened without following symlinks) was already proven by the revert code and its security tests (D-009, M2).

## 5. What was built

| File | Purpose |
|---|---|
| `src/checkpoint.rs` | `apply_changes`: compare the three states, plan, then write through parent handles; `symlink_on_the_way` |
| `src/runtime.rs` | `apply`: finished? which repository? which paths are caches? print the list; write the log record |
| `src/main.rs` | `agentos apply <id> [--to <folder>] [--dry-run]` |
| `src/log.rs`, `src/trace.rs` | `result.applied`; `applied: N change(s) to …` in the trace |
| `tests/apply.rs` | Six tests |

## 6. The code, explained

### 6.1 The three-way question (`checkpoint::apply_changes`)

```rust
let now = scan_one(target, rel);
let equals = |root: &Path, want: Option<&Entry>| -> Result<bool> { … same(root, target, rel, w, cur)? … };
if equals(work, new)? { out.already += 1; continue; }          // already the result
if !equals(base, old)? { problems.push(…); continue; }          // the user changed it
```

`same` compares kind, mode and, for files, content. Only paths that differ between base and workspace get this far, so a file the run did not touch is never looked at. `None` on both sides means "absent, as it should be".

### 6.2 Plan, then write

The loop only fills `ops` and `problems`. After it: any problem returns `Refused` and nothing has been written. A dry run returns here too. Then additions and changes go in path order (a folder before its contents), deletions deepest first.

### 6.3 One file

```rust
let (parent, name) = parent_of(&root, rel)?;                      // no symlink on the way
let fd = openat(&parent, tmp, O_WRONLY | O_CREAT | O_EXCL | O_NOFOLLOW | O_CLOEXEC, mode)?;
std::io::copy(&mut File::open(work.join(rel))?, &mut dst)?;
dst.set_permissions(mode)?; dst.sync_all()?;
renameat(&parent, tmp, &parent, name)?;                           // replaces the name in one step
```

`parent_of` opens the folder with `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS`, the same function the revert uses. `renameat` replaces the name itself: if a symlink sits there, the link is replaced, not what it points at.

### 6.4 A folder the run removed

It is removed with `rmdir`, which only removes an empty folder. If it still holds something (a denied file, something new), it stays and is listed as kept. Apply never deletes a file it did not know.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-108 | A separate command; refuse on any conflict before writing; safe handles; a log record | `run --apply`; calling `git apply`; `--force`; a three-way merge |

## 8. Problems hit

Only one, in the test: `[vec; 4]` needs a `Copy` type. The code passed its six tests on the first run. That says more about the revert code it reuses than about this milestone.

## 9. How to run it

```sh
agentos run --repo <repo> --policy <policy> --script <steps>     # or --agent llm
agentos apply <id> --dry-run                                     # the list
agentos apply <id>                                               # into <repo>
agentos apply <id> --to <another copy>
```

## 10. What this milestone does not do

- **Not verified on `fhs`.**
- **No merge.** If you edited a file the run also changed, apply refuses; use the patch.
- **No undo.** Apply into a clean git tree, so that `git status` shows exactly what came in and `git checkout`/`git clean` can take it out.
- **Folder ↔ file changes** are refused.
- **Not atomic across files.** All checks come first; a failure in the middle would need something like a full disk.
- **It does not judge what it copies.** Executable files, symlinks and CI files are listed, not vetted.
- **Binary files through the agent's own file tools** are still parked.

## 11. Check your understanding

1. What does design rule 10 protect, and why does a user-run command not break it while `run --apply` would?
2. Name the three states of a path that apply compares. In which two cases does it go ahead or skip, and what is the third?
3. Why are all paths checked before any is written?
4. An unrelated file in the repository was edited after the run. Why is that not a conflict?
5. Why does apply open the parent folder with `RESOLVE_NO_SYMLINKS` instead of just writing to `repo/path`?
6. Why write under a temp name and rename?
7. The run deleted a folder that, in the repository, still holds a `.env`. What does apply do, and why?
8. Why is `.pytest_cache` not applied?
9. Interview level: apply refuses instead of merging. When is refusing the better product decision, and what would you need before building the merge?
