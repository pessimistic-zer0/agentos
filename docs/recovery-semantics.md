# Recovery semantics

What Norn v0.1 promises about a run that is killed, what it does not promise, and how each promise is checked. Plain words first; terms are in the glossary of `decision-log.md`.

## 1. The promise, in one paragraph

If the `norn` process is killed at any moment (`kill -9`, a crash, the terminal closing), then after `norn recover <id>`:

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
| `fs.write` | The intent stored the file's hash before (`pre_hash`) and the hash of the new content (`new_hash`). Recovery deletes the leftover temp file and hashes the file now. **Equals `new_hash` → APPLIED**, and the missing checkpoint is taken now. **Equals `pre_hash` → NOT APPLIED**; but if the intent lists folders the write was going to make (`created_dirs`), some of them may exist, so the workspace is rolled back to the last checkpoint first (D-084). Anything else → roll back to the last checkpoint, NOT APPLIED | The write is one atomic rename, so the file is observably on one side or the other |
| `fs.write` with `executable` | The intent also names the permission bits the write sets (`mode`). They are set on the temporary file before the rename, so content and mode change in one step. If the new content equals the old (a write that only changes the mode), the hashes cannot tell: then APPLIED needs the file to have the planned mode, otherwise NOT APPLIED (D-123) | `tests/binary.rs`, `a_kill_around_a_write_that_only_changes_the_mode_is_classified_by_the_mode` |
| `fs.edit` | The same as `fs.write` (D-120). Before the intent is written, the runtime reads the file, replaces the one occurrence in memory and so knows the whole new content; the intent holds the same `pre_hash`, `new_hash` and temp name, and the file is replaced the same way. A resumed agent that asks for an edit again after it was APPLIED gets "old_str was not found", so an edit cannot be made twice | The effect is one atomic rename of a file whose content was fixed before the intent; `tests/recovery/main.rs`, `an_edit_is_recovered_like_a_write` |
| `exec` | Kill the command's cgroup if it still exists, roll the workspace back to the last checkpoint, NOT APPLIED | What a command changed cannot be known; everything it can change is inside the workspace, which a folder copy restores exactly |

The verdict is written as `tool.reconciled {applied, reason, checkpoint | restored}`. `norn trace` shows it as `RECOVERED, APPLIED` or `RECOVERED, NOT APPLIED`.

## 4. Why recovery can be interrupted

Recovery is built from steps that give the same result when repeated:

- deleting temp folders and unnamed checkpoints (deleting twice is deleting once);
- **rollback**: copy the checkpoint next to the workspace, then swap the two folders with one `renameat2(RENAME_EXCHANGE)`. Before the swap the workspace is untouched; after it, fully restored; there is no state in between;
- the verdict record is written only *after* its work is done. If recovery dies first, the call is still uncertain next time and is settled again, the same way.

## 5. Resuming

`norn run --resume <id>` always recovers first, then continues the agent:

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
| I5 | The log is a gap-free prefix | Sequence numbers are 1..n; `norn trace` refuses any illegal order of records |
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
| `norn recover` wall time (whole process, debug build, 8 trials in parallel) | p50 20.4 ms, p99 55.1 ms |

Result of record (`fhs` preset, Ubuntu 24.04 in CI, 2026-11-13, <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105>, `docs/results/recovery-200-fhs.md`):

| | |
|---|---|
| Runs (24 cells × 200) | 4,800 |
| Runs in which the runtime was actually killed | 4,800 |
| Invariant violations | **0** |
| Duplicated side effects | **0** |
| Cut-off calls whose verdict matched the files | 3,756 of 3,756 |
| Runs in which recovery itself was killed (P8 cells, 400 runs) | 306 |
| `norn recover` wall time (shared CI runner, debug build, 8 trials in parallel) | p50 33.5 ms, p99 315.3 ms |

That green run was the third. The first two CI runs failed this suite, 1 and then 3 violations in 4,800 runs, and each found a real bug that the kills on the laptop had not: a checkpoint copy that outlived the killed runtime (B-007), and a lock that outlived it for a few milliseconds (B-008, brought in by the first fix). Both are in `docs/bugs.md`. Two things changed in the semantics as a result: the `cp` child that copies a folder is killed by the kernel when the runtime dies, and taking the execution's lock waits up to three seconds before reporting "in use".

The harness was also shown able to fail: with the rollback of an uncertain `exec` removed, it reported doubled effects; with the deletion of unnamed checkpoints removed, it reported leftovers and failed recoveries (M3 document, §2).

## 7. What is not promised

- **Power loss or a kernel crash.** The tests kill a process. Data the kernel has accepted survives a process kill whether or not it was fsynced, so these tests cannot show that the fsync calls are in the right places, and checkpoint contents are not fsynced at all. Nothing here says the state survives a power cut.
- **Exactly-once execution.** See §5. An uncertain `exec` is rolled back and may be issued again.
- **Effects outside the workspace.** v0.1 has none (no network, no external tools). The moment a tool can send an email or open a pull request, rollback no longer undoes it. That case was left for after v0.1 and is now covered, for one fake service, by the bonus tool: see §9.
- **A crash during setup.** If the runtime dies before the workspace and its first checkpoint exist, `recover` says so and a new run must be started.
- **Changes made to the execution folder by anyone else.** Recovery assumes only Norn writes there. A lock file (`flock`) keeps two `norn` processes off the same execution; it does not stop other programs.
- **Disk full.** If a checkpoint or a log write fails for lack of space, the runtime stops with an error; the call in flight is then uncertain and is settled by the rules above, if recovery itself has room to work.
- **Other machines.** Run on one laptop (NixOS 26.05, kernel 7.1.8, btrfs) and on one kind of CI runner (Ubuntu 24.04, `fhs` preset). Nothing else has been run.

## 8. Trying it

```sh
RUN="systemd-run --user --scope -p Delegate=yes -q norn --state-dir /tmp/norn-crash"
NORN_CRASH_AT=P4 NORN_CRASH_CALL=6 $RUN run --repo examples/demo-repo \
    --policy policies/nix.toml --script examples/scripts/fix-demo.toml   # killed: exit 137
$RUN status                 # interrupted (run `norn recover`)
$RUN recover <id>           # #6 fs.write APPLIED: file has the new content; checkpoint cp-0002 taken
$RUN run --resume <id>      # continues with call #7
```

`NORN_CRASH_AT` takes `P1`…`P8`; `NORN_CRASH_CALL` picks the call; `NORN_CRASH_DELAY_US` sets the random delay for the mid-effect points P3 and P5 (`src/faults.rs`). `make demo` runs this sequence; `make eval-recovery` runs the whole harness.

## 9. The bonus: a call whose effect cannot be rolled back (added after v0.1)

Everything above relies on one fact: all effects are inside the workspace, and a folder can be put back from a copy. The bonus tool `ext.call` asks a service outside the runtime to do something (D-073). A copy of the workspace does not undo that. This section gives its rules. Nothing in sections 1 to 8 changes for the other four tools.

### 9.1 The problem in one picture

```text
   runtime                         service
      │ ── request ──────────────►   │        case A: the request is lost.   Nothing was done.
      │                              │
      │ ── request ──────────────►   │ does it
      │    ◄────────────── answer ── │        case B: the answer is lost.    It WAS done.
      ▼
   "no answer"   (the same in both cases; so is a kill of the runtime in mid-request)
```

From the runtime's side A and B look identical. Treating "no answer" as "failed" doubles the effect in case B when the agent tries again. Treating it as "done" loses the effect in case A.

### 9.2 The rules

1. **The key goes into the intent first.** The intent of an `ext.call` holds the service's URL, the hash of the payload, the timeout, the number of attempts, and an *idempotency key*, `agentos-<execution id>-c<call id>`. The key is computed, not random, so everyone who ever sends this call sends the same key (D-077). The payload is kept in the blob store.
2. **An answer is one of three things** (D-076): HTTP 2xx, *applied*; HTTP 4xx, *refused, nothing done*; anything else (timeout, dropped connection, 5xx, nobody listening, a redirect), *unknown*.
3. **Unknown is retried with the same key**, up to `external.attempts` times. The service promises to apply a key at most once and to repeat its first answer, so a retry cannot double anything.
4. **Still unknown: stop.** The run ends with an error and the intent stays open. No COMPLETED and no FAILED is written, because neither is known to be true (D-079). The log now looks exactly as it would after a kill at that moment.
5. **Recovery settles it forwards** (D-078, ADR 0012). It sends the same request again with the key from the intent. 2xx: `tool.reconciled`, APPLIED. 4xx: NOT APPLIED. No answer: recovery stops with an error, writes no verdict, and the execution stays interrupted. `norn recover` is then simply run again later. `run --resume` recovers first, so the agent never moves on past an unsettled call.

An `ext.call` takes no checkpoint, and a workspace rollback (for example after a later uncertain `exec`) does not touch what the service did.

### 9.3 What a crash leaves, for `ext.call`

| Killed… | The log shows | At the service | Recovery |
|---|---|---|---|
| P1: before the intent | `tool.requested` only | nothing: no request can have left | NOT APPLIED. If the agent asks again it is a new call with a new key, which is safe because nothing was sent under the old one |
| P2: after the intent, before the request | intent, no ending | nothing | sends the request; APPLIED |
| P3: while the request is in flight | intent, no ending | applied, not applied, or about to be | sends it again with the same key; APPLIED, exactly once, whichever request the service acts on |
| P7: after COMPLETED | completed | applied | nothing to do |
| P8: during recovery | as before, plus `recovery.started` | whatever the interrupted recovery's request did | run again; the same key again |
| not killed; no answer to any attempt | intent, no ending | applied or not | as P3 |

P4 to P6 sit around the checkpoint and do not exist for this tool.

### 9.4 What is checked, and the result

`tests/recovery/main.rs::external_calls_hold_under_kills_and_timeouts` runs a workload with two `ext.call` steps among file and command steps, against a fake service inside the test program (D-075, D-080). The service survives every kill, and its own list of applied requests is the oracle.

| Check | How |
|---|---|
| No request without a durable intent (I1) | Every request the service ever received carries a key named by an intent in the log |
| Verdicts are right | Recovery's APPLIED / NOT APPLIED equals the service's record for that key |
| The log tells the truth | Every ending of an `ext.call` in the log (completed, failed, reconciled) agrees with the service |
| Nothing lost, nothing doubled | At the end each order is in the service's record exactly once, counted by payload |
| A stop decides nothing | A run or recovery that stopped "uncertain" wrote no verdict and made no new call |
| The workspace invariants I2 to I5 | As in §6 |

Result of record (`nix` preset, 2026-11-13, `docs/results/recovery-ext-200-nix.md`):

| | |
|---|---|
| Runs (8 cells × 200) | 1,600 |
| Runs in which the runtime was killed (the five P cells, 1,000 runs) | 987 |
| Runs in which recovery itself was killed (P8, 200 runs) | 192 |
| Stops with the call left uncertain (a run or a recovery got no answer) | 288 |
| Invariant violations | **0** |
| Orders applied twice, or not at all | **0** |
| Cut-off calls whose verdict matched the service's record | 932 of 932 |

On the `fhs` preset (Ubuntu 24.04 in CI, 2026-11-13, <https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321>, commit `1bc27da`, `docs/results/recovery-ext-200-fhs.md`): 1,600 runs, 994 killed in the P cells, 192 with recovery killed too, 303 stops with the call left uncertain, **0** invariant violations, **0** orders applied twice or lost, 942 of 942 verdicts matching the service's record. That is one CI run.

The harness was shown able to fail here too: with recovery answering "not applied" without asking the service, 48 doubled effects in 160 runs; with a fresh key for every attempt, 70 (`docs/results/recovery-ext-mutations.md`).

### 9.5 What is not promised

- **Exactly-once only with a service that honours the key.** With one that ignores it, every retry is a new effect, and Norn cannot tell.
- **The service's own durability.** The fake keeps its state in memory and is never killed in the tests. Whether a real service remembers keys across its own crashes is its property.
- **Progress while the service is down.** The call stays uncertain and the run stays stopped for as long as nobody answers. That is deliberate.
- **The answer of a reconciled call.** The agent is told the call was applied; the service's answer is kept only as the first 200 characters in the verdict's reason.
- **Order between the workspace and the service.** If a later `exec` is rolled back, an earlier `ext.call` stays done. The two are not one transaction.

## 10. A killed MCP session (added for v2)

With `norn mcp` the agent is a client such as Claude Code (D-139). Nothing in sections 1 to 6 changes: every `tools/call` goes through the same gateway, with the same intent, checkpoint and completion record. What changes is who restarts, and who finishes:

- **A stop is only a stop.** A client ends a session with SIGINT and kills the server half a second later (`experiments/m15-mcp/NOTES.md`, E3b). norn handles it like any stop (D-106): it exits at once, and the execution is left unfinished, exactly as after `kill -9`.
- **The client restarts the server; the server continues the execution.** A client starts a dead server again with the same command line (E3). `norn mcp` then continues the newest unfinished MCP execution of that repository, and runs recovery before its first answer (design rule 5). A call the client sends again is a new call with a new id (D-141).
- **The person finishes it.** `norn finish <id>` settles a cut-off call and writes the patch; `norn apply` is as before.

Checked: the kill cells of section 6 driven by an MCP client that restarts the server after each kill, 200 per cell: 4,800 runs, 4,782 killed, 0 invariant violations, 0 duplicated effects, 3,897 of 3,897 cut-off calls classified correctly (`docs/results/recovery-mcp-200-nix.md`, D-144). `fs.delete` is recovered like a write: absent means applied (1,600 runs, 0 violations, `docs/results/recovery-delete-200-nix.md`, D-147).

