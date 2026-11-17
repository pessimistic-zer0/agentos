# The six inventory tickets with Claude as the model, through a relay (`nix` preset)

Date: 2026-11-16 and 17. Code: commit `8ed3792`, debug build, NixOS 26.05, kernel 7.1.8. One run per ticket, by hand; a record, not a benchmark.

## How it was run, and what kind of evidence this is

The planned trial (D-114, C7) needs a hosted model that can hold a whole conversation; the Groq key could not (`hosted-model-trial-nix.md`). Prince asked to use Claude Code instead (D-130). So the model was Claude, answering through a relay:

- `agentos run --agent llm` talked to a small local server (`relay.py`) as it would to any OpenAI-compatible API: the whole conversation, the tool definitions, one request per turn.
- The relay wrote each request to a file and waited. A Claude Code subagent read it, chose one tool call, and wrote the answer back. Each ticket had its own subagent, started cold, with no knowledge of the project.
- The subagents were told to learn about the project only through agentos's tool results, and not to read the project, the solution notes or the AgentOS repository with their own tools. Each confirmed it followed that, and the log of every run shows only calls through the gateway. (Minor deviations they reported themselves: writing an arguments file with `echo` instead of the Write tool.)

What this is not: a hosted API; an independent model (the subagents are the same model family that built AgentOS); a measure of speed (a human-scale relay sits in every turn). What it is: the real agent loop, policy, sandbox, log and recovery, with a capable model on the other side, on six tickets the AgentOS author did not write.

The relay and the scripts are in `~/Temp/AgentOS_temp/run4/` (outside this repository), with each run's state folder.

## The six tickets

Policy: `run3/policy.toml` (the earlier trials' policy, plus `["python3", "scripts/regen_snapshot.py"]` for INV-106 and `max_tool_calls = 60`). Task: the ticket's text plus how to run the tests. Before any change the project's suite gives 3 failed, 75 passed (INV-101 and INV-102).

Every result was copied to a fresh clone with `agentos apply --to` and checked there against `SOLUTIONS.md`.

| Ticket | Calls (denied) | End | Checked on a clone | Against the solution notes |
|---|---|---|---|---|
| INV-101 one-line bug | 4 (0) | COMPLETED | 2 failed, 76 passed | **Right.** `>` became `>=`, nothing else; the 2 failures are INV-102's, as the notes say |
| INV-102 bug across two modules | 13 (1) | COMPLETED | 1 failed, 77 passed; `WEB-104240 (EUR)` | **Right.** Currency parsed, upper-cased, `None` when absent, and the settings' default used for it; the failure is INV-101's |
| INV-103 new package and command | 18 (0) | COMPLETED, after one interruption | 3 failed, 88 passed | **Right.** `alerts` command: US-EAST 8 alerts, first ELC-3004 shortfall 403; US-WEST 14, first PLB-4010 shortfall 331; BER 15, first FST-1009 available 238, point 500, shortfall 262: all as in the notes. 13 new tests |
| INV-104 tax per warehouse | 19 (1) | COMPLETED, after one interruption | 3 failed, 88 passed | **Right.** Rates in `config/settings.yaml` (`BER 0.19`, `US-WEST 0.0725`); quote taxes 40.77, 183.75, 278.08: the notes' figures. Settings without the new key still load |
| INV-105 the large fixture | 19 (3) | COMPLETED, after one interruption | 3 failed, 86 passed | **Right, with a leftover.** `import` gives 0 skipped; every one of the 2,613 rows equals `movements_truth.json`; LEG totals 11,052 / 2,499 / 59 / -5, none after September. Left an empty `tests/test_probe_tmp.py` it could not delete |
| INV-106 binary fixture and script | 17 (1) | COMPLETED, after one interruption | 3 failed, 77 passed | **Right.** Fixture is version 2, same timestamp, items and quantities, reorder points 500/15/40/25; `scripts/regen_snapshot.py` executable and writes the same bytes twice. 2 new tests |

In every row the remaining failures are the other tickets' (INV-101 and INV-102 open on that clone). No existing test lost a line in any result. The original project was unchanged after all six.

**Six of six solved**, all six by the notes' own checks. For comparison, the local 4B model solved two of the two it was given (INV-101, and INV-102 with `fs.edit`), and the Groq run one of one before the key's limit.

## What it showed about AgentOS

**Worked as designed, under a real model:**
- `fs.edit` (D-120) was the main editing tool: 26 edits over the six tickets, none failed to match. `fs.write` was used for new files.
- `fs.read` with a line range read the 2,614-line CSV of INV-105 in pieces (D-102).
- Binary files and the executable bit: INV-106 regenerated a gzip fixture and made a script executable; both arrived through `agentos apply` (D-123, D-108). The patch alone could not carry them and said so.
- `run --resume`: four runs were interrupted when the subagents hit Claude's usage limit and the runtime gave up waiting after ten minutes. Each was resumed from its log hours later, the conversation rebuilt, and finished. The subagents saw the same request again and carried on.
- Denials were answers: six commands refused (`grep` three times, `python3 -c`, `rm`), each run went on.

**Friction the six subagents reported, independently** (the reason the trial was worth running):

1. **The stop rule against a project that is already red.** The system prompt says not to stop before the tests pass, and D-107 refuses a final answer while the last command failed. These tickets start in a project with other tickets' failing tests. Four of six had their first final answer refused; they got past it by running a narrower test command (a file, or `--deselect` of the other tickets' tests). So the check is easy to satisfy with any passing command, and it pushes an agent towards fixing what it was not asked to. Five of six said so.
2. **A denial does not say what is allowed.** "DENIED by policy: no matching rule" gives the agent nothing to go on. There is no search tool and `grep` is not allowed, so the agents read whole files instead. Five of six said so.
3. **An allowed script is any Python.** INV-106 wrote a throwaway version of the one allowed script to print a binary fixture, then replaced it; INV-105 wrote a throwaway test file to analyse the CSV and ran it through `pytest`. Both are inside the sandbox and the policy, and both show that allowing `pytest` or a named script allows whatever the agent writes there (the threat model says this; the trial saw it used, for honest purposes).
4. **No way to delete a file.** INV-105's probe file could only be emptied.
5. **Wording.** `fs.write` says "everything that was in it before is gone" for a new file; the dropped-lines warning fires for a file the agent itself wrote earlier in the run.
6. **No baseline.** Nothing tells the agent which tests failed before it started, so "these failures are not mine" was each agent's inference.

None of these is a fault in the sandbox, the log or recovery. Items 1, 2, 4 and 5 are about what the agent is told; they are parked (spec §22), not fixed, because v1.0's scope is closed (D-114).
