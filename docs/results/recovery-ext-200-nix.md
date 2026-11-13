# Fault-injection results for the external tool (`ext.call`), `nix` preset

Command: `make eval-recovery` (200 runs per cell, tests run one at a time, D-081) on 2026-11-13, on the working tree that became the M7 commit, debug build, 8 workers, NixOS 26.05, kernel 7.1.8, btrfs. Raw report: `recovery-ext-200-nix.json`. The same command's result for the 24 original cells is `recovery-200-nix-m7.md`.

**Not yet verified on the `fhs` preset**: the bonus commits had not been pushed when this was written, so CI had not run them.

The workload has two `ext.call` steps among file and command steps. Each run aims at one of them. In the P cells the runtime is killed; in the T cells nothing is killed and the fake service misbehaves. The oracle is the service's own record of what it applied (D-080).

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | cut-off calls classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P2 | killed after the intent, before the request | 200 | 200 | 200 | - | 0 | 0 | 0 | 200/200 |
| P3 | killed while the request is in flight | 200 | 192 | 192 | - | 0 | 0 | 0 | 152/152 |
| P7 | killed after the completion record | 200 | 200 | 200 | - | 0 | 0 | 0 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 200 | 195 | 195 | 192 | 0 | 0 | 0 | 180/180 |
| T1 | request lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T2 | answer lost (no kill) | 200 | 200 | 0 | - | 0 | 0 | 0 | 0/0 |
| T3 | no answer to any attempt (no kill) | 200 | 200 | 0 | - | 288 | 0 | 0 | 200/200 |

external total: 1600 runs, 0 invariant violations, 0 duplicated effects, 932/932 cut-off calls classified correctly, 288 stops with the call left uncertain; successful `agentos recover` wall time (includes waiting for the service) p50 19.3 ms, p99 328.3 ms; harness took 152 s with 8 workers.

## How to read the columns

- **as intended**: the cell's situation really occurred (the kill happened; or, for T1 and T2, the run finished without stopping; or, for T3, the run stopped with the call uncertain). The test fails if this is not more than half of a cell's runs.
- **stopped uncertain**: how many times a run or a recovery ended with "no answer, nothing decided". In T3 that is at least once per run by design, and twice when the first recovery found the service still silent.
- **duplicated effects**: orders found more than once in the service's record, counted by payload, plus extra lines from the command step.
- **classified correctly**: for runs in which an `ext.call` was cut off, recovery's APPLIED / NOT APPLIED against the service's record. P7 has none (the call had completed). In P3 the timer sometimes fires after the call has ended, so fewer than 200 are cut off.
- The recovery times include waiting on a service that is deliberately slow or silent. They are not comparable with the v0.1 recovery times and are not a benchmark.

## The same cells with both big tests running at once

The first full-size run started the external cells and the 24 original cells together (16 workers). It is kept as `recovery-ext-200-nix-concurrent.json` and `recovery-200-nix-m7-concurrent.json`: external 1600 runs, 0 violations, 0 duplicated effects, 917/917 classified correctly; original 4800 runs, 0 violations, 0 duplicated effects. It passed; it is not the result of record because of the doubled load.

Whether the harness can fail at all: `recovery-ext-mutations.md`.
