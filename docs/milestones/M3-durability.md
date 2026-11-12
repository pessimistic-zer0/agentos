# M3 — Durability

| | |
|---|---|
| Weeks in the plan | 6–8 |
| Started / finished | 2026-11-11 / 2026-11-12 |
| Commits | `dfdc16b` … the commit that adds this file (tag `m3`) |
| Status | Done with gaps (the `fhs` preset is not yet verified) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Make the runtime survive being killed at any moment. Before M3, a `kill -9` left an execution "interrupted" with no way to tell what the last call had done. After M3, every call that changes the workspace is followed by a save point (checkpoint); after a crash, `agentos recover` decides for the cut-off call whether it happened, repairs the workspace, and writes its verdict to the log; `agentos run --resume` then continues. This is the **checkpoints + recovery** part, and it completes the **event log**'s state machine. It is the project's headline claim, so it comes with a harness that kills the runtime thousands of times and checks the result.

## 2. Requirements

Exit criterion from the v1 table: *"P1–P8 pass all invariants over 200 runs each."*

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| P1–P8, 200 runs per point and tool, all five invariants (`nix` preset) | Yes | `nix develop -c make eval-recovery`: 4,800 runs, 4,788 killed, **0 invariant violations**. Table in `docs/results/recovery-200-nix-m3.md`, raw report in `docs/results/recovery-200-nix-m3.json` |
| 0 duplicated side effects | Yes | Same run: 0 duplicated effects |
| Cut-off calls classified correctly | Yes | Same run: 3,866 of 3,866 (the other runs had no call open at the moment of the kill, for example all P7 runs) |
| Recovery is re-entrant (P8) | Yes | Cells `P8 fs.write` and `P8 exec`: 400 runs, recovery itself killed in 315 of them, 0 violations |
| Recovery time p50 / p99 reported | Yes | p50 19.1 ms, p99 49.8 ms (wall time of the whole `agentos recover` process, debug build, 8 trials in parallel; see §9) |
| The harness can fail | Yes | Two bugs seeded into recovery, one at a time: "no rollback for exec" → 31 duplicated effects in 96 runs; "keep orphan checkpoints" → 21 violations in 96 runs. Restored code → 0 |
| Intent fsynced before the effect (rule 3) | Yes | I1 check in the harness (crashes at P1: workspace equals the last checkpoint); `synchronous=FULL` (D-033) |
| effect → checkpoint → COMPLETED naming it (rule 4) | Yes | I2 check: every completed mutating call names an existing checkpoint that holds its effect |
| UNCERTAIN call never re-executed before reconciliation (rule 5) | Yes | I4 check on record order; `run --resume` always recovers first |
| `agentos recover` exists; CLI now has all four commands | Yes | `agentos recover <id>`; `agentos run --resume <id>` |
| Experiments run first | Yes | `experiments/m3-rename-kill`, `experiments/m3-sqlite-kill` |
| Same on the `fhs` preset | **Not yet verified** | Needs the Ubuntu CI run after a push |
| Earlier suites still pass | Yes | `make test` → 17 unit, 25 security, 4 vertical-slice and the quick recovery run (72 kills) all pass; `cargo clippy --all-targets` clean |

What the harness does **not** show: anything about power loss. It kills a process; data the kernel has accepted survives that with or without fsync (D-022).

## 3. What you need to know first

**Why "uncertain" exists.** If a program dies between "start doing X" and "X is done", an outside observer cannot tell from the program alone whether X happened. The only honest states are: it did not start, it finished, or we do not know. Recovery's job is to turn "do not know" into one of the other two *by looking at the world*, never by guessing and never by simply doing X again.

**Intent, effect, completion.** Three moments for each call: the intent record is written and fsynced; the effect happens; the completion record is written. A crash can fall into any gap. The crash points P1–P7 are those gaps, named.

**Checkpoint.** A full copy of the workspace folder, made with `cp -a --reflink=auto`. On btrfs and XFS a reflink copy shares disk blocks with the original, so it is fast and small until files change.

**Two stores, one truth.** The log (SQLite) and the checkpoints (folders) cannot be updated in one step. The rule that keeps them consistent: only what the log names counts. A checkpoint folder without a log record is garbage; a COMPLETED record is never written before its checkpoint exists.

**Idempotent.** An operation is idempotent if doing it twice gives the same result as doing it once. Recovery is built only from such steps, because recovery itself can be killed and run again.

**Atomic exchange.** `renameat2(..., RENAME_EXCHANGE)` swaps two names in one kernel step. Nothing can observe a state in between.

**Oracle.** In testing, the thing that knows the right answer. A good oracle is independent of the code under test.

Glossary: `docs/decision-log.md`.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m3-rename-kill` | Replace a 2 MB file in a loop, kill at random: write in place vs temp + rename | In place: torn file after 149 of 200 kills. Temp + rename: 0 of 200, but the temp file was left behind 164 times | Atomic rename works; recovery must delete the temp file, so the intent stores its name |
| `m3-sqlite-kill` | Kill a writer in the middle of 3-row transactions, 200 times | 0 corrupt databases, 0 gaps, 0 visible half transactions, 0 lost acknowledged commits | SQLite gives invariant 5 for free; this is the evidence D-005 was waiting for |
| Seeded bugs (mutation check) | Break recovery on purpose, run the harness | Both bugs caught (see §2) | The harness is able to fail |

## 5. What was built

| File | Purpose |
|---|---|
| `src/faults.rs` | Crash points: `point` (die now) and `arm` (die after a random delay), driven by `AGENTOS_CRASH_*` |
| `src/recovery.rs` | `recover`: kill leftovers, delete orphans, settle each cut-off call; `history` for resuming |
| `src/checkpoint.rs` | `restore` (copy + atomic exchange), `remove_leftovers`, `on_disk`, `number` |
| `src/gateway.rs` | Checkpoint after each mutating call; crash points P1–P7; cgroup path in the exec intent |
| `src/log.rs` | `recovery.started`, `tool.reconciled`, `recovery.completed`; `CallState::Reconciled`; `named_checkpoints`; `check_gap_free` |
| `src/runtime.rs` | `recover`, `resume`, `agent_kind`; `drive` takes the calls made so far |
| `src/agent/mod.rs`, `scripted.rs` | `persist` and `restore` |
| `src/main.rs` | `agentos recover <id>`, `agentos run --resume <id>` |
| `src/trace.rs` | Shows `RECOVERED, APPLIED` / `RECOVERED, NOT APPLIED` |
| `tests/recovery/main.rs` | The fault-injection harness |
| `Makefile` | `make test` → 17 unit, 25 security, 4 vertical-slice and the quick recovery run (72 kills) all pass; `cargo clippy --all-targets` clean, `make eval-recovery`, `make eval-security`, `make eval` |
| `experiments/m3-*` | The two experiments |

The life of one mutating call, with the crash points:

```text
 tool.requested ─ policy ─┬─ P1 ─ tool.intent (fsync) ─ P2 ─┬─ effect ──┬─ P4 ─ checkpoint ─┬─ P6 ─ tool.completed ─ P7 ─► agent
                          │                                 │   (P3:     │        (P5: killed │        names cp-000N
                     (tool.denied)                          │   killed   │         during the │
                                                            │   during)  │         copy)      │
 crash here ⇒ recovery:   P1: no intent ⇒ NOT APPLIED       P2..P6: UNCERTAIN ⇒ reconcile     P7: nothing to do
```

Recovery, in order:

```text
 lock ─ read log, check no gaps ─ kill leftover cgroup ─ delete .tmp copies and unnamed checkpoints
      ─ recovery.started ─ for each cut-off call: decide (and roll back or checkpoint) ─ tool.reconciled
      ─ recovery.completed
```

## 6. The code, explained

### 6.1 `faults.rs` — dying on purpose

```rust
pub fn point(name: &str, call_id: Option<u64>) {
    if matches(name, call_id).is_some() {
        die();
    }
}
fn die() -> ! {
    let _ = nix::sys::signal::kill(nix::unistd::getpid(), nix::sys::signal::Signal::SIGKILL);
    std::process::abort()
}
```

`-> !` is the "never returns" type. SIGKILL cannot be caught or delayed: the kernel removes the process at once, no Rust destructors run, nothing is flushed. That is the same as `kill -9` from another terminal. `matches` compares the point name and the call id with the environment variables and counts hits, so a test can say "the third time P8 is reached".

The configuration is read once into a `OnceLock` (a cell that is set at most once and then shared between threads). `arm` starts a thread that sleeps for a random part of `AGENTOS_CRASH_DELAY_US` and then calls `die()`, so the kill lands somewhere inside whatever the main thread is doing: the temp-file write, the sandboxed command, the checkpoint copy.

### 6.2 `gateway.rs` — where the points are

```rust
faults::point("P1", Some(call_id));
self.log.append(EventBody::ToolIntent { call_id, rule: decision.rule.clone(), plan })?;
faults::point("P2", Some(call_id));
faults::arm("P3", Some(call_id));

match self.effect(call_id, approved)? {
    Ok(output) => {
        let checkpoint = if mutating {
            faults::point("P4", Some(call_id));
            let name = checkpoint::name(checkpoint::number(&self.last_checkpoint).unwrap_or(0) + 1);
            faults::arm("P5", Some(call_id));
            checkpoint::create(&self.dir.work(), &self.dir.checkpoints(), &name)?;
            faults::point("P6", Some(call_id));
            Some(name)
        } else { None };
        ...
        self.log.append(EventBody::ToolCompleted { call_id, summary, output: blob, checkpoint: checkpoint.clone() })?;
        if let Some(name) = checkpoint { self.last_checkpoint = name; }
        faults::point("P7", Some(call_id));
```

The order of the real statements is design rule 4: effect, then `checkpoint::create`, then the COMPLETED record that carries the checkpoint's name. `self.last_checkpoint` changes only after the record is written, so in memory, too, "last checkpoint" means "last one the log names".

`checkpoint::create` (from M1) copies to `.tmp-cp-000N` and renames to `cp-000N`. A kill during the copy leaves only the `.tmp-` folder; a kill after the rename but before COMPLETED leaves an *orphan* `cp-000N`.

### 6.3 `log.rs` — the new records

```rust
#[serde(rename = "tool.reconciled")]
ToolReconciled { call_id: u64, applied: bool, reason: String, checkpoint: Option<String>, restored: Option<String> },
```

`applied` is the verdict. `checkpoint` is set when recovery took the missing checkpoint for an applied write; `restored` when it rolled back. In `log::calls`, a `tool.reconciled` is legal after `Requested` or `IntentLogged` and makes the call terminal:

```rust
(CallState::Requested | CallState::IntentLogged { .. }, EventBody::ToolReconciled { applied, reason, checkpoint, .. }) =>
    CallState::Reconciled { applied: *applied, reason: reason.clone(), checkpoint: checkpoint.clone() }
```

`A | B` in a pattern means "either". `named_checkpoints` collects, in log order, the checkpoint named by `workspace.created`, each `tool.completed` and each `tool.reconciled`. Its last element is *the* last checkpoint.

### 6.4 `checkpoint::restore` — rollback that can be interrupted

```rust
let tmp = restore_tmp(work);                 // .../work.restore-tmp
if tmp.exists() { std::fs::remove_dir_all(&tmp)?; }
run_cp(cp, &tmp)?;                           // copy the checkpoint next to work/
faults::point("P8", None);
nix::fcntl::renameat2(AT_FDCWD, &tmp, AT_FDCWD, work, RenameFlags::RENAME_EXCHANGE)?;
faults::point("P8", None);
std::fs::remove_dir_all(&tmp)?;              // tmp now holds the old workspace
```

Before the exchange, `work/` is untouched. After it, `work/` is the checkpoint's content. A kill at either P8 leaves a state from which running `restore` again ends in the same place. The obvious version (delete `work/`, copy into it) has a window with a missing or half-filled workspace.

### 6.5 `recovery.rs` — the verdicts

`recover` first refuses a log with a gap, returns early for a finished execution, and collects the calls without an ending. Then, before changing any file:

```rust
for c in &pending {
    if let CallState::IntentLogged { plan } = &c.state {
        if let Some(path) = plan.get("cgroup").and_then(Value::as_str) {
            kill_leftover_cgroup(Path::new(path))?;
        }
    }
}
```

A sandboxed command normally dies with the runtime, but not instantly. Killing its cgroup and waiting until it is empty guarantees nothing is still writing to `/work` (D-055).

Next come the leftovers: `.tmp-*` copies, `work.restore-tmp`, and every checkpoint folder the log does not name. If nothing was cut off and nothing was left over, recovery returns "clean" without writing to the log (a crash at P7 looks like this).

`reconcile` is the table from the v1 design:

```rust
match &c.call {
    LoggedCall::FsRead { .. } | LoggedCall::FsList { .. } => Ok(not_applied("read-only call; nothing to undo")),
    LoggedCall::Exec { .. } => rollback("a command's effects cannot be known after a crash"),
    LoggedCall::FsWrite { .. } => {
        ...
        match inspect_write(&dir.work(), &path, &temp)? {
            Some(Some(h)) if h == new_hash => { /* APPLIED: checkpoint::create now */ }
            Some(now) if now == pre_hash => Ok(not_applied("file still has its old content; the write did not happen")),
            Some(_) => rollback("file matches neither the old nor the new content"),
            None => rollback("the file's folder could not be inspected safely"),
        }
    }
}
```

`inspect_write` opens the file's folder beneath the workspace (the same `openat2` helper as the file tools), deletes the leftover temp file named in the intent, and hashes the target without following a symlink. The three-way comparison is the interesting part of the whole design: the intent stored the hash before (`pre_hash`) and the hash after (`new_hash`), so recovery can *observe* which side of the rename the crash fell on. `Some(Some(h))` is "a file with hash h"; `Some(None)` is "no file", which equals a `pre_hash` of `None` for a brand-new file.

The order inside the loop is what makes P8 safe:

```rust
let v = reconcile(dir, c, &last, &named)?;     // do the work (rollback or checkpoint)
faults::point("P8", None);
log.append(EventBody::ToolReconciled { ... })?; // only then record the verdict
```

If recovery dies before the record, the call is still pending next time and is reconciled again. A second rollback restores the same checkpoint. A second "applied" check finds the orphan checkpoint already deleted by step 2 and takes it again.

### 6.6 `runtime.rs` and the agent — carrying on

```rust
let report = crate::recovery::recover(&dir, &mut log)?;   // always first (rule 5)
...
let history = crate::recovery::history(&events)?;
agent.restore(&history, &events)?;
drive(&mut gw, agent, history.last().cloned(), history.len() as u64, verbose)
```

`history` is one `ToolResult` per earlier call, built from the log: `Ok`, `Denied`, `Failed`, or for reconciled calls `Ok` (applied) or `NotApplied`. The scripted agent's `restore` replays its own rule over that list:

```rust
self.pos = None;
for i in 0..history.len() {
    let prev = if i == 0 { None } else { Some(&history[i - 1]) };
    self.next_action(prev)?;
}
```

After the loop the agent stands on the step that produced the last result; `drive` then hands it that last result, and it either repeats the step (NOT APPLIED) or moves on. Nothing about the agent's position is stored anywhere except the log.

### 6.7 `tests/recovery/main.rs` — the harness

The workload (`script_text`): list, write `src/a.txt` (400 KB), exec, read, write `src/a.txt` again, exec, write `src/new.txt`. Each exec appends one line to `src/counter.txt`, sleeps 30 ms, appends one line to `src/side.txt`.

`run_trial` for one (point, tool, n):

1. Pick one of the tool's calls at random; run with `AGENTOS_CRASH_AT`, `AGENTOS_CRASH_CALL`, `AGENTOS_CRASH_DELAY_US`. Expect the process to die with signal 9.
2. **Before recovery**, read the log and the files. If no call has an open intent, the workspace must equal the last named checkpoint (**I1**). For the cut-off call, decide from the files alone whether its effect is there (`expect_applied`).
3. For P8: run `recover` with `AGENTOS_CRASH_AT=P8` and a random hit number 1–5, expect it to die; then run `recover` normally, timing it.
4. `check_invariants`: sequence numbers 1..n (**I5**); no call requested while another is unsettled, none left unsettled (**I4**); every completed or applied mutating call names an existing checkpoint containing its write (**I2**); workspace equals the last checkpoint and equals the model (**I3**); no unnamed checkpoints or temp folders.
5. Compare recovery's verdict with `expect_applied`.
6. `run --resume` to the end; check the invariants again; the final files must be exactly what an uninterrupted run writes, with `counter` and `side` at 2 lines each. Extra lines are counted as duplicated effects.

`model` is the independent oracle: it walks the log, and for each call that ended applied it records the file hash (writes) or adds one (execs). It never asks the runtime what the workspace should be.

Trials run on 8 threads with `std::thread::scope`, which lets threads borrow local variables because the scope guarantees they finish before it returns. An `AtomicUsize` hands out job numbers; a `Mutex<Vec<_>>` collects results.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-052 | A test crash is SIGKILL to itself, chosen by environment variables | `abort()` (core dumps); killing only from outside; test-only builds |
| D-053 | Rollback = copy + `renameat2(RENAME_EXCHANGE)` | Delete then copy; two renames; in-place sync |
| D-054 | Checkpoint after every mutating call; unnamed checkpoints deleted; no fsync of checkpoint data | Skip when unchanged; fsync everything |
| D-055 | Recovery kills the leftover cgroup by the path in the intent, on top of `PR_SET_PDEATHSIG` | Parent-death signal only; pids in the log |
| D-056 | `run --resume`; agent placed by replaying results from the log | `recover` always continues; a position side file |
| D-057 | Harness with an independent oracle, a mutation check, 24 cells | Exit-code-only checks; P4–P6 for reads |

## 8. Problems hit

1. **The harness passed on its first run.** Expected at least one failure from new code this size; got 72 of 72. A pass that early is a reason for suspicion, not relief. Checked by seeding two bugs into recovery, one at a time (skip the rollback of an uncertain `exec`; keep orphan checkpoints). The harness reported both (doubled lines in `counter.txt`; `checkpoint cp-0001 already exists`), and passed again with the real code. No runtime bug was found by the harness in M3, so `docs/bugs.md` has no new entry.
2. **Few kills landed inside the checkpoint copy.** In the first quick run only 1 of 3 runs per P5 cell had a call open at the kill; the timer (up to 8 ms) often fired after the copy (about 3 ms) and its COMPLETED record. Shortened to 4 ms. In the full run 165 of 200 (`fs.write`) and 185 of 200 (`exec`) P5 kills cut a call off; the rest landed just after it completed, which is a valid crash too, only a less interesting one.
3. **`abort()` would have written thousands of core dumps.** Noticed while designing the crash hook, before it happened. The hook sends SIGKILL instead (D-052) and the v1 document's sentence was corrected.
4. **Recovering runtime and dead runtime live in different cgroup scopes.** The exec intent first stored only the cgroup's *name* (M2). Each `systemd-run` makes a new scope, so recovery could not have found the old group by name. Changed to the full path before the first crash test (D-055).
5. **`P3 exec`: 8 of 200 runs were not killed.** The random delay (up to 120 ms) sometimes outlasted the whole remaining run. Those runs completed normally and still passed every check; the test requires that more than half of all runs are killed, and 4,788 of 4,800 were.

## 9. How to run it

```sh
nix develop
make test                     # everything, with 3 kills per cell (about 20 s)
make eval-recovery            # 200 kills per cell = 4,800 runs; prints the table, writes target/recovery-report.json
make eval-recovery RUNS=20    # a quicker look
```

By hand:

```sh
S=/tmp/agentos-crash
RUN="systemd-run --user --scope -p Delegate=yes -q cargo run -q -- --state-dir $S"
AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=6 $RUN run --repo examples/demo-repo \
    --policy policies/nix.toml --script examples/scripts/fix-demo.toml     # dies after call #5's line; exit 137
$RUN status                   # <id>  interrupted (run `agentos recover`) 6 calls
$RUN recover <id>             # #6 fs.write APPLIED: file has the new content; checkpoint cp-0002 taken
$RUN run --resume <id>        # RESUMING after 6 calls; #7 exec pytest -q ... exit 0; COMPLETED
$RUN trace <id>               # call #6 shows "RECOVERED, APPLIED"
```

Try `AGENTOS_CRASH_AT=P3 AGENTOS_CRASH_CALL=2 AGENTOS_CRASH_DELAY_US=150000` to kill the runtime while pytest runs: recovery rolls back and the resumed agent runs pytest again as a new call.

The recovery times in §2 are the wall time of the whole `agentos recover` process as measured by the harness, including process start and opening SQLite, while 8 trials run in parallel. They are not a tuned benchmark.

## 10. What this milestone does not do

- **No power-loss claim.** Checkpoint contents are not fsynced; the tests only kill the process (D-022, D-054).
- **No exactly-once claim.** An uncertain `exec` is rolled back and may be issued again by the agent as a *new* call. That is safe only because everything in v1 can be undone by restoring a folder. A call to an outside service could not be rolled back; that is the "Bonus" after v0.1.
- **A crash during setup** (before the workspace and `cp-0000` exist) is not recoverable: `recover` says to start a new run.
- **Old checkpoints are never pruned.** Cheap with reflinks, a real disk cost on ext4.
- **The LLM agent's conversation** is not yet in the log; `restore` for it comes in M4.
- **`fhs` preset not yet verified.**

## 11. Check your understanding

1. Name the three records of a successful `fs.write` in order. For a crash in each of the two gaps between them, what does recovery find and decide?
2. Why does the `fs.write` intent store *both* the old and the new hash? What could recovery not decide with only one of them?
3. Why is an uncertain `exec` always rolled back instead of inspected?
4. What is an orphan checkpoint, at which crash points can one appear, and why is deleting it correct?
5. Why must COMPLETED be written after the checkpoint and not before? Describe the bad outcome of the other order.
6. `checkpoint::restore` could be "delete `work/`, then copy". What goes wrong at crash point P8, and how does `RENAME_EXCHANGE` avoid it?
7. In `recover`, the verdict record is written after the rollback. What would break if it were written first?
8. Why does recovery kill a cgroup when the sandbox already dies with its parent?
9. How does the scripted agent know where to continue? What single source is its position rebuilt from?
10. The harness decides "was the write applied?" itself, before running recovery. Why not just read recovery's verdict?
11. What did seeding a bug into recovery prove that 4,800 passing runs could not?
12. A reviewer asks: "So your runtime is durable against power failure?" What is the honest answer, and which design decisions does it rest on?
