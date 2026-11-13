# Fault-injection results for the external tool (`ext.call`), `fhs` preset

Source: GitHub Actions run <https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321> on 2026-11-13, commit `1bc27da` (tag `m7`), Ubuntu 24.04 runner, `AGENTOS_TEST_PRESET=fhs`, `make eval-recovery` (200 runs per cell, tests one at a time), debug build, 8 workers. Conclusion: success. Raw reports (the run's artifact, unchanged): `recovery-ext-200-fhs.json` and, for the 24 original cells in the same run, `recovery-200-fhs-m7.json`.

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | cut-off calls classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P2 | killed after the intent, before the request | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P3 | killed while the request is in flight | 200 | 195 | 195 | - | 0 | 0 | 0 | 162/162 |
| P7 | killed after the completion record | 200 | 200 | 200 | - | 0 | 0 | 0 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 200 | 199 | 199 | 192 | 0 | 0 | 0 | 180/180 |
| T1 | request lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T2 | answer lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T3 | no answer to any attempt (no kill) | 200 | 200 | 0 | - | 303 | 0 | 0 | 200/200 |

external total: 1600 runs, 0 invariant violations, 0 duplicated effects, 942/942 cut-off calls classified correctly, 303 stops with the call left uncertain; successful `agentos recover` wall time (includes waiting for the service, shared runner) p50 18.1 ms, p99 345.0 ms.

The 24 original cells in the same run: 4800 runs, 4799 killed, 0 invariant violations, 0 duplicated effects, 3724/3724 classified correctly; `agentos recover` p50 35.0 ms, p99 115.6 ms.

This is one green run. The run's earlier steps (lint, `make test` including `tests/external.rs`) also passed. Column meanings: see `recovery-ext-200-nix.md`.
