# M17 — What agents ran into

| | |
|---|---|
| Weeks in the plan | v2 (D-139); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` waits for a CI run |
| Commits | the M17 commit (tag `m17` after CI is green, D-113) |
| Status | Done. Verified on both presets and Debian 12 by CI run 45728656310 on `1571d09`; tagged there (D-158) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

The trials of v1.0 (a hosted model, then six capable agents through a relay) found the same few problems again and again: an agent could not finish in a project whose other tests already failed; a refusal did not say what was allowed; there was no way to search or to delete a file; a conversation too large for the server was told to the model three times. MCP clients meet these first, so v2 fixes them. It touches the gateway (two new tools), the policy (hints), recovery (one branch for deletes), and the built-in LLM agent.

## 2. Requirements

From `docs/v2-scope.md`, M17.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| C1 a test baseline; the stop rule refuses only when there is no progress or something new fails | Yes, `nix` | `cargo test --lib baseline` (2); `tests/llm_agent.rs` red-project tests (2); `tests/mcp.rs` (baseline note, and after a restart) |
| C2 a denial says what is allowed | Yes, `nix` | `tests/mcp.rs`: `refusals_say_what_is_allowed…` |
| C3 `fs.search` | Yes, `nix` | `tests/agent_tools.rs`: `fs_search_finds_lines_in_readable_files_only_and_skips_caches` |
| C4 `fs.delete`, with its own kill cells | Yes, `nix` | `tests/agent_tools.rs` (2); `a_delete_is_recovered_like_a_write` at 200 per point: 1,600 runs, 0 violations (`docs/results/recovery-delete-200-nix.md`) |
| C5 `replace_all` | Yes, `nix` | `tests/agent_tools.rs`: `fs_edit_with_replace_all…`; unit test in `src/tools/fs.rs` |
| C6 end at once on HTTP 413 | Yes, `nix` | `tests/llm_agent.rs`: `a_conversation_too_large_for_the_server…` |
| Both presets | `fhs` not yet | needs a push |

## 3. What you need to know first

**Baseline.** A reference result to compare with. Here: what a command failed the first time it ran, before the agent changed any file.

**Set difference.** For two sets of test names, "new failures" are the names in *now* that are not in *before*, and "fixed" the names in *before* that are not in *now*.

**`unlinkat`.** The system call that removes a name from a folder, given a handle on the folder and the name. Like `renameat` for a write, it acts on the name and never follows a symlink.

## 4. Experiments

None new: every item came from a trial already on record (`docs/results/relay-model-trial-nix.md`, `hosted-model-trial-nix.md`).

## 5. What was built

| File | Purpose |
|---|---|
| `src/baseline.rs` | Reading failing test names (pytest, cargo test, node's test runner); `Baseline` (first result before the first change); `Comparison` (new, fixed, progress, the note) |
| `src/agent/llm.rs` | The stop rule uses the baseline; results carry the comparison; schemas, parser and result text for the new tools; `Reply::TooLarge`; the system prompt |
| `src/mcp.rs` | `exec` results carry the comparison; the baseline is rebuilt from the log when an execution is continued |
| `src/policy.rs` | `allowed_hint` |
| `src/gateway.rs` | The hint on refusals; `Approved::Delete`, `Approved::Search`; their decide, plan and effect; `replace_all` passed through |
| `src/tools/mod.rs`, `src/tools/fs.rs` | The new calls and outputs; `fstool::delete`, `fstool::search`; `edited` with `replace_all` |
| `src/recovery.rs` | The `fs.delete` branch |
| `tests/agent_tools.rs` | The new tools through the scripted agent |
| `tests/recovery/main.rs` | `run_delete_trial`, `a_delete_is_recovered_like_a_write` |

## 6. The code, explained

**`baseline::failures`.** Takes a command's output and returns the names of the failing tests, or `None` if the command failed and no name could be read. Each runner prints failures its own way: pytest a summary line `FAILED tests/x.py::test_y - reason` (and `ERROR` for a file that could not be collected); cargo `test name ... FAILED`; node's TAP reporter `not ok 3 - name`, its spec reporter `✖ name (1.2ms)` (with a summary heading `✖ failing tests:` that is skipped). A command that exited 0 has no failures. Anything else is "unknown", and then the old rule applies: any failure means not done.

**`Baseline::observe` and `from_log`.** `observe` takes each call and its result in order. While no file tool has changed anything, the first result of each argv is stored. A successful `fs.write`, `fs.edit` or `fs.delete` ends that phase; a refused one does not. `from_log` does the same from the event log, so after a restart (a resumed run, or `norn mcp` continuing an execution) nothing is lost.

**`Comparison::progressed`.** No new failures, and fewer than before. The plan said "refuse only for new failures"; that alone would let an agent that changed nothing finish, so progress is required too (D-145).

**The stop rule (`LlmAgent::next_action`).** When the model answers in words while its last command failed, norn first asks the baseline. If that command has made progress, the run finishes as `completed`, and the comparison is added to the summary. Otherwise the D-107 rule runs as before: one reminder, then `stopped`. A first version never fired: `track_file_content` takes the agent's `last_call` out (`Option::take`), and the new code read it afterwards. It now copies the call first.

**`Policy::allowed_hint`.** Given the refused call and the rule that refused it: a deny rule says "denied on purpose"; a read-only folder says so; "no matching rule" gets the list that would have matched (command prefixes, write or read globs, services). The gateway adds it to the agent's answer only; the log's `tool.denied` keeps the rule.

**`fs.delete` in the gateway.** Opens the folder beneath the workspace exactly as `fs.write` does, checks the write rules, reads the current file (which refuses a symlink or anything that is not a regular file), and refuses a missing one. The intent records the path and the old content's hash, like a write. `fstool::delete` is one `unlinkat` through the folder's handle. Recovery: absent means applied (a checkpoint is taken), the old content means not applied, anything else rolls back to the last checkpoint.

**`fstool::search`.** A depth-first walk with a stack of (folder handle, path). Each folder's entries are read and sorted; each entry is looked at with `fstatat(…, AT_SYMLINK_NOFOLLOW)`, so a symlink is never a folder or a file to it. Folders are opened with `O_DIRECTORY | O_NOFOLLOW` through the folder above; files with `O_NOFOLLOW | O_NONBLOCK` and checked again with `fstat` (a FIFO swapped in cannot block the walk). The `include` function from the gateway applies the policy: deny rules and caches are skipped, files must be readable. Non-UTF-8 files and files over 1 MiB are skipped; at most 200 matches, lines cut at 200 characters.

**`Reply::TooLarge`.** The model server's 413 becomes its own kind of reply, and the agent ends the run as `stopped` with that reason. The run's patch is written as for any end.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-145 | Baseline from the first run before the first change; progress required | A test command in the policy run at start; "no new failures" alone; knowing the ticket's test |
| D-146 | Hints for the agent only; the log keeps the rule | The hint in the log; listing the deny rules |
| D-147 | `fs.delete` as a write-shaped intent; plain-text `fs.search` that applies the read rules per file; `replace_all` | Deleting folders; regular expressions; allowing `grep` |
| D-148 | 413 ends the run at once as `stopped` | `interrupted`; shortening the conversation |

## 8. Problems hit

1. The baseline never fired in the LLM agent: the call it needed had been taken by `track_file_content` (Findings). Found by the red-project test; fixed by copying the call first.
2. Two tests guessed facts about their data (the number of `values` in `stats.py`; `pytest` leaving `.pytest_cache`); both now take them from the files or set them up.
3. The trace prints at most 40 characters of a call's arguments, which cut the `replace_all` marker; its text is now shorter (`…, all)`).
4. An existing LLM test pins the exact list of tools offered to the model; it was updated to the eight-tool list on purpose, and the system prompt names the new tools.

## 9. How to run it

```sh
nix develop -c make test                                                   # everything, 223 on nix
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test agent_tools
nix develop -c env NORN_RECOVERY_RUNS=200 systemd-run --user --scope -p Delegate=yes \
  cargo test --test recovery a_delete_is_recovered -- --nocapture
```

## 10. What this milestone does not do

- The baseline reads three test runners; for any other command the old rule applies.
- A command that edits files before any file tool does moves its baseline later than "untouched".
- No folder deletes; no regular expressions in search.
- A conversation that grows too large still ends the run; shortening it stays parked.

## 11. Check your understanding

1. Why is the baseline taken from the agent's own first run instead of a run norn starts itself?
2. Why does a refused `fs.write` not end the baseline phase, but a completed one does?
3. What does "progress" require, and why is "no new failures" alone not enough?
4. Why is the hint added to the agent's answer and not to the log's `tool.denied` rule?
5. How does recovery decide whether a cut-off `fs.delete` happened? Why is that the same shape as for a write?
6. Why does `fs.search` use `fstatat` with `AT_SYMLINK_NOFOLLOW` and open files with `O_NONBLOCK`?
7. Why are caches skipped by search, although the agent may read them?
8. Why does a 413 end the run instead of telling the model?
