# M16 — A killed MCP session, and attacks through MCP

| | |
|---|---|
| Weeks in the plan | v2 (D-139); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` waits for a CI run |
| Commits | the M16 commit (tag `m16` after CI is green, D-113) |
| Status | Done. Verified on both presets and Debian 12 by CI run 45728656310 on `1571d09`; tagged there (D-154) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

M15 gave norn a second front door. M16 checks that it is as safe as the first: that a `norn mcp` killed at any of the eight crash points, and started again by its client, loses nothing and doubles nothing; that every attack of the security suite fails the same way when it arrives as MCP messages; and that hostile protocol messages cannot crash the server or reach the gateway. It touches no code in `src/`: the point is to test the event log, checkpoints and recovery through the new door.

## 2. Requirements

From `docs/v2-scope.md`, M16. B1 was redesigned by the M15 experiments (D-141) and built in M15.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| B1 a killed session is continued (D-141: by the client's restart) | Yes, `nix` | M15's `a_server_the_client_restarts…`; every trial below |
| B2 kill cells through MCP, same invariants | Yes, `nix` | `recovery_holds_through_mcp`: at 3 per cell, 72 runs, 72 killed, 0 violations, 0 duplicated effects, 59/59 classified; at 200 per cell: 4,800 runs, 0 violations, 0 duplicated effects, 3,897/3,897 classified (`docs/results/recovery-mcp-200-nix.md`) |
| B3 the security cases through MCP | Yes, `nix`: 31 of 33 | `NORN_SECURITY_FRONT=mcp cargo test --test security`: 33 passed; 31 of them send their attack through `norn mcp` (see §10) |
| B3 protocol cases | Yes, `nix` | `hostile_messages_never_crash…` and M15's `malformed_and_unknown_requests…` |
| Both presets | `fhs` not yet | `make test` runs both passes; needs a push |

## 3. What you need to know first

**The kill harness** (M3, `tests/recovery/main.rs`). A test sets `NORN_CRASH_AT=P4` (and which call), and norn kills itself with SIGKILL when it reaches that point. The eight points sit around one call's life: before the intent, after it, in the middle of the effect, before and after the checkpoint, before the completion record, after it, and inside recovery (P8). After the kill the harness checks five invariants (no effect without an intent; every completed change names a checkpoint that holds it; the workspace equals the last checkpoint and what the log says; no call starts while an older one is unsettled; the log has no gaps), then lets the run go on and checks the end state is exactly what an uninterrupted run gives.

**Test oracle.** The part of a test that decides whether a result is right. Here: the five invariants and the end-state check. M16 changes who drives the calls, not the oracle.

## 4. Experiments

None new. The client's behaviour after a crash (start the server again with the same command line) was measured in M15 (E3).

## 5. What was built

| File | Purpose |
|---|---|
| `tests/recovery/main.rs` | `mcp_steps`, `steps_done`, `run_mcp_trial`, `recovery_holds_through_mcp` |
| `tests/security/main.rs` | `run_steps_with_env`, `run_through_mcp`; the environment test goes through it |
| `tests/mcp.rs` | `hostile_messages_never_crash_the_server_or_reach_the_gateway` |
| `tests/common/mcp_client.rs` | `start_with` (extra environment), `try_call` (no answer when the server died), `wait`, `send_bytes` |
| `Makefile` | `make test` and `make eval-security` run the security suite a second time with `NORN_SECURITY_FRONT=mcp` |

## 6. The code, explained

**`run_mcp_trial`.** Mirrors `run_trial` step by step.

1. A `norn mcp` is started with the crash variables in its environment, and the client sends the seven workload calls with `try_call` until one gets no answer: the server killed itself.
2. Before anything touches the folder, the same two checks as `run_trial`: if no intent is open, the workspace must equal the last checkpoint (I1); and the truth about the cut-off call is read from the files themselves.
3. For P8, the client starts the server with `NORN_CRASH_AT=P8`; its first call makes the server continue the execution, and recovery inside it is killed.
4. The client starts the server again, cleanly, and sends a *probe*: `fs_list` of an empty folder. That first call is when the server reopens the execution and runs recovery (D-141), and the probe changes nothing, so the invariants can be checked right after recovery, and the verdict on the cut-off call compared with the files.
5. `steps_done` counts the calls the log shows as done (completed, or reconciled as applied), and the client sends the rest. This is what `norn run --resume` does for a script. A client that blindly sent the cut-off `exec` again would double its effect by its own choice; that would measure the client, not norn.
6. The client stops the server with SIGINT, the person runs `norn finish`, and the invariants and the end state are checked: `src/a.txt` and `src/new.txt` as an uninterrupted session leaves them, and exactly two lines in each of the files the two `exec` calls append to.

**`run_through_mcp`** (security). Reads the attack's script, turns each step into a `tools/call` (the tool name with `.` replaced by `_`, the other fields as arguments), stops the server and runs `norn finish`. The test then reads the log and the blobs exactly as for a scripted run, so every assertion applies unchanged.

**The environment test.** It starts the runtime with `NORN_TEST_SECRET` in its environment and has the attack look for it in its own environment and in every `/proc/*/environ` it can see. Through MCP the runtime's environment is the client's, and Claude Code's can hold an API key; the sandbox starts commands with an environment built from the policy, so the secret is not there either way.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-144 | Same cells and oracle with an MCP driver; continue from the first step not done; a probe call after the restart; the security suite through one switch, run by `make test` | A second harness; always repeating the cut-off call; a copy of the security suite; requiring every P8 trial to kill recovery |

## 8. Problems hit

1. An assertion added in this milestone (every P8 trial must kill recovery inside the restarted server) failed: 2 of 3 in one cell. The trial kills recovery at its n-th pass of the point, n from 1 to 5, and a recovery with little to do never gets there. The existing harness counts these and never asserted them; the MCP test now does the same. No code was wrong.
2. The MCP security pass ran in the same 4.2 s as the scripted one. It looked as if the switch had not taken effect; one test's folder showed `agent mcp` and the server's stop message, so it had.
3. An edit made with a Python script failed on Rust's `'''` inside a Python string and changed nothing; the edit was made with the editor instead. The test results printed right after it were from the unchanged files and were not used.

## 9. How to run it

```sh
nix develop -c make test                                        # both security passes, 3 kills per cell
nix develop -c env NORN_RECOVERY_RUNS=200 systemd-run --user --scope -p Delegate=yes \
  cargo test --test recovery recovery_holds_through_mcp -- --nocapture   # the MCP cells at full size
nix develop -c env NORN_SECURITY_FRONT=mcp systemd-run --user --scope -p Delegate=yes cargo test --test security
```

`make eval-recovery` runs every test in `tests/recovery/`, the MCP cells included.

## 10. What this milestone does not do

- Two security tests are not sent through MCP: the TOCTOU symlink swap and the FIFO read. Both exercise the file tools' own code, which is the same for both doors, and each needs its own process handling. The MCP pass reports them as passed because they run their scripted form.
- The client in these tests is a fixed program, not a model. What a real model does after `Connection closed` is up to it; M18's trial is where that is seen.
- Cancellation (`notifications/cancelled`) is not tested beyond being ignored: a call is handled to its end.

## 11. Check your understanding

1. What does the probe call after the restart make possible, and why must it change nothing?
2. Why does the test client continue from the first step the log does not show as done, instead of repeating the call that was cut off?
3. Which invariant would catch a server that started a new call before settling the cut-off one?
4. Why does the security suite get a switch in one function instead of a second copy?
5. Why is the environment test the one that matters most for MCP?
6. A P8 trial does not always kill recovery. Why not, and why is that not a gap in the test?
7. Why can a deeply nested JSON message not crash the server?
8. What would "duplicated effects" measure if the client always repeated the cut-off `exec`?
