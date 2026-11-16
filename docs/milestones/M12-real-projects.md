# M12 — What real projects still could not do

| | |
|---|---|
| Weeks in the plan | v1.0 (D-114); no week plan |
| Started / finished | 2026-11-16 / 2026-11-16 |
| Commits | `ad2b302` … `0f140f0`; tag `m12` on `f38fff9`, the commit CI ran (D-129) |
| Status | Done |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it. Fill it in while working.

## 1. Aim

Three gaps the trials on real projects found (D-114). **B5:** an edit tool, so a model can change one line without sending the whole file. **B4:** a read-only folder inside the workspace (`node_modules`). **B6:** binary files and file modes through the agent's own file tools. B5 and B6 touch the **gateway** and the tools; B4 touches the **sandbox** and the **policy**.

## 2. Requirements

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| B5: an edit changes exactly the named text and nothing else | Yes, both presets (`fhs`: CI run 43894443267, D-125) | `tests/edit.rs`: `an_edit_changes_one_place_and_nothing_else`, `an_edit_keeps_the_files_mode_and_works_on_text_that_is_not_ascii` |
| B5: an edit that fits no place, or more than one, changes nothing and says why | On `nix` yes | `an_edit_that_does_not_fit_one_place_changes_nothing_and_says_why`; unit tests `edit_tests` in `src/tools/fs.rs` |
| B5: policy and confinement as for `fs.write`, plus read access | On `nix` yes | `an_edit_needs_write_and_read_access_and_stays_in_the_workspace` (fails with the read check removed) |
| B5: a kill at any crash point leaves the edit not done or wholly done, and a resumed run applies it once | Yes, both presets | `an_edit_is_recovered_like_a_write`: 8 points × 50 kills by hand on `nix`, 400 runs, 0 violations; 8 × 200 in CI on `fhs`, 1,600 runs, 0 violations (run 43894443267); reports 22 violations when recovery is deliberately broken |
| B5: a small model can use it | Yes, one run | Qwen3-VL-4B, ticket INV-102: 7 calls, 36 s, solved; three failures before (`docs/results/real-repo-trial-nix.md`) |
| B4: commands can use the folder and cannot change it; the original repository is untouched | Yes, both presets (CI run 43894443267, D-125) | `tests/rodirs.rs`: `commands_use_the_folder_and_cannot_change_it` (ten kinds of change refused; fails without the read-only flag) |
| B4: it is never copied: not in the workspace on the host, not in a checkpoint, not in the patch, not touched by `apply` | On `nix` yes | the same test |
| B4: the file tools read it and can never write it or leave it | On `nix` yes | `the_file_tools_read_it_from_the_repository_and_never_write_it` |
| B4: a denied path inside, a missing folder or a symlink stops the run before it starts | On `nix` yes | `a_denied_file_in_the_folder_stops_the_run_before_it_starts`, `a_folder_that_is_missing_or_a_symlink_is_refused` |
| B4: a mount point cannot be redirected | On `nix` yes | `a_command_that_swaps_the_way_to_the_mount_point_is_reverted`, `the_sandbox_refuses_a_mount_point_that_is_a_symlink` |
| B4: it is there again after a kill and a resume | On `nix` yes | `the_folder_is_still_there_after_a_kill_and_a_resume` |
| B4: the real case | Yes, one session | the Node project with `ro_dirs = ["node_modules"]` (`docs/results/second-stack-trial-nix.md`, last part) |
| B6: a binary file is read and written byte for byte | Yes, both presets (`fhs`: CI run 43903121563, D-129) | `tests/binary.rs`: `binary_files_are_read_and_written_byte_for_byte` (compared with the `base64` program); unit tests in `src/tools/base64.rs` |
| B6: an invalid request changes nothing and says why | On `nix` yes | `requests_that_are_not_valid_change_nothing_and_say_why` |
| B6: the executable bit can be set, cleared and seen, and is kept otherwise | On `nix` yes | `the_executable_bit_is_set_cleared_kept_and_shown` |
| B6: a kill around a write that only changes the mode is classified correctly | On `nix` yes | `a_kill_around_a_write_that_only_changes_the_mode_is_classified_by_the_mode` (fails with the mode comparison switched off) |

## 3. What you need to know first

**Why whole-file writes are a problem.** A model writes its reply token by token, up to a limit. To change one line of a 7 KB file with `fs.write` it must write all 7 KB back, exactly. Small models run out of reply, and any model can drop a line on the way.

**An edit as a write.** `fs.write` is safe under a kill because of how it is done (D-016): the intent records the file's hash before and the hash of the new content; the content goes to a temporary file, is forced to disk, and is renamed over the target in one step. Afterwards the file has either the old hash or the new one. An edit only adds a step *before* all that: read the file, replace the text in memory. From then on the new content is known, and it is a write.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `experiments/m12-repo-mount/` | Mounted a repository folder read-only onto an empty folder in a copy, in a private mount namespace, and tried nine ways to change it | All refused except renaming a parent folder; the host never saw the mount | The read-only flag is enough; the host's workspace stays empty there, so nothing that reads it needs to change |
| (none in `experiments/` for `fs.edit`) | The mechanism is string replacement feeding the existing write path; the open question was whether a small model can use the tool, which needs the tool (D-120) | The trial with the real model is in §2 | |

## 5. What was built

| File | Purpose |
|---|---|
| `src/tools/mod.rs` | `ToolCall::FsEdit`, `LoggedCall::FsEdit` (hashes and sizes of the two texts), `ToolOutput::Edit { bytes, line }` |
| `src/tools/fs.rs` | `edited`: the replacement and its refusals; `current_bytes` |
| `src/gateway.rs` | `decide` turns an allowed edit into an approved write of the whole new content |
| `src/recovery.rs` | An edit is reconciled by the rule for a write |
| `src/agent/llm.rs` | The tool `fs_edit`, the system prompt, the wording of results |
| `tests/edit.rs`, `tests/recovery/main.rs`, `tests/llm_agent.rs` | Tests for `fs.edit` |
| `src/tools/base64.rs` | Base64 encode and decode, by hand |
| `src/tools/mod.rs`, `src/tools/fs.rs`, `src/gateway.rs`, `src/recovery.rs` | `encoding` and `executable` on the existing tools; bytes instead of text on the write path; the mode in the intent and in recovery's check |
| `tests/binary.rs` | Tests for binary files and the executable bit |
| `src/policy.rs` | `filesystem.ro_dirs`, its validation, `ro_dir()`, the write refusal |
| `src/workspace.rs` | The empty folder in the copy; `check_ro_dir` refuses denied paths inside, a symlink, a missing folder |
| `src/sandbox/ns.rs` | The read-only mounts onto `/work/<folder>`, after checking the way to each mount point |
| `src/gateway.rs`, `src/runtime.rs` | The gateway knows the repository path; reads of paths in a read-only folder are opened beneath the repository |
| `tests/rodirs.rs` | Tests for `ro_dirs` |

```text
fs.edit {path, old_str, new_str}
   │ decide: path beneath the workspace? write allowed? read allowed?
   │         read the file; old_str exactly once?  ── no ──► FAILED, nothing changed
   ▼ yes
Approved::Write { whole new content }      ← from here on: fs.write
   intent (pre_hash, new_hash, temp) → temp file + fsync + rename → checkpoint → COMPLETED
```

## 6. The code, explained

**`tools::fs::edited(content, old_str, new_str, rel)`** returns the new text and the line where the replaced text began, or an error written for a model to act on.

```rust
let Some(at) = content.find(old_str) else { /* "was not found ... read the file again" */ };
let next = at + old_str.chars().next().map_or(1, char::len_utf8);
if let Some(more) = content[next..].find(old_str) { /* "more than once (at line A and at line B)" */ }
```

`find` gives the byte position of the first occurrence. The search for a second one starts one *character* after the first one begins, not after it ends, so overlapping occurrences count (`aa` fits `aaa` twice). It must step a whole character: Rust strings are UTF-8, a character can be several bytes, and slicing in the middle of one panics. `char::len_utf8` says how many bytes the first character has.

**`Gateway::decide`, the `FsEdit` arm.** It opens the target exactly as `fs.write` does (`open_for_write`: `openat2` beneath the workspace), checks the write rule, then the read rule, reads the file through the already opened folder handle (`current_bytes`, which refuses symlinks and non-regular files), and calls `edited`. Success gives `Approved::Write { target, content, edit_line: Some(line) }`. A failure gives `Decided::Fail`, which is logged as an intent that says "failed before any effect" followed by FAILED (D-040).

**Why read access too.** The errors say whether a text is in a file and on which lines. With write access only, an agent could ask "is `password = hunter2` in this file?" by trying edits.

**`handle`.** The two texts are put in the blob store before the request is logged, and the resulting file content before the intent, so every hash in the log can be looked up.

**Recovery.** One line: `LoggedCall::FsWrite { .. } | LoggedCall::FsEdit { .. }`. The intent has the same fields. If the edit was applied and the agent asks for it again after a resume, `old_str` is no longer in the file and the call fails cleanly, so an edit cannot land twice.

**`ro_dirs`, in the order things happen.**

1. *The policy is read* (`Policy::validate`): each entry must be a plain relative folder path, not matched by a deny rule, not inside another entry.
2. *The workspace is copied* (`workspace::copy_dir`): when the walk reaches such a folder it calls `check_ro_dir` (a real folder? nothing denied inside?), makes an empty folder in the copy and goes on without descending.
3. *A command starts* (`ns::build_root`): after `/work` is mounted, for each entry the sandbox walks from `/newroot/work` down the path with `symlink_metadata`, which does not follow links, and stops if any step is not a real folder. Then `bind(repo/<folder>, /newroot/work/<folder>)` and a remount with `MS_RDONLY`. A bind mount starts with the flags of where it came from; the remount is what makes it read-only.
4. *The agent reads* (`Gateway::decide`): the requested path is normalised as text (`lexical_rel`) only to choose the root: the workspace, or the repository if the path is in a read-only folder. The open itself is still `openat2` beneath that root, and the path the kernel resolved must still be inside the folder; a link in `node_modules` that points at `../src/x` is refused instead of quietly reading the repository's `src/x`.
5. *The agent writes*: `Policy::check_path(Write, …)` refuses before looking at the write globs.

**Binary files and modes (D-123).**

*Base64* (`tools/base64.rs`): three bytes are 24 bits; cut into four groups of 6 bits, each group is one of 64 characters. A last group of one or two bytes is padded with `=`. `encode` does exactly that with `chunks(3)`. `decode` goes the other way and is strict: wrong length, a foreign character, padding in the middle, or leftover bits that belong to no byte are errors.

*The write path carries bytes.* `Approved::Write.content` was a `String` and is now a `Vec<u8>`; text is just bytes that happen to be UTF-8. `ToolCall::write_bytes()` turns the request into those bytes once, and both the log (hash, size) and the gateway use it.

*The mode travels with the content.* `write_in` creates the temporary file, writes, sets the permission bits on it (`set_permissions`, which is `fchmod`), forces it to disk, then renames it over the target. There is no moment at which the new content has the old mode.

*Recovery.* A hash covers a file's bytes, not its mode. So when a write changes only the mode, "the file has the new hash" is true before and after. The intent now records `mode`, and `reconcile` asks for both: new hash *and* planned mode means APPLIED; otherwise the write did not happen.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-123 | Base64 for binary files, `executable` on `fs.write`, shown by `fs.list`; the mode in the intent | Hex; new tools; a free mode number; a crate |
| D-122 | `filesystem.ro_dirs`: the repository's folder bind-mounted read-only at its place; never copied; file tools read it from the repository | A frozen copy per execution; a writable mount; globs; hiding it from the file tools |
| D-120 | `fs.edit`, exactly one occurrence, turned into a write before the intent; read and write access needed | A separate effect and recovery rule; `replace_all`; loose matching; creating files; a diff tool |

## 8. Problems hit

1. One clippy warning (a complex type in the new recovery test for `fs.edit`) was fixed before the commit.
2. **A test that expected the wrong thing.** For `ro_dirs`, a test had a command replace the mount point with a symlink and expected the *next* command to fail in the sandbox's check. The next command ran normally: the revert after the first command had already put the folder back, because a read-only folder is never a writable path. The test now asserts the revert, and the sandbox's own check is tested through the library with a workspace made by hand. Findings row 2026-11-16.

## 9. How to run it

```sh
nix develop -c make test       # 161 passed on nix
systemd-run --user --scope -p Delegate=yes cargo test --test edit
systemd-run --user --scope -p Delegate=yes cargo test --test rodirs
AGENTOS_RECOVERY_RUNS=50 systemd-run --user --scope -p Delegate=yes cargo test --test recovery an_edit -- --nocapture
```

A script step:

```toml
[[step]]
tool = "fs.edit"
path = "src/stats.py"
old_str = "(len(values) - 1)"
new_str = "len(values)"
```

## 10. What this milestone does not do

- A binary file must fit in `max_output_kb` as base64 (about 48 KB with the shipped 64); there is no partial read of one. Only the executable bit can be set, not other permission bits.
- `fs.edit` replaces one occurrence; there is no "replace all" (parked).
- It edits UTF-8 text only.
- The model trial is one run of one model on one ticket.
- A folder in `ro_dirs` is the live original: not in checkpoints, not restored by a rollback, and a change made to it during a run is seen at once (`docs/threat-model.md` §5).
- Mounts below such a folder on the host are not carried in (the bind is not recursive).

## 11. Check your understanding

1. Why could a small model not change one line of a 7 KB file with `fs.write`?
2. `fs.edit` is "a write whose content the runtime works out". What does that buy for crash recovery?
3. Why must `old_str` occur exactly once, and why do overlapping occurrences count?
4. Why does `fs.edit` need read access as well as write access?
5. After a crash, an edit was APPLIED and the agent asks for it again. What happens, and why is that safe?
6. Why is `node_modules` mounted from the original repository instead of being copied? What is the price?
7. What makes the mounted folder read-only, and why can a command not undo it?
8. Why does a run refuse to start when a file in a read-only folder matches a deny rule?
9. A test expected a command to fail and it did not. Which defence had already acted, and how was the second one tested?
10. What is base64, and why is the decoded data's hash the one stored in the log?
11. Why is the file's mode set on the temporary file and not after the rename?
12. Why can recovery not tell from hashes alone whether a mode-only write happened? What was added to the intent?
