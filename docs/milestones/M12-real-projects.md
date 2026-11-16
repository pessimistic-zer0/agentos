# M12 — What real projects still could not do

| | |
|---|---|
| Weeks in the plan | v1.0 (D-114); no week plan |
| Started / finished | 2026-11-16 / in progress |
| Commits | `ad2b302` … (tag `m12` only after a green CI run, D-113) |
| Status | In progress: B5 (`fs.edit`) built on `nix`; B4 and B6 not started |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it. Fill it in while working.

## 1. Aim

Three gaps the trials on real projects found (D-114). **B5:** an edit tool, so a model can change one line without sending the whole file. **B4:** a read-only folder inside the workspace (`node_modules`). **B6:** binary files and file modes through the agent's own file tools. B5 and B6 touch the **gateway** and the tools; B4 touches the **sandbox** and the **policy**.

## 2. Requirements

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| B5: an edit changes exactly the named text and nothing else | On `nix` yes; `fhs` not yet verified | `tests/edit.rs`: `an_edit_changes_one_place_and_nothing_else`, `an_edit_keeps_the_files_mode_and_works_on_text_that_is_not_ascii` |
| B5: an edit that fits no place, or more than one, changes nothing and says why | On `nix` yes | `an_edit_that_does_not_fit_one_place_changes_nothing_and_says_why`; unit tests `edit_tests` in `src/tools/fs.rs` |
| B5: policy and confinement as for `fs.write`, plus read access | On `nix` yes | `an_edit_needs_write_and_read_access_and_stays_in_the_workspace` (fails with the read check removed) |
| B5: a kill at any crash point leaves the edit not done or wholly done, and a resumed run applies it once | On `nix` yes | `an_edit_is_recovered_like_a_write`: 8 points × 50 kills by hand, 400 runs, 0 violations; reports 22 violations when recovery is deliberately broken |
| B5: a small model can use it | Yes, one run | Qwen3-VL-4B, ticket INV-102: 7 calls, 36 s, solved; three failures before (`docs/results/real-repo-trial-nix.md`) |
| B4: a read-only folder inside the workspace | Not started | |
| B6: binary files and file modes through the file tools | Not started | |

## 3. What you need to know first

**Why whole-file writes are a problem.** A model writes its reply token by token, up to a limit. To change one line of a 7 KB file with `fs.write` it must write all 7 KB back, exactly. Small models run out of reply, and any model can drop a line on the way.

**An edit as a write.** `fs.write` is safe under a kill because of how it is done (D-016): the intent records the file's hash before and the hash of the new content; the content goes to a temporary file, is forced to disk, and is renamed over the target in one step. Afterwards the file has either the old hash or the new one. An edit only adds a step *before* all that: read the file, replace the text in memory. From then on the new content is known, and it is a write.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| (none in `experiments/`) | The mechanism is string replacement feeding the existing write path; the open question was whether a small model can use the tool, which needs the tool (D-120) | The trial with the real model is in §2 | |

## 5. What was built

| File | Purpose |
|---|---|
| `src/tools/mod.rs` | `ToolCall::FsEdit`, `LoggedCall::FsEdit` (hashes and sizes of the two texts), `ToolOutput::Edit { bytes, line }` |
| `src/tools/fs.rs` | `edited`: the replacement and its refusals; `current_bytes` |
| `src/gateway.rs` | `decide` turns an allowed edit into an approved write of the whole new content |
| `src/recovery.rs` | An edit is reconciled by the rule for a write |
| `src/agent/llm.rs` | The tool `fs_edit`, the system prompt, the wording of results |
| `tests/edit.rs`, `tests/recovery/main.rs`, `tests/llm_agent.rs` | Tests |

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

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-120 | `fs.edit`, exactly one occurrence, turned into a write before the intent; read and write access needed | A separate effect and recovery rule; `replace_all`; loose matching; creating files; a diff tool |

## 8. Problems hit

None so far that changed the design. One clippy warning (a complex type in the new recovery test) was fixed before the commit.

## 9. How to run it

```sh
nix develop -c make test       # 146 passed on nix
systemd-run --user --scope -p Delegate=yes cargo test --test edit
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

- `fs.edit` replaces one occurrence; there is no "replace all" (parked).
- It edits UTF-8 text only (binary files: B6).
- The model trial is one run of one model on one ticket.

## 11. Check your understanding

(Filled in when the milestone is finished.)
