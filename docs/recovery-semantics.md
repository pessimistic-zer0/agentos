# Recovery semantics

What AgentOS v0.1 promises about a run that is killed, what it does not promise, and how each promise is checked. Plain words first; terms are in the glossary of `decision-log.md`.

## 1. The promise, in one paragraph

If the `agentos` process is killed at any moment (`kill -9`, a crash, the terminal closing), then after `agentos recover <id>`:

- the event log is intact and says, for every tool call, exactly one of: **denied**, **completed**, **failed**, or **reconciled** as APPLIED or NOT APPLIED;
- the workspace is exactly the state produced by the calls the log calls completed or APPLIED, and nothing else;
- no call's effect was applied twice, and no call that was cut off is run again by the runtime on its own.

"Killed" means the *process* dies. See §7 for what is not covered.

## 2. The life of a call

```text
 tool.requested ── policy ──► tool.denied                      (terminal; nothing was attempted)
                     │
                     ▼
                tool.intent    written and fsynced BEFORE the effect starts
                     │
                  effect       fs.read / fs.list: none
                     │         fs.write: temp file + fsync + rename
                     │         exec: command in the sandbox, then revert of changes outside the write globs
                     ▼
                checkpoint     mutating calls only: full copy of the workspace, temp name then rename
                     │
                     ▼
                tool.completed (names the checkpoint)   or   tool.failed (nothing was changed)
```

Three rules make recovery possible:

1. **Intent before effect.** No change to the workspace happens before its intent is on disk. So a change always has a trace.
2. **Effect, then checkpoint, then COMPLETED.** A COMPLETED record always names a checkpoint that already exists. "The last checkpoint" means the last one *named in the log*; a checkpoint folder the log does not name does not count.
3. **One call at a time.** At most one call can be in flight when the process dies.

Every record is one SQLite transaction in WAL mode with `synchronous=FULL`; sequence numbers are 1, 2, 3, … with no gaps.

## 3. What a crash leaves, and what recovery decides

The crash points are named P1–P8 (the fault-injection harness kills the runtime at each).

| Killed… | The log shows | The workspace | Recovery's verdict |
|---|---|---|---|
| P1: before the intent is written | `tool.requested` only | unchanged | NOT APPLIED ("no intent was recorded, so the effect never started") |
| P2: after the intent, before the effect | intent, no ending | unchanged | by tool, below |
| P3: during the effect | intent, no ending | unknown | by tool, below |
| P4: after the effect, before the checkpoint | intent, no ending | changed | by tool, below |
| P5: during the checkpoint copy | intent, no ending | changed; a `.tmp-cp-…` folder may exist | by tool, below; the temp folder is deleted |
| P6: after the checkpoint, before COMPLETED | intent, no ending | changed; a checkpoint exists that the log does not name | by tool, below; the unnamed checkpoint is deleted first |
| P7: after COMPLETED, before the agent saw the result | completed | consistent | nothing to do; on resume the agent is given the stored result |
| P8: during recovery itself | as before, plus `recovery.started` | whatever the interrupted step left | run recovery again; same result |

For a call with an intent and no ending ("uncertain"), by tool:

| Tool | How it is settled | Why |
|---|---|---|
| `fs.read`, `fs.list` | NOT APPLIED | No effect to undo. The agent can ask again |
| `fs.write` | The intent stored the file's hash before (`pre_hash`) and the hash of the new content (`new_hash`). Recovery deletes the leftover temp file and hashes the file now. **Equals `new_hash` → APPLIED**, and the missing checkpoint is taken now. **Equals `pre_hash` → NOT APPLIED.** Anything else → roll back to the last checkpoint, NOT APPLIED | The write is one atomic rename, so the file is observably on one side or the other |
| `exec` | Kill the command's cgroup if it still exists, roll the workspace back to the last checkpoint, NOT APPLIED | What a command changed cannot be known; everything it can change is inside the workspace, which a folder copy restores exactly |

The verdict is written as `tool.reconciled {applied, reason, checkpoint | restored}`. `agentos trace` shows it as `RECOVERED, APPLIED` or `RECOVERED, NOT APPLIED`.

## 4. Why recovery can be interrupted

Recovery is built from steps that give the same result when repeated:

- deleting temp folders and unnamed checkpoints (deleting twice is deleting once);
- **rollback**: copy the checkpoint next to the workspace, then swap the two folders with one `renameat2(RENAME_EXCHANGE)`. Before the swap the workspace is untouched; after it, fully restored; there is no state in between;
- the verdict record is written only *after* its work is done. If recovery dies first, the call is still uncertain next time and is settled again, the same way.

## 5. Resuming

`agentos run --resume <id>` always recovers first, then continues the agent:

- The **scripted agent** replays the recorded results: each one either moved it to the next step or (NOT APPLIED) left it on the same step, which it then asks for again as a *new* call.
- The **LLM agent** rebuilds its conversation from the `agent.message` events and receives, for the cut-off call, recovery's verdict as the tool result ("NOT APPLIED: the runtime was restarted … call the tool again"). A request to the model that was cut off is sent again; the answer may differ.

So after an uncertain `exec`, the same command may run again, as a new call with a new intent, on a workspace that was rolled back first. That is *at-least-once with rollback*, not exactly-once.

## 6. The invariants, and how each is checked

`tests/recovery/main.rs` runs a seven-step workload (all four tools), kills the runtime at a chosen point, and checks the result against an oracle that does not ask the runtime: it reads the files on disk to decide whether a cut-off write happened, and adds up the applied calls in the log to predict the workspace. Each `exec` step appends one line to two files, so a doubled effect is an extra line.

| # | Invariant | Check |
|---|---|---|
| I1 | No effect without a durable intent before it | Before recovery: if no call has an open intent, the workspace equals the last named checkpoint |
| I2 | No COMPLETED record whose effect is missing | Every completed or APPLIED mutating call names a checkpoint that exists and contains the write |
| I3 | After recovery the workspace is the last checkpoint plus exactly the APPLIED calls | Workspace = last named checkpoint = the model computed from the log; no unnamed checkpoints or temp folders remain |
| I4 | No uncertain call is re-executed before it is reconciled | No call is requested while another has no ending; none is left without one |
| I5 | The log is a gap-free prefix | Sequence numbers are 1..n; `agentos trace` refuses any illegal order of records |
| — | Verdicts are right | Recovery's APPLIED / NOT APPLIED equals what the files said before recovery ran |
| — | Nothing lost, nothing doubled | After resuming to the end, the files are exactly those of an uninterrupted run |

Result of record (`nix` preset, 2026-11-12, `docs/results/recovery-200-nix.md`):

| | |
|---|---|
| Runs (24 cells × 200) | 4,800 |
| Runs in which the runtime was actually killed | 4,797 |
| Invariant violations | **0** |
| Duplicated side effects | **0** |
| Cut-off calls whose verdict matched the files | 3,918 of 3,918 |
| Runs in which recovery itself was killed (P8 cells, 400 runs) | 311 |
| `agentos recover` wall time (whole process, debug build, 8 trials in parallel) | p50 20.4 ms, p99 55.1 ms |

The harness was also shown able to fail: with the rollback of an uncertain `exec` removed, it reported doubled effects; with the deletion of unnamed checkpoints removed, it reported leftovers and failed recoveries (M3 document, §2).

## 7. What is not promised

- **Power loss or a kernel crash.** The tests kill a process. Data the kernel has accepted survives a process kill whether or not it was fsynced, so these tests cannot show that the fsync calls are in the right places, and checkpoint contents are not fsynced at all. Nothing here says the state survives a power cut.
- **Exactly-once execution.** See §5. An uncertain `exec` is rolled back and may be issued again.
- **Effects outside the workspace.** v1 has none (no network, no external tools). The moment a tool can send an email or open a pull request, rollback no longer undoes it. That case is deliberately left for after v0.1.
- **A crash during setup.** If the runtime dies before the workspace and its first checkpoint exist, `recover` says so and a new run must be started.
- **Changes made to the execution folder by anyone else.** Recovery assumes only AgentOS writes there. A lock file (`flock`) keeps two `agentos` processes off the same execution; it does not stop other programs.
- **Disk full.** If a checkpoint or a log write fails for lack of space, the runtime stops with an error; the call in flight is then uncertain and is settled by the rules above, if recovery itself has room to work.
- **Other machines.** Measured on one laptop (NixOS 26.05, kernel 7.1.8, btrfs). The `fhs` preset on Ubuntu has not been run.

## 8. Trying it

```sh
RUN="systemd-run --user --scope -p Delegate=yes -q agentos --state-dir /tmp/agentos-crash"
AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=6 $RUN run --repo examples/demo-repo \
    --policy policies/nix.toml --script examples/scripts/fix-demo.toml   # killed: exit 137
$RUN status                 # interrupted (run `agentos recover`)
$RUN recover <id>           # #6 fs.write APPLIED: file has the new content; checkpoint cp-0002 taken
$RUN run --resume <id>      # continues with call #7
```

`AGENTOS_CRASH_AT` takes `P1`…`P8`; `AGENTOS_CRASH_CALL` picks the call; `AGENTOS_CRASH_DELAY_US` sets the random delay for the mid-effect points P3 and P5 (`src/faults.rs`). `make demo` runs this sequence; `make eval-recovery` runs the whole harness.
