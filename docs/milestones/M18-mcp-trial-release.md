# M18 — Claude Code through norn, and release 1.1.0

| | |
|---|---|
| Weeks in the plan | v2 (D-139); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` and the tags wait for a CI run |
| Commits | the M18 commit (tags `m15` to `m18` and `v1.1` there, after CI is green; D-150) |
| Status | Done on `nix`. Not yet verified on `fhs`; not tagged |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Show that v2 works with a capable agent that norn did not write, on real work, and release it. The six tickets that M13 ran through norn's own agent loop are run again with Claude Code as an MCP client, its own tools switched off; each result is checked on a fresh clone against the solution notes. Then the documents, the version number and the CHANGELOG. No code changes beyond the version.

## 2. Requirements

From `docs/v2-scope.md`, M18, and §7 "How v2 counts as done".

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| D1 the six tickets with Claude Code as an MCP client, own tools off | Yes, `nix` | `docs/results/mcp-claude-code-trial-nix.md`: six of six right against `SOLUTIONS.md` (D-149) |
| D2 the documents | Yes | README (status, "From an MCP client"), threat model (rows 36–38, §8), recovery semantics (§10), CHANGELOG, compatibility page |
| D3 release 1.1.0 | Built on `nix`; CI not yet | `Cargo.toml` 1.1.0; `packaging/release.sh --check`: `norn-1.1.0` packed and the demo run from it |
| An MCP client does a ticket from start to patch, `norn apply` applies it | Yes | M15's demo session and all six tickets here |
| Kill cells through MCP, 200 per cell, 0 violations | Yes, `nix` | `docs/results/recovery-mcp-200-nix.md` |
| Security and protocol cases through MCP, 0 escapes | Yes, `nix` | `make test` (second security pass), `tests/mcp.rs` |
| `make test` on both presets and Debian in CI | Not yet | needs a push |

## 3. What you need to know first

**A trial, not a benchmark.** One run per ticket. It shows that the pieces work together with a real agent and real work, and what that agent did; it does not measure a success rate.

**Checking on a clone.** The result is applied to a fresh `git clone` of the project with `norn apply --to`, and the checks run there. That tests what a person would get, not the workspace norn kept.

## 4. Experiments

None. The setup of Claude Code with its tools off was measured in M15 (E2).

## 5. What was built

| File | Purpose |
|---|---|
| `docs/results/mcp-claude-code-trial-nix.md` | The trial's record |
| `~/Temp/AgentOS_temp/run5/` (outside the repository) | `start.sh` (one Claude Code session per ticket), `check.sh` (finish, clone, apply, suite), `truth_check.py` (INV-105 row by row), each run's state and Claude Code's output |
| `README.md`, `docs/threat-model.md`, `docs/recovery-semantics.md`, `CHANGELOG.md` | v2 in the documents |
| `Cargo.toml` | 1.1.0 |

## 6. The code, explained

No new code. The trial scripts are small: `start.sh` writes an MCP configuration that starts `norn mcp` on the project with the M13 policy and a state folder of the ticket's own, writes the prompt (the task text plus two sentences), activates the project's virtualenv so that its `pytest` is on the `PATH` Claude Code passes to norn (M15 found that this matters), and runs `claude -p` with `--tools ""`. `check.sh` does what a person does afterwards. `truth_check.py` imports the clone's own parser and compares every row it reads with the truth file; on the original code it matches 17 of 2,613 rows, which shows the check can fail.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-149 | The trial: same project, tasks and policy as M13; Claude Code with its tools off | Leaving its tools on; several runs per ticket |
| D-150 | 1.1.0 in this commit; tags `m15`–`m18` and `v1.1` here after CI | Tags on each milestone's own commit |

## 8. Problems hit

None in norn. The README's status section had kept v0.1's numbers (130 tests, M0 to M10); it was rewritten with sources for each figure.

## 9. How to run it

```sh
cd ~/Temp/AgentOS_temp && run5/start.sh INV-101        # needs a run5/INV-101 that does not exist yet
run5/check.sh INV-101
nix develop -c packaging/release.sh --check              # the 1.1.0 archive, demo run from it
```

## 10. What this milestone does not do

- It does not measure how often an agent succeeds; one run per ticket.
- The model is from the same family as the one that built norn; an independent model is still untried through MCP.
- The LLM agent's baseline stop rule is not exercised here: Claude Code decides itself when it is done.

## 11. Check your understanding

1. Why were Claude Code's own tools switched off, and what would the trial show if they were not?
2. Why is each result checked on a fresh clone and not in norn's workspace?
3. How does `truth_check.py` show that it can tell a wrong result from a right one?
4. Why does `start.sh` activate the project's virtualenv before starting Claude Code?
5. INV-106's patch says it cannot carry two things. Which, and how did they still reach the clone?
6. Why are the milestone tags put on the release commit and not on each milestone's commit?
