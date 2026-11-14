# Fault-injection results, `nix` preset, after the first-user work (M8)

Command: `make eval-recovery` (200 kills per cell, tests run one at a time, D-081) on 2026-11-14, commit `9c7d434`, debug build, 8 workers, NixOS 26.05, kernel 7.1.8, btrfs. Raw reports: `recovery-200-nix-m8.json`, `recovery-ext-200-nix-m8.json`.

Why it was run again: since the last run of record on this machine (`recovery-200-nix-m7.md`), recovery itself changed (D-084: a cut-off `fs.write` that was going to make folders; D-086: how trees are deleted), and so did the startup check, the patch writer and how programs are found. One change came after this run: empty files in the patch's notes (D-095, `write_patch` only).

During the run a subagent was building a test repository on the same machine (light use: file writes and one `pip install`), so the recovery times may be a little higher than on an idle machine. The counts are not affected.

The `fhs` run of the same code is CI run 43590153676 (D-094).

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
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 199/199 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 195/195 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 195 | - | 0 | 0 | 154/154 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 174/174 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 190/190 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 114 | 0 | 0 | 197/197 |
| P8 | exec | 200 | 199 | 186 | 0 | 0 | 187/187 |
total: 4800 runs, 4794 killed, 0 invariant violations, 0 duplicated effects, 3896/3896 cut-off calls classified correctly; `agentos recover` wall time p50 19.5 ms, p99 45.3 ms; harness took 491 s with 8 workers

## The 8 external-call cells (M7)

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | cut-off calls classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P2 | killed after the intent, before the request | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P3 | killed while the request is in flight | 200 | 195 | 195 | - | 0 | 0 | 0 | 158/158 |
| P7 | killed after the completion record | 200 | 200 | 200 | - | 0 | 0 | 0 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 200 | 193 | 193 | 187 | 0 | 0 | 0 | 175/175 |
| T1 | request lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T2 | answer lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T3 | no answer to any attempt (no kill) | 200 | 200 | 0 | - | 307 | 0 | 0 | 200/200 |
external total: 1600 runs, 0 invariant violations, 0 duplicated effects, 933/933 cut-off calls classified correctly, 307 stops with the call left uncertain; successful `agentos recover` wall time (includes waiting for the service) p50 19.3 ms, p99 328.4 ms; harness took 157 s with 8 workers
