# v4 trial: TraceWeaver on the kept runs of earlier trials, `nix` preset

*2026-11-28, development machine (NixOS, kernel 7.1.8), norn at the M26 working tree (after `d68aed0`), debug build. Plan item X3 of `docs/v4-scope.md`. One person-free session, by hand; a record of what the views could and could not answer, not a benchmark.*

## The material

The state folders kept from earlier trials, in `~/Temp/AgentOS_temp/` (outside this repository): 32 executions written by norn 0.1 to 1.1. They were copied to a scratch folder first, so that the reads (which leave SQLite's `-wal` and `-shm` files, D-172) did not touch the originals. The v3 trial's own state folder was not kept; the frozen 1.2 executions of `tests/fixtures/compat/` stand in for runs with network and approvals.

## The questions, written before looking (2026-11-28 16:44 UTC)

1. **Time.** In the M13 relay run of ticket INV-104 (`run4/INV-104`), where did the time go: how much in the model, how much in the tools, and was there a stop?
2. **Two runs of one ticket.** The relay run (M13) and the Claude Code run (M18, `run5/INV-104`) of INV-104: where did they first go different ways, and did they end with the same change?
3. **What a long run touched.** In the M18 Claude Code run of INV-105 (`run5/INV-105`), which calls failed or were refused, and which files did the change touch, call by call?
4. **Why.** Why did the local model of the M9 LLM run (`run/state-m9-llm`) make its first write?
5. **Across runs.** Across all kept runs, which calls were ever refused, and by which rule?

## The answers

Each answer gives the commands used and what they showed. `norn` is the debug build of the M26 tree, with `NORN_STATE_DIR` set to the scratch copy.

### 1. Time in the M13 relay run of INV-104: answered, with one limit

`norn trace run4_INV-104__…`: 19 calls. The tools took 2.8 s in all; the longest was the full `pytest` (0.879 s). Before each of calls 1 to 18 the agent took between 6.6 s and 15.0 s (the relay's subagent thinking). Before call 19 the "agent" column shows **10h54m**.

The limit: the log was written before 1.3.0 (at M13), when a resume left no event (D-170), so the view cannot tell a stop from a slow model there. The trial's own record (`docs/results/relay-model-trial-nix.md`) says the run stopped when the subagents hit a usage limit and was resumed hours later. With 1.3 the same run would show a line `-- resumed by \`norn run --resume\` after 18 call(s)` and a normal time for call 19 (`tests/trace.rs`: `a_resume_is_recorded_and_shown_between_the_calls`).

### 2. The relay run and the Claude Code run of INV-104: answered

`norn diff run4_INV-104__… run5_INV-104__…`: 19 and 22 calls, 7 the same and in the same order.

- They part at **call 1**: the relay's model tried `grep -rn -i tax src config tests`, which the policy refused; Claude Code listed the folder (`fs.list .`).
- Both read the same six files (`config.py`, `pricing.py`, `models.py`, `settings.yaml`, `test_config.py`, `test_pricing.py`), partly in another order; Claude Code also read `tests/conftest.py` and searched (`fs.search "tax"`).
- The relay run edited, then ran `pytest`; Claude Code ran `pytest` first (`= #18 #11 exec pytest -q  exit 1`), then edited.
- Both ended `completed`, with a final `pytest` run that left out the same unrelated failing tests (`~ #19 #22`, other arguments).
- Their patches are **different** (106 and 76 changed lines). `norn diff` cannot say whether both are right; the trials checked both on a clone against the ticket's notes (`docs/results/relay-model-trial-nix.md`, `mcp-claude-code-trial-nix.md`).
- `task DIFFERENT`: an MCP execution's task is the session's name, not the ticket. The line is correct, but not useful for such a pair.

While answering this, the order of `norn diff`'s lines was found to be wrong: a changed call (`~`) came out before the calls that preceded it in run B. Fixed: in each stretch between two common calls, what only A has comes first, then B's calls in B's order (D-175).

### 3. What the Claude Code run of INV-105 touched: answered

`norn trace run5_INV-105__… --denied --failed`: 4 of 31 calls.

| Call | What | Result |
|---|---|---|
| #9 | `fs.search "LEG" in tests/fixtures/stock_movements_2025.csv` | FAILED: a file, not a folder |
| #12 | `exec python3 -I -c …` (a CSV analysis) | DENIED, no matching rule |
| #29 | `pytest -q` | exit 1 |
| #31 | `fs.edit README.md` | DENIED, no matching rule |

`--security` gives the same two denials, and no undone changes or network. `--tool fs.write`, `--tool fs.delete` and `--diff N` show what the model did after #12 was refused: it wrote its analysis as a test file, `tests/test_tmp_analysis.py` (#13, rewritten at #23), ran it, and deleted it (#25). `--diff 13` shows that file's 27 lines, and `--diff 21` the 39-line comment and code added to `src/inventory/parsing.py`. `--diff` alone names the three files of the final change: `src/inventory/parsing.py`, `tests/test_legacy_columbus.py`, `tests/test_parsing.py`. The scratch file is not among them, and the refused README edit never happened.

### 4. Why the local model made its first write: not by an id

`norn trace run__20261115-071323300-… --graph`: every call says **cause not recorded**, because the log was written before 1.3.0 (at M9). `--call 3` shows the write (973 bytes over a 7.6 kB file) and says "cause not recorded". The reason exists: `--json` shows `agent.message` 15, the assistant message just before the call. Its text says the model meant to make `parse_order` keep the order's currency, and its tool call is `fs_write`. But only the order of events links the two, and the views do not guess from order (spec §9.13, D-170). The answer needs a person reading the events. For a log written by 1.3 the link is an id (`tests/llm_agent.rs`: `each_call_names_the_assistant_message_that_asked_for_it`).

### 5. Refused calls across all runs: answered, after one change

`norn search --outcome denied`: **24 calls in 32 executions**. By tool: 19 `exec`, 3 `fs.write`, 1 `fs.edit`, 1 `fs.read`. By rule: 23 `no matching rule`, 1 `filesystem.deny[2] ".env.*"` (a scripted run's deliberate read of `.env.example`). The commands refused were `grep`, `git`, `python3` scripts and `-c`, `python`, `npm`, `node` and `rm`: the trials' policies allowed test runners only.

At first the search printed `DENIED` without the rule, and the count by rule needed `--json` and a script. `norn search` now prints the rule of a refused call and the error of a failed one (D-175).

## What this showed

- Four of the five questions were answered with the new views alone; the fifth (why) is answered by an id only for logs written from 1.3 on.
- Two of the views were changed by the trial: the order of `norn diff`'s lines, and the rule in `norn search`.
- `norn search` read all 32 old logs without an error; the other views were used on four of them, also without one. Where a view would need a newer field, it says so instead of guessing: "cause not recorded", "tokens not recorded", no resume line.
