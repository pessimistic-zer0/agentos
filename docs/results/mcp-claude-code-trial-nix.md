# The six inventory tickets with Claude Code as an MCP client (M18, `nix` preset)

*2026-11-26, development machine (NixOS 26.05, kernel 7.1.8), norn at commit `9c82dca` (M17), debug build; Claude Code 2.1.332 on Prince's account (D-139), model `claude-opus-5-5`. One run per ticket, all six at once; a record, not a benchmark. The scripts and every run's state folder are in `~/Temp/AgentOS_temp/run5/` (outside this repository).*

## How it was run

The same project, task texts and policy as the M13 trial (`relay-model-trial-nix.md`): `inventory-service` at commit `0755692` (3 failed, 75 passed before any change), `run3/task-INV-10x.txt`, `run3/policy.toml`. What differs is the agent:

- **M13:** norn's own LLM loop (`norn run --agent llm`), with Claude subagents answering through a relay that looked like a model server.
- **M18:** Claude Code itself, connected to `norn mcp`, with its own tools switched off: `claude -p "<task>" --mcp-config <norn> --strict-mcp-config --tools "" --allowedTools mcp__norn`. So norn's tools were the only way to read, write or run anything (M15, E2). Each ticket a fresh session and its own state folder (`run5/start.sh`).

The prompt was the ticket's task text plus two sentences: work only through the norn tools; when done, say so, and a person reviews and applies the result with `norn finish` and `norn apply`.

Then, as the person (`run5/check.sh`): `norn finish`, `git clone` of the project, `norn apply --to` the clone, the suite on the clone, and the ticket's checks from `SOLUTIONS.md`. INV-105 was also compared row by row with `movements_truth.json` (`run5/truth_check.py`; on the original code it matches 17 of 2,613 rows, so it does tell right from wrong).

## Results

| Ticket | Session | Calls (denied) | Suite on the clone | Against the solution notes |
|---|---|---|---|---|
| INV-101 one-line bug | 12 s, 5 turns, 0.08 USD | 4 (0) | 2 failed, 76 passed | **Right.** `>` became `>=`, the only change; the 2 failures are INV-102's |
| INV-102 bug across two modules | 26 s, 14 turns, 0.19 USD | 13 (0) | 1 failed, 77 passed | **Right.** Currency parsed and used in `quote`; the order model's currency became optional with the settings' default used when absent; `WEB-104240 (EUR)` |
| INV-103 new package and command | 72 s, 27 turns, 0.44 USD | 26 (1) | 3 failed, 86 passed | **Right.** `alerts` with `--catalog`, `--warehouse`, `--csv`: US-EAST 8 alerts, first ELC-3004 (available -363, shortfall 403); US-WEST 14, first PLB-4010 (331); BER 15, first FST-1009 (238, 500, 262). New tests under `tests/alerts/` |
| INV-104 tax per warehouse | 46 s, 23 turns, 0.29 USD | 22 (0) | 3 failed, 82 passed | **Right.** A `tax_rate` per warehouse in `config/settings.yaml`; quote taxes 40.77, 183.75, 278.08; tests added to two existing files, no line removed |
| INV-105 the large fixture | 92 s, 32 turns, 0.62 USD | 31 (2) | 3 failed, 94 passed | **Right.** `import`: 2,613 read, 0 skipped, exit 0; 2,613 of 2,613 rows equal `movements_truth.json`; the ISO-date tests still pass (no global day-first). A new test file; its throwaway analysis test was deleted with `fs.delete` |
| INV-106 binary fixture and script | 66 s, 31 turns, 0.35 USD | 30 (1) | 3 failed, 79 passed | **Right.** Version 2, same `taken_at` and 10 items, reorder points FST 500, TLS 15, ELC 40, PLB 25; `scripts/regen_snapshot.py` is executable and writes the same bytes twice. The patch listed the binary file and the mode it could not carry; `apply` carried both |

In every row the remaining failures are the other open tickets' (INV-101, INV-102). No existing test lost a line. The original project was unchanged after all six (`git status` clean).

**Six of six solved**, as in M13. In all: 126 calls, 4 refused, 1.97 USD, under two minutes per ticket.

## What it showed

**The front door held.** Every call went through the gateway; the log of each execution holds them all, with checkpoints. Claude Code had no other tool. Each execution stayed `open` until `norn finish`, as designed (D-141).

**The M17 tools were used at once:**
- `fs.search`: 9 searches in 5 of 6 tickets, where M13's agents read whole files to find things.
- `fs.delete`: INV-105 wrote a throwaway test to analyse the CSV, ran it, and deleted it. In M13 the same ticket left an empty file behind.
- Refusals with hints: 4 refusals in 126 calls (M13: 6 in 90). INV-103 was refused `python -m …` and ran the allowed `python3 -m …` next. INV-105 was refused an edit of `README.md` (not in the write rules) and went on.

**What it did not show.** The LLM agent's stop rule and its baseline (D-145) do not apply here: Claude Code decides itself when it is done. The baseline note in `exec` results is not in the log (only what the client was answered is), so how often the agents saw it is not recorded. One agent (INV-102) once narrowed its test command to three files, then ran the full suite.

**What stays true from M13.** An allowed `pytest` runs whatever test file the agent writes (INV-105's analysis test). That is inside the sandbox and the policy, and in the threat model.

**Not comparable with M13 in time or cost.** M13's relay put a human-scale loop in every turn and its runs were interrupted by usage limits; these sessions ran without a break. The model is the same family in both.
