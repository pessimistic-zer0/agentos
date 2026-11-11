# M1 — Vertical slice

| | |
|---|---|
| Weeks in the plan | 1–2 |
| Started / finished | 2026-11-11 / 2026-11-11 |
| Commits | `7adf32c` … the commit that adds this file (tag `m1`) |
| Status | Done |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Build the whole path of one tool call, end to end, with no sandbox yet: an agent asks for a call, the **gateway** checks the **policy**, writes the step to the **event log**, runs the tool, and writes the result. `agentos trace` then reads the log back. The plan skips the sandbox here on purpose: getting the event model right is easier while the code is small. This milestone touches four of the five parts: gateway, policy, event log, and the first checkpoint (only the base copy used for the patch).

## 2. Requirements

Exit criterion from the v1 table: *"Scripted agent fixes the demo repo's failing test; `trace` prints every call."*

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| A demo repo with a failing `pytest` test, no downloads needed | Yes | `examples/demo-repo`; inside the workspace, call #2 (`pytest -q`) ends `exit 1` |
| The scripted agent fixes it | Yes | Call #7 (`pytest -q`) ends `exit 0`; test `scripted_agent_fixes_the_demo_repo` checks both exit codes and the patch content |
| `trace` prints every call | Yes | Test `trace_prints_every_call` checks lines `#1`…`#7`, a `DENY` and `finished: completed`; `--events` prints exactly one line per event |
| Four tools: `fs.read`, `fs.list`, `fs.write`, `exec` | Yes | All four appear in the demo trace below |
| No matching rule means deny | Yes | Unit test `no_rule_means_deny`; integration test `denied_command_and_unknown_path_are_reported` (`sh -c` and a write to `pytest.ini` are refused with `no matching rule`) |
| Denied files are not copied into the workspace (D-008) | Yes | `original_repo_is_never_modified_and_denied_files_are_not_copied` checks `work/.env` does not exist; unit test `denied_files_are_not_copied_and_links_stay_links` |
| The original repo is never modified (rule 10) | Yes | Same test compares a fingerprint of every file, mode and link in `examples/demo-repo` before and after |
| An intent is logged before every allowed effect (rule 3) | Yes | `log::calls` refuses COMPLETED without an intent (`illegal_transitions_are_reported`); every allowed call in the demo has a `tool.intent` |
| Commands run as argv with `execve`, never a shell (rule 8) | Yes | `tools/exec.rs` uses `std::process::Command` with an argv; `sh` is not in any allow rule |
| The output is a patch | Yes | `result.patch` applies to a fresh copy with `patch -p1 --dry-run` and `git apply --check` (both exit 0) |
| Each commit builds | **No, one does not** | Clean-checkout build of each commit: `c52f417` fails (`src/tools/mod.rs` not staged). Fixed in the next commit `14b60c6`. See §8 |

Run: `nix develop -c cargo test` → 15 unit tests and 4 integration tests pass. `cargo clippy --all-targets` → no warnings.

The last row is a process slip, not part of the exit criterion; history was not rewritten to hide it (CLAUDE.md forbids that).

## 3. What you need to know first

**Gateway.** A single function that every tool call goes through. Because the agent can only *return* a request, never act, the gateway is the one place where checks and logging happen. This is design rule 1.

**Policy, default deny.** The policy is a list of things that are allowed. Anything not on the list is refused. The opposite (a list of forbidden things) always misses something.

**Glob.** A path pattern. `*` matches part of one name, `**` matches any number of folders. `src/**` is everything under `src`.

**Event log, append-only.** A list of records that only grows. Nothing is changed or deleted, so the log is a full history. The current state of a call is worked out by reading its records in order ("event sourcing").

**Intent before effect.** Before doing anything, write down what you are about to do and make sure it is on disk (fsync). If the program dies, the log shows a step that started and did not finish. M3 builds recovery on this; M1 only makes sure the records are written in the right order.

**`execve` and argv.** A command is a list of words, `["pytest", "-q"]`. `execve` is the kernel call that starts a program with exactly that list. A *shell* (`sh -c "pytest -q"`) would instead parse a string, where characters like `;` or `$(...)` start other commands. Using argv only means there is nothing to inject into.

**Content hash and blob store.** SHA-256 turns any bytes into a 64-character fingerprint; the same bytes always give the same fingerprint. The log records fingerprints; the bytes are stored in files named by their fingerprint (a content-addressed store).

**Advisory lock.** `flock` puts a lock on a file. The kernel releases it automatically when the process dies, even on `kill -9`.

## 4. Experiments

The v1 table lists no experiments for weeks 1–2. Two small checks were run while building:

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| Apply the demo patch | `patch -p1 --dry-run` and `git apply --check` on a fresh copy of the demo repo | Both exit 0; the "Binary files … differ" lines for `__pycache__` are skipped | The `a/` `b/` header rewrite works; pytest changes files besides the source |
| TOML literal strings | Parse `fix-demo.toml` in Python's `tomllib` and compare the `fs.write` content with the fixed file | Identical; the newline right after the opening `'''` is dropped by TOML | Scripts can hold whole source files without escaping |

## 5. What was built

| File | Purpose |
|---|---|
| `Cargo.toml`, `Cargo.lock` | The `agentos` crate and its dependencies |
| `policies/nix.toml`, `policies/fhs.toml` | Default policies; they differ only in `[sandbox].ro_mounts` |
| `src/lib.rs` | Lists the modules; the logic lives in the library so tests can use it |
| `src/policy.rs` | Reads the policy, glob matcher, decisions that name their rule |
| `src/blob.rs` | SHA-256 and the content-addressed blob store |
| `src/log.rs` | The SQLite event log, the event types, rebuilding call states |
| `src/tools/mod.rs` | `ToolCall`, `LoggedCall`, `ToolOutcome`, `ToolOutput`, `ToolResult` |
| `src/tools/fs.rs` | `fs.read`, `fs.list`, `fs.write` (text path check for now) |
| `src/tools/exec.rs` | Program resolution, `exec` with a watchdog thread and output caps |
| `src/workspace.rs` | The execution folder layout; copying the repo without denied files |
| `src/checkpoint.rs` | Directory copies with `cp -a --reflink=auto`; the output patch |
| `src/gateway.rs` | requested → policy → intent → effect → completed/failed |
| `src/agent/mod.rs` | The `Agent` trait and `Action` |
| `src/agent/scripted.rs` | The scripted agent |
| `src/runtime.rs` | Set up an execution and run the agent loop; the lock |
| `src/trace.rs` | `trace` and `status` output |
| `src/main.rs` | The CLI: `run`, `status`, `trace` (`recover` comes in M3) |
| `examples/demo-repo/` | Python project with a failing test and a fake `.env` |
| `examples/scripts/fix-demo.toml` | The 7-step script that fixes it |
| `tests/common/mod.rs` | Test helpers: temp state folder, run the binary, read events |
| `tests/vertical_slice.rs` | The four M1 integration tests |

How the pieces talk:

```text
 main.rs ──► runtime::start ──► workspace::create (copy repo, skip denied)
                │                checkpoint::create (cp-0000)
                ▼
          runtime::drive  loop:
             agent.next_action(last result) ──► Action::Call(ToolCall)
                                                     │
                                                     ▼
             gateway.handle ──► log: tool.requested
                             ──► policy (decide)  ──► log: tool.denied ──► back to agent
                             ──► log: tool.intent (fsync)
                             ──► tools::fs / tools::exec (the effect)
                             ──► blobs: full output; log: tool.completed / tool.failed
                                                     │
             agent ◄──────────── ToolResult ◄────────┘
          Action::Finish ──► checkpoint::write_patch ──► log: execution.finished
```

## 6. The code, explained

### 6.1 `policy.rs` — what is allowed

```rust
pub fn check_path(&self, access: Access, rel: &str) -> Decision {
    if let Some(rule) = self.denied(rel) {
        return Decision::deny(rule);
    }
    let (field, globs) = match access {
        Access::Read => ("filesystem.read", &self.filesystem.read),
        Access::Write => ("filesystem.write", &self.filesystem.write),
    };
    match first_match(field, globs, rel) {
        Some(rule) => Decision::allow(rule),
        None => Decision::deny("no matching rule".into()),
    }
}
```

Deny rules are checked first, so they always win. If no allow rule matches, the answer is deny (rule 2). The `rule` string, such as `filesystem.deny[0] ".env"`, is what the trace prints. `if let Some(rule) = ...` is Rust's way to run code only when an `Option` holds a value.

The glob matcher works on path *components* (the parts between `/`):

```rust
fn match_components(pat: &[&str], path: &[&str]) -> bool {
    match pat.first() {
        None => path.is_empty(),
        Some(&"**") => (0..=path.len()).any(|skip| match_components(&pat[1..], &path[skip..])),
        Some(p) => !path.is_empty() && match_one(p.as_bytes(), path[0].as_bytes())
                   && match_components(&pat[1..], &path[1..]),
    }
}
```

`&[&str]` is a *slice*: a borrowed view of part of a list. `&pat[1..]` is "everything after the first item", without copying. For `**` the code tries letting it swallow 0, 1, 2, … components. Because `**` may swallow zero components, `secrets/**` also matches the folder `secrets` itself, which is why the whole folder is left out of the workspace (D-032).

`#[serde(deny_unknown_fields)]` is on every policy struct: a typo such as `memory_mib` stops the program instead of silently using a default. The test `bad_policies_are_refused` checks that and five other mistakes.

`exec` rules are prefixes:

```rust
if argv.len() >= rule.len() && argv[..rule.len()] == rule[..] { ... allow ... }
```

`["pytest"]` allows `["pytest", "-q"]`, but not `["/usr/bin/pytest"]` (a path is not the name) and not `["sh", "-c", "pytest"]`.

### 6.2 `log.rs` — the event log

The event types are one `enum`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
pub enum EventBody {
    #[serde(rename = "tool.requested")]
    ToolRequested { call_id: u64, call: Value },
    #[serde(rename = "tool.intent")]
    ToolIntent { call_id: u64, rule: String, plan: Value },
    ...
}
```

`#[serde(tag = "type")]` makes serde write each variant as a JSON object with a `"type"` field, e.g. `{"type":"tool.intent","call_id":6,...}`. This is called an *internally tagged* enum. Reading it back gives the right variant.

```rust
pub fn append(&mut self, body: EventBody) -> Result<u64> {
    let seq = self.next_seq;
    ...
    self.conn.execute("INSERT INTO events (seq, ts, type, call_id, body) VALUES (?1, ?2, ?3, ?4, ?5)",
        params![seq as i64, ts, body.type_name(), body.call_id().map(|c| c as i64), json])?;
    self.next_seq += 1;
    Ok(seq)
}
```

The runtime picks `seq` itself (last + 1), so the numbers have no gaps; M3's invariant 5 checks that. SQLite stores signed 64-bit integers, so `u64` values are converted to `i64` at this one boundary (`as` is a numeric cast). With `synchronous=FULL`, `execute` returns only after the row is on disk.

`log::calls` rebuilds each call's state by walking events in order, using a `match` on *(current state, next event)*:

```rust
rec.state = match (&rec.state, &ev.body) {
    (CallState::Requested, EventBody::ToolDenied { rule, .. }) => CallState::Denied { rule: rule.clone() },
    (CallState::Requested, EventBody::ToolIntent { plan, .. }) => CallState::IntentLogged { plan: plan.clone() },
    (CallState::IntentLogged { .. }, EventBody::ToolCompleted { output, checkpoint, .. }) => ...,
    (state, body) => bail!("event {}: `{}` is not allowed after state {state:?}", ev.seq, body.type_name()),
};
```

This is the state machine written as a table. The last arm catches every pair that is not listed, so an illegal order (COMPLETED straight after REQUESTED) is an error, not a silent mistake. `bail!` is `anyhow`'s "return an error now".

### 6.3 `tools/mod.rs` — requests and results

`ToolCall` is what the agent sends; `LoggedCall` is what the log stores. They differ only for `fs.write`: the log keeps `content_hash` and `bytes`, not the content (ADR 0004). `ToolOutcome` is how a call ended (`Ok`, `Denied`, `Failed`, `NotApplied`); `ToolOutput` is the data (file text, a listing, exec output). `ToolError` is a tool-level failure that goes back to the agent; `anyhow::Error` means the runtime itself broke and the run stops. Keeping the two apart is the point of having two error kinds.

### 6.4 `workspace.rs` — the copy

```rust
let child_rel = if rel.is_empty() { name.to_string() } else { format!("{rel}/{name}") };
if policy.denied(&child_rel).is_some() {
    report.denied.push(child_rel);
    continue;
}
...
let ft = entry.file_type()?; // does not follow symlinks
if ft.is_symlink() {
    let target = fs::read_link(&from)?;
    std::os::unix::fs::symlink(&target, &to)?;
}
```

Each path is checked against the deny globs *before* it is copied (D-008), and a denied folder is skipped with everything inside it. `file_type()` on a directory entry describes the entry itself; it does not follow a symlink. So a link to `/etc/passwd` is copied as a link and the file it points to is never read. A folder's permission bits are set after its contents are copied, so a read-only folder in the repo does not block the copy.

### 6.5 `tools/fs.rs` — file tools (temporary path check)

`resolve` turns the agent's path into a workspace-relative one by processing `.` and `..` as text. Its comment says plainly that this is **not** a security boundary: a symlink inside the workspace can still point outside, and the text check cannot see that. M2 replaces it with `openat2` (D-038).

`write` is atomic:

```rust
let mut f = fs::File::create(&tmp)?;
f.write_all(content.as_bytes())?;
f.sync_all()?;            // fsync: the new bytes are on disk
fs::rename(&tmp, &r.host) // one step: readers see the old file or the new one
```

The temp name is `.agentos-tmp-<call_id>`, recorded in the intent so recovery can delete a leftover (M3).

`(|| -> std::io::Result<()> { ... })()` is a closure that is called immediately. It lets the code use `?` for the four steps and then handle any error in one place (delete the temp file).

### 6.6 `tools/exec.rs` — running a command

`Programs::resolve` looks up each allowed program on the host `PATH` and follows symlinks to its real file (`canonicalize`). On this machine `pytest` becomes `/nix/store/…-python3-3.13.15-env/bin/pytest`. The command's `PATH` is built from those folders only.

```rust
cmd.arg0(&req.argv[0]).args(&req.argv[1..]).env_clear().envs(...)
```

`env_clear()` removes every inherited environment variable; only `PATH`, `HOME` and `LANG` are added back, so tokens in the runtime's environment never reach the command.

The watchdog is the first thread in the project (Rust book ch. 16):

```rust
let (done_tx, done_rx) = mpsc::channel::<()>();
let watchdog = std::thread::spawn(move || {
    if done_rx.recv_timeout(timeout).is_err() {
        let _ = nix::sys::signal::kill(Pid::from_raw(pid as i32), Signal::SIGKILL);
        return true;
    }
    false
});
let status = child.wait()?;
let _ = done_tx.send(());
```

`mpsc::channel` makes a pair: a sender and a receiver. The watchdog waits on the receiver for at most `timeout`. If the main thread sends "done" first, the watchdog exits quietly; if time runs out, it kills the command. `move` hands ownership of `done_rx` and `pid` to the new thread. Two more threads read stdout and stderr (`capture`): a command that prints a lot would block forever on a full pipe if nobody read it. They keep at most `max_output_kb` and throw the rest away.

The M1 watchdog kills by process id, and there is a tiny window where the command has just exited and the id could be reused. M2 kills the whole cgroup instead, which has no such window.

### 6.7 `gateway.rs` — one call, start to end

```rust
self.log.append(EventBody::ToolRequested { call_id, call: serde_json::to_value(call.logged())? })?;
let approved = match self.decide(&call) { ... denied → log tool.denied, return ... };
let plan = match self.plan(call_id, &approved) { ... };
self.log.append(EventBody::ToolIntent { call_id, rule: decision.rule.clone(), plan })?;
match self.effect(call_id, approved) {
    Ok(output) => { /* blob + tool.completed */ }
    Err(e) => { /* tool.failed */ }
}
```

`decide` returns a nested result: the outer error means "malformed request" (an empty argv, `..` climbing out), the inner one means "the policy says no". Both are logged as `tool.denied` before any intent (D-040). `plan` builds the intent's contents; for `fs.write` that is the path, the hash of the file now (`pre_hash`), the hash of the new content (`new_hash`) and the temp file name. That is everything recovery needs to decide later whether the write happened (D-016). For `fs.write`, the new content is put in the blob store before `tool.requested`, so every hash in the log can be looked up.

`Approved` is an enum holding the already-resolved path or argv. The effect uses exactly what the policy approved, not the raw request.

### 6.8 `agent/` and `runtime.rs` — the loop

```rust
pub trait Agent {
    fn next_action(&mut self, last: Option<&ToolResult>) -> Result<Action>;
    fn kind(&self) -> &'static str;
    fn task(&self) -> String;
}
```

A *trait* is a set of methods a type promises to have (like an interface). Both agents (scripted now, LLM in M4) implement it, and `runtime::drive` takes `&mut dyn Agent`: "any type that implements `Agent`", chosen at run time.

The scripted agent's whole rule:

```rust
let next = match (self.pos, last) {
    (None, _) => 0,
    (Some(p), Some(r)) if matches!(r.outcome, ToolOutcome::NotApplied { .. }) => p,
    (Some(p), _) => p + 1,
};
```

`if matches!(...)` is a *match guard*: the arm applies only when the extra condition holds. A NOT APPLIED result repeats the step; anything else moves on.

`runtime::start` sets up the folder, takes the lock, copies the policy and the repo, makes `cp-0000`, and calls `drive`. `drive` stops when the agent finishes or the call budget (`max_tool_calls`) is used up, then writes the patch.

### 6.9 `trace.rs` and `main.rs`

`trace` reads all events and prints one line per call using `log::calls`. `status` says `completed`, `running` (someone holds the lock) or `interrupted (run agentos recover)`. `main.rs` only parses arguments with `clap` and calls the library.

### 6.10 The tests

`tests/vertical_slice.rs` runs the real binary (`env!("CARGO_BIN_EXE_agentos")` is the path cargo gives integration tests) in a fresh state folder under `target/tmp`. It reads results through `agentos trace --json`, the same way a user's script would, so the tests also check the JSON output. `AGENTOS_TEST_PRESET=fhs` switches the policy preset; CI on Ubuntu will use it.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-032 | Own 30-line glob matcher | `glob` crate; `globset` crate |
| D-033 | One SQLite file per execution, one `events` table, JSON bodies, FULL sync on every event | Tables per type; one shared database; FULL only for intents |
| D-034 | No STARTED record; the intent marks the start (replaces D-006's state list) | Keep STARTED |
| D-035 | Patch made with `diff --no-dereference` | Plain `diff -ruN`, which follows symlinks |
| D-036 | One folder per execution; ids `YYYYMMDD-HHMMSS-xxxxxx` in UTC | `/tmp` workspaces; spec §10.3's split layout |
| D-037 | Scripted agent reads TOML `[[step]]`s; repeats NOT APPLIED steps | JSON scripts; steps in Rust code |
| D-038 | M1 file tools use a text check, to be replaced by `openat2` in M2 | `openat2` now; no check |
| D-039 | `flock` on `executions/<id>/lock` | PID file; no lock |
| D-040 | Malformed = DENIED before intent; allowed-but-impossible = intent + FAILED; `fs.write` never creates folders | An INVALID state; creating folders |
| D-041 | `fs.read` is UTF-8 text only, capped by `max_output_kb` | Truncating; base64 |

## 8. Problems hit

1. **`u64` and SQLite.** Expected `rusqlite` to store `u64` sequence numbers. The compiler said `u64: ToSql` is not implemented: SQLite integers are signed 64-bit, and a `u64` above `i64::MAX` would not fit. Fixed by converting at the database boundary (`seq as i64`). Found at first build.
2. **`Command::arg0` not found.** Setting a program's `argv[0]` separately from its path is a Unix-only feature, so the method lives in the `CommandExt` trait, which must be imported. A trait's methods are only callable when the trait is in scope. Found at first build.
3. **Bytecode in the patch.** Expected the patch to hold only `src/stats.py`. It also says "Binary files … differ" for two `__pycache__/*.pyc` files that pytest wrote. Both `patch` and `git apply` accept the patch. Not fixed: the files are inside the write globs (`**/__pycache__/**`), so they are allowed changes. M2 adds the revert of changes *outside* the write globs.
4. **Two mistakes in the commit series.** M1 was committed as a series of five commits made by a script, each supposed to build. Two mistakes: (a) the script rebuilt the decision log for each commit from the M0 version plus only that commit's entries, so `13da7df`, `0a2c113` and `c52f417` dropped D-032 again; (b) `c52f417` left `src/tools/mod.rs` unstaged, so it does not build. The script had checked the build in the working folder, not the commit. Found by building each commit in a clean `git worktree`. Fixed forward in `14b60c6` (history is never rewritten). Lesson: check commits from a clean checkout.

All four are in the Findings table of the decision log (1 and 2 are ordinary compile errors and are listed here only).

## 9. How to run it

From a fresh clone, with Nix:

```sh
nix develop -c cargo test        # 15 unit + 4 integration tests: ok
nix develop -c cargo run -- --state-dir /tmp/agentos-demo run \
    --repo examples/demo-repo --policy policies/nix.toml \
    --agent scripted --script examples/scripts/fix-demo.toml
```

Expected output (the id and times differ):

```text
20261111-135128-a29abb  RUNNING  policy default-nix  agent scripted
  workspace: 4 files copied, left out by deny rules: .env
  #1   fs.list  .                                        allow  4 entries
  #2   exec     pytest -q                                allow  exit 1, 0.2s
  #3   fs.read  tests/test_stats.py                      allow  304 bytes
  #4   fs.read  src/stats.py                             allow  554 bytes
  #5   fs.read  .env                                     DENY   filesystem.deny[0] ".env"
  #6   fs.write src/stats.py (548 bytes)                 allow  wrote 548 bytes
  #7   exec     pytest -q                                allow  exit 0, 0.2s
20261111-135128-a29abb  COMPLETED  script finished after 7 steps
  patch: /tmp/agentos-demo/executions/20261111-135128-a29abb/result.patch
```

Then:

```sh
nix develop -c cargo run -- --state-dir /tmp/agentos-demo trace            # one line per call
nix develop -c cargo run -- --state-dir /tmp/agentos-demo trace --events   # every event
nix develop -c cargo run -- --state-dir /tmp/agentos-demo status
```

On Ubuntu: use `policies/fhs.toml`; `pytest` must be on `PATH` (`apt install python3-pytest`). Not yet verified on Ubuntu.

## 10. What this milestone does not do

- **No sandbox.** `exec` runs directly on the host, in the workspace folder. Code run by `pytest` (`conftest.py`) can read and write anything the user can. M2 adds namespaces, cgroups and Landlock.
- **File tools are not confined.** The text path check misses symlinks (D-038). M2 replaces it with `openat2`.
- **No write-glob check for `exec`.** Changes outside the write globs made by commands are not reverted yet (D-009). M2/M3, once a checkpoint exists before each call.
- **No checkpoint per call, no recovery, no crash points.** Only the base checkpoint `cp-0000` exists. A crash leaves the execution `interrupted` with no way to continue. M3.
- **No LLM agent.** M4.
- **The `fhs` preset is not yet verified.** It needs the Ubuntu CI run, which happens only after Prince pushes.

## 11. Check your understanding

1. Why does the agent return an `Action` instead of calling the tools itself? What design rule does that enforce?
2. In `check_path`, why are deny rules checked before allow rules? What would go wrong the other way round?
3. Does `secrets/**` match the path `secrets`? Why does the answer matter for the workspace copy?
4. Why is `["pytest"]` allowed to match `["pytest", "-q"]` but not `["/usr/bin/pytest"]` or `["sh", "-c", "pytest"]`?
5. What is the difference between `tool.denied` and `tool.failed`? Which one can come after an intent?
6. Why is the new content of an `fs.write` stored in the blob store *before* `tool.requested` is logged?
7. What three things does the `fs.write` intent record, and what will recovery use each for?
8. Why is `fs.write` done as temp file + fsync + rename, and what would a crash leave behind if it wrote the target file directly?
9. Why does `exec` read stdout and stderr on separate threads? What happens to a command that prints 10 MB if nobody reads its pipe?
10. `resolve` in `tools/fs.rs` refuses `../etc/passwd`. Describe a path it accepts that still reaches a file outside the workspace.
11. Why is there no STARTED state? What would recovery gain from one?
12. What does `diff --no-dereference` prevent, and why is the patch step a place where host files could leak even though the agent has finished?
