# M9 — The parked conveniences

| | |
|---|---|
| Weeks in the plan | None. Asked for by Prince on 2026-11-14, after M8 |
| Started / finished | 2026-11-14 / 2026-11-15 |
| Commits | `b6fc0d6` … the commit that adds this file (tag `m9`) |
| Status | Done with gaps (not yet verified on `fhs`: these commits have not been pushed; two ideas held on purpose) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

> **Note, added 2026-11-15 after the tag:** verified on `fhs` by CI run 43691133260 on `d98eab1` (D-110). The first run, 43690755406, failed on a false note that only Ubuntu could show (B-014, fixed by D-109). The lines below that say "not yet verified" describe the state when this document was written.

## 1. Aim

M8 made AgentOS usable on a real project and wrote down, in the parking lot, twelve things that still got in a user's way. Prince asked why not build them all. M9 builds ten: the ones that were seen to cost a user time and that do not change what the runtime is allowed to do. Two are held: a command that writes into the original repository, and file tools for binary files.

It touches the **policy** (three new lists), the **gateway** and **checkpoints** (what counts as a change), the file tool `fs.read`, and mostly the **command line** around the five parts. The **event log** gains one optional field. The **sandbox** and **recovery** are unchanged. No tool is added.

## 2. Requirements

The requirement is the list Prince pasted back on 2026-11-14, with Claude's recommendation that he accepted ("as you wish"): build these, hold `apply` and binary tools (D-098).

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| A clean-up command | Yes | `agentos rm` (D-103). `tests/hardening.rs::rm_deletes_what_is_safe_to_delete_and_says_why_not_otherwise` |
| Leaving folders out of the copy | Yes, for folders that work from their own path | `filesystem.skip` (D-099). `tests/deps.rs::a_virtualenv_inside_the_repo_is_skipped_in_the_copy_and_still_used`. Not for `node_modules`; see §10 |
| Starting itself under `systemd-run` | Yes | D-105. `agentos_moves_itself_into_a_delegated_scope` |
| A message on Ctrl-C | Yes | D-106. `stopping_a_run_says_how_to_go_on_and_the_run_can_go_on` (INT, TERM, HUP) |
| A check command | Yes | `agentos check` (D-104). `check_reports_everything_without_starting_anything` |
| Reading part of a file | Yes | `fs.read` with `start_line`, `max_lines` (D-102). `part_of_a_large_file_can_be_read` |
| A hint when a mounted virtualenv is not the active one | Yes | D-101. `a_mounted_virtualenv_that_is_not_active_gets_a_note` |
| A narrower default deny list | Yes, by exceptions | `filesystem.deny_except` (D-100). `a_cache_is_scratch_and_an_example_env_file_is_not_a_secret` |
| Caches as non-violations | Yes | `filesystem.scratch` (D-099). The same test, and its control `without_a_scratch_rule_the_cache_is_reverted_and_reported` |
| A clearer end state when the model gives up | Yes, with mock models | D-107. Two tests in `tests/llm_agent.rs`. With the real 4B model the reminder was not reached (§8) |
| An apply command | **Held** | D-098: it writes into the original repo (design rule 10); needs Prince's word |
| Binary files and file modes through the file tools | **Held** | D-098: only useful after `apply` |
| Nothing that passed before got worse | Yes on `nix` | `make test`: 118 passed. 200-kill run: see `docs/results/recovery-200-nix-m9.md` |
| The real-project session again | Yes | `docs/results/real-repo-trial-nix.md`, second part: every block met in M8 is gone |
| Decision-log entries, threat model, README, this document | Yes | D-098 to D-107; threat model §5; README "Using it" and "On your own project" |
| Same on the `fhs` preset | **Not yet verified** | Needs a push and a green CI run |

Nothing was weakened or skipped to pass. One existing test changed on purpose: `without_a_delegated_cgroup_the_message_starts_with_what_to_do` now sets `AGENTOS_IN_SCOPE=1`, because without it agentos no longer shows that message; it moves itself into a scope instead.

## 3. What you need to know first

**Three questions about a path that used to be one.** "Is it a secret?" (`deny`), "should it be copied?" (`skip`), "does a change there matter?" (`scratch`). M8's trick of denying a virtualenv to keep it out of the copy answered the second question with the first one's word.

**A scope and its name.** systemd puts every started program into a named group (a scope unit). `systemd-run --scope` names it after the process id unless told otherwise, and then becomes the program it starts (`exec`), keeping that id.

**Signals.** The kernel's way of poking a process: SIGINT is Ctrl-C, SIGHUP is "your terminal went away", SIGTERM is a polite `kill`. A program may install a handler, a function that runs when one arrives, in the middle of whatever the program was doing. That is why a handler may only do very simple things (glossary: Async-signal-safe).

**An exit code** is the one fact about a finished command that nobody has to be believed about. 0 means it succeeded.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| The M8 session, repeated (`docs/results/real-repo-trial-nix.md`, second part) | The same 18 calls on the inventory project with a policy written for M9, no prefix | All eight blocks from M8 gone; the patch applied; 79 tests passed in a clone | The features do what they were built for, on the project that asked for them |
| The 4B model on the two-module ticket again | Whether the reminder helps a small model | 7 calls where there had been 1; then the server answered HTTP 500 to a cut-off reply and the run stopped with an error | A new limit, parked; D-107 itself stays untested with a real model |
| Self-start by hand from a terminal, then from a test | D-105 | Worked by hand; the test failed with "unit already loaded" | The scope needs its own name |

## 5. What was built

| File | Purpose |
|---|---|
| `src/policy.rs` | `deny_except`, `skip`, `scratch`; `Policy::skipped`, `scratch`, `command_may_change` |
| `src/workspace.rs` | The copy honours `skip`; `survey` (what the copy would hold); `all_execution_dirs`, `age_in_days` |
| `src/checkpoint.rs` | `write_patch` drops sections and notes for scratch paths |
| `src/gateway.rs` | The revert after a command spares scratch paths; `fs.read` passes its range on |
| `src/tools/mod.rs`, `src/tools/fs.rs` | `start_line`, `max_lines`; `ReadLines`; the one-pass line reader |
| `src/tools/exec.rs` | `Programs.notes`: the inactive-virtualenv note |
| `src/interrupt.rs` (new) | The signal handler and its prepared text |
| `src/runtime.rs` | `remove_execution`, `check`; tells `interrupt` what is going on |
| `src/main.rs` | `rm`, `check`; `enter_delegated_scope` |
| `src/agent/llm.rs` | The reminder and the `stopped` ending; the new `fs_read` arguments |
| `policies/*.toml` | `deny_except`, `skip = []`, `scratch = [".pytest_cache/**"]` |
| `tests/hardening.rs` (22 tests), `tests/deps.rs` (9), `tests/llm_agent.rs` (5) | The tests |

## 6. The code, explained

### 6.1 One question per list (`policy.rs`)

```rust
pub fn denied(&self, rel: &str) -> Option<String> {
    let rule = first_match("filesystem.deny", &self.filesystem.deny, rel)?;
    match first_match("filesystem.deny_except", &self.filesystem.deny_except, rel) {
        Some(_) => None,
        None => Some(rule),
    }
}
pub fn command_may_change(&self, rel: &str) -> bool {
    self.writable(rel) || self.scratch(rel)
}
```

Everything that asks "is this denied?" already went through `denied` (the copy, the file tools, the revert), so the exception takes effect everywhere at once. `scratch` first asks `denied`, so a denied path is never scratch. The gateway's revert uses `command_may_change`; the agent's own `fs.write` still uses `check_path`, which knows only the write globs.

### 6.2 Scratch paths leave the patch (`checkpoint.rs::write_patch`)

`diff`'s output is a run of sections, one per file. A text section starts with a `diff …` line; a binary one is a single `Binary files …` line. Both name the path twice, which is how the path is cut out without guessing where it ends:

```rust
let same_path_twice = |s: &str, between: &str| s[..s.len().saturating_sub(between.len()) / 2].to_string();
```

A flag `dropping` is set at each header and decides whether the following lines are kept. "The run changed something" used to mean "diff found a difference"; it now means "a section was kept, or a note was written".

### 6.3 Lines from a file of any size (`tools/fs.rs::read`)

One pass with `read_until(b'\n')`, one line in memory at a time. Lines in the range are kept while they fit in `max_output_kb`; all lines are counted, so the answer can say `lines 1-20 of 2614`. If the first wanted line alone is over the limit, that is an error with the line number. Nothing is cut in the middle of a line, and the answer says that it is a part.

### 6.4 Starting itself in a scope (`main.rs::enter_delegated_scope`)

```rust
let _ = Command::new("systemd-run").arg(unit).args(scope).arg(me)
    .args(std::env::args_os().skip(1)).env("AGENTOS_IN_SCOPE", "1").exec();
```

`exec` replaces this process with `systemd-run`, which makes the scope and replaces itself with agentos again. One process id throughout; no parent in between. Before that, the same `systemd-run` is tried with `true`, because once `exec` succeeds there is no coming back to print a better error.

### 6.5 The signal handler (`interrupt.rs`)

```rust
extern "C" fn on_signal(sig: libc::c_int) {
    let (ptr, len) = (TEXT.load(Ordering::SeqCst), LEN.load(Ordering::SeqCst));
    unsafe {
        if !ptr.is_null() && len > 0 { libc::write(2, ptr.cast(), len); }
        libc::_exit(128 + sig)
    }
}
```

Two system calls. The text was built earlier, by ordinary code, and left in memory for good (`Box::leak`), because the handler may not allocate. `set` stores length 0, then the pointer, then the length, so a signal arriving in between prints the old text or nothing, never past the end of a buffer.

Why not "finish the call and exit cleanly": that would be a second way of stopping. The first way, stopping dead, is the one that 4,800 kills test.

### 6.6 Deleting an execution (`runtime::remove_execution`)

Order: is the id a plain folder name; does the folder exist; is it locked (someone is running it); did it finish (else `--force`); take the lock; `remove_tree`. The age for `--older-than` comes from the date in the id, turned into a day number by counting from March so that the leap day falls at the end of a year.

### 6.7 "Done" needs a command that succeeded (`agent/llm.rs`)

```rust
if self.last_command_failed {
    if self.nudged_to_go_on { return Ok(Action::GiveUp { reason: … }); }
    self.nudged_to_go_on = true;
    self.push(json!({"role": "user", "content": format!("{GO_ON_MARK}, but the last command you ran did not succeed …")}));
    continue;
}
```

Both flags are rebuilt from the log when a run is resumed: the first from the last command's stored result, the second by looking for the reminder's opening words among the logged messages.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-098 | Open scope for ten parked ideas; hold `apply` and binary tools | Build all twelve; a git-format patch; a mount inside the workspace |
| D-099 | `skip` and `scratch` | Using `deny` for both; caches in the write globs; scratch outside checkpoints |
| D-100 | `deny_except` | Narrowing the deny globs; `!pattern` |
| D-101 | A note for a mounted but inactive virtualenv | Refusing to start; activating it automatically |
| D-102 | `fs.read` with a line range | A higher limit; byte offsets; a new tool; silent cutting |
| D-103 | `agentos rm`, never automatic | Automatic clean-up; deleting only checkpoints |
| D-104 | `agentos check` | `run --dry-run`; stopping at the first problem |
| D-105 | agentos `exec`s itself through `systemd-run` | A child process; D-Bus; keeping the prefix |
| D-106 | A message, then stop like a crash | Finishing the call first; rolling back in the handler |
| D-107 | One reminder, then `stopped` | Trusting the model's words; reminding until the budget is gone |

## 8. Problems hit

1. **A moved value in `Programs::resolve`** while adding the note (compile error; fixed before any test ran).
2. **A wrong expectation in the range test:** the demo's `stats.py` has 19 lines, not 21. The code was right.
3. **Self-start collided with its own scope's name** when agentos was started by `systemd-run --scope`, as tests and CI do: `Unit run-p….scope was already loaded`. Found by the new test, not by hand (D-105).
4. **The real model did not reach the reminder.** It went further than in M8, then its reply was cut off inside a tool call, its server answered HTTP 500, and the run ended with an error and an `interrupted` execution. Not fixed: telling a passing fault from a permanent one needs thought. Parked.
5. **An edit script stopped half-way through `main.rs`** (an anchor did not match), leaving the new commands declared but not handled; the build said so at once.

## 9. How to run it

```sh
nix develop -c make test                       # 118 tests
agentos check --policy policies/nix.toml --repo examples/demo-repo
agentos run --repo examples/demo-repo --policy policies/nix.toml --script examples/scripts/fix-demo.toml   # no prefix
agentos rm --dry-run --older-than 14
```

Press Ctrl-C during a run to see the message; `agentos run --resume <id>` goes on.

A policy for your own project, with the new lists: README, "On your own project".

## 10. What this milestone does not do

- **Not verified on `fhs`** until a CI run on these commits is green.
- **`skip` does not help `node_modules`**: a skipped folder is not in the workspace, and Node looks for it there. That needs a read-only folder inside the workspace. Parked.
- **`apply` and binary file tools** are held (D-098).
- **An error from the model's server stops the run** (§8, item 4). Parked.
- **D-107 was not seen with a real model**, only with scripted ones.
- **The reminder knows one sign of "not done"**: the last command failed. A model that never runs a command, or edits after a passing run and stops, still ends as `completed`.
- **Benchmarks were not run again** after M9; the startup path has one more cgroup check.

## 11. Check your understanding

1. `deny`, `skip` and `scratch` each answer a different question about a path. Which three?
2. Why was denying `.venv/**` in M8 the wrong word for the right effect?
3. A cache is in `scratch`. Is it in the checkpoint? In the patch? May the agent write to it with `fs.write`? Why each?
4. Why does `deny_except` exist instead of a narrower deny pattern?
5. `fs.read` now returns part of a file. Why must the answer say so, and what goes wrong with `fs.write` if it does not?
6. Why does agentos `exec` systemd-run, and not start it as a child and wait?
7. The self-start worked from a terminal and failed in a test. What was different, and what does it say about trying things by hand?
8. What two things does the signal handler do, and why nothing more?
9. Why is "stop like a crash" the safer design for Ctrl-C in this project in particular?
10. `agentos rm` refuses an interrupted execution without `--force`. What would be lost?
11. What does the runtime use as evidence that a model is not done, and why not the model's own words?
12. Interview level: M9 added ten features to a project whose strength was a short, fully tested list of claims. How would you decide which of them deserve a place in the README's claims?
