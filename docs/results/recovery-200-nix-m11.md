# Fault-injection results, `nix` preset, after M11

Command: `make eval-recovery` (200 kills per cell, tests run one at a time, D-081) on 2026-11-16, started at commit `1600435` (the commits after it change documents only), debug build, 8 workers, NixOS 26.05, kernel 7.1.8, btrfs. Raw reports: `recovery-200-nix-m11.json`, `recovery-ext-200-nix-m11.json`. A CI watcher and an editor session were running on the machine; it was not otherwise loaded.

Why it was run again: M11 changed code on the paths this suite kills. Every sandboxed command now gets a seccomp filter and a file-size limit (D-116, D-117); the watchdog thread measures the workspace while a command runs (D-117); and a checkpoint is now a copy from which scratch paths are deleted before it is renamed into place (D-118), which is also what recovery does when it rolls an `fs.write` forward. This run shows the five invariants still hold.

What it does not show: the suite's commands stay far below `limits.workspace_mb`, so the kill-and-undo path of the disk limit is not exercised under kills here; `tests/hardening.rs` tests it without kills.

On `fhs`: see the milestone document for the CI run.

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
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 199/199 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 197 | - | 0 | 0 | 153/153 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 182/182 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 198/198 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 122 | 0 | 0 | 196/196 |
| P8 | exec | 200 | 198 | 185 | 0 | 0 | 186/186 |
total: 4800 runs, 4795 killed, 0 invariant violations, 0 duplicated effects, 3912/3912 cut-off calls classified correctly; `agentos recover` wall time p50 20.3 ms, p99 61.1 ms; harness took 550 s with 8 workers

## The 8 external-call cells (M7)

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | cut-off calls classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P2 | killed after the intent, before the request | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P3 | killed while the request is in flight | 200 | 192 | 192 | - | 0 | 0 | 0 | 161/161 |
| P7 | killed after the completion record | 200 | 200 | 200 | - | 0 | 0 | 0 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 200 | 198 | 198 | 194 | 0 | 0 | 0 | 178/178 |
| T1 | request lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T2 | answer lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T3 | no answer to any attempt (no kill) | 200 | 200 | 0 | - | 299 | 0 | 0 | 200/200 |
external total: 1600 runs, 0 invariant violations, 0 duplicated effects, 939/939 cut-off calls classified correctly, 299 stops with the call left uncertain; successful `agentos recover` wall time (includes waiting for the service) p50 20.0 ms, p99 332.6 ms; harness took 166 s with 8 workers
