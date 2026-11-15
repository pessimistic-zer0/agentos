# Fault-injection results, `nix` preset, after M9

Command: `make eval-recovery` (200 kills per cell, tests run one at a time, D-081) on 2026-11-15, commit `c2c1e00` with the M9 documents not yet committed (no source change after it), debug build, 8 workers, NixOS 26.05, kernel 7.1.8, btrfs. Raw reports: `recovery-200-nix-m9.json`, `recovery-ext-200-nix-m9.json`. The machine was otherwise idle.

Why it was run again: M9 changed what the runtime does around a call without changing recovery itself: a signal handler is installed for every run (D-106), the revert after a command spares scratch paths (D-099), the patch writer filters sections, `fs.read` has a new code path (D-102), and the shipped policies have new lists. This run shows that none of it disturbed the five invariants.

Not yet run on `fhs`.

## The 24 cells of v0.1

| point | tool | runs | killed | recovery killed (P8) | violations | duplicated effects | cut-off calls classified correctly |
|---|---|---|---|---|---|---|---|
| P1 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 198/198 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 189 | - | 0 | 0 | 138/138 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 188/188 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 191/191 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 119 | 0 | 0 | 196/196 |
| P8 | exec | 200 | 199 | 187 | 0 | 0 | 188/188 |
total: 4800 runs, 4788 killed, 0 invariant violations, 0 duplicated effects, 3899/3899 cut-off calls classified correctly; `agentos recover` wall time p50 19.6 ms, p99 54.0 ms; harness took 534 s with 8 workers

## The 8 external-call cells (M7)

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | cut-off calls classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P2 | killed after the intent, before the request | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P3 | killed while the request is in flight | 200 | 194 | 194 | - | 0 | 0 | 0 | 155/155 |
| P7 | killed after the completion record | 200 | 200 | 200 | - | 0 | 0 | 0 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 200 | 195 | 195 | 189 | 0 | 0 | 0 | 177/177 |
| T1 | request lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T2 | answer lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T3 | no answer to any attempt (no kill) | 200 | 200 | 0 | - | 306 | 0 | 0 | 200/200 |
external total: 1600 runs, 0 invariant violations, 0 duplicated effects, 932/932 cut-off calls classified correctly, 306 stops with the call left uncertain; successful `agentos recover` wall time (includes waiting for the service) p50 19.2 ms, p99 330.4 ms; harness took 163 s with 8 workers
