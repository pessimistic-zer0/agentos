# Fault-injection results, `fhs` preset

This is the `fhs` result of record. It comes from CI, not from the development machine.

Source: GitHub Actions run <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> on 2026-11-13, commit `c3c9502`, runner image `ubuntu-24.04` (Ubuntu 24.04.5 LTS) version 20261101.360.1, rustc 1.100.0, `AGENTOS_TEST_PRESET=fhs`, `make eval-recovery` (200 kills per cell), debug build, 8 workers. Raw report (the run's `recovery-report-fhs` artifact, unchanged): `recovery-200-fhs.json`.

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
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 76/76 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 89/89 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 200 | - | 0 | 0 | 197/197 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 198/198 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 111 | 0 | 0 | 200/200 |
| P8 | exec | 200 | 200 | 195 | 0 | 0 | 196/196 |

total: 4800 runs, 4800 killed, 0 invariant violations, 0 duplicated effects, 3756/3756 cut-off calls classified correctly; `agentos recover` wall time p50 33.5 ms, p99 315.3 ms; harness took 1366 s with 8 workers.

## How to read this

- It is the third CI run. The first two failed this suite and each found a bug: `recovery-200-fhs-run1-failed.md` (B-007) and `recovery-200-fhs-run2-failed.md` (B-008). One green run after two red ones is one green run; it is not a statistic.
- The recovery times are from a shared CI runner under an 8-worker load and are not comparable with the `nix` numbers from the laptop. The p99 here (315.3 ms) is higher than in the two failed runs (97.1 ms and 108.3 ms). A possible cause is the lock wait added for B-008 (D-071); that was not checked.
- The log does not say which filesystem or kernel version the runner used, so none is claimed here.
- Not covered by this run: the three overhead benchmarks and `make demo` were not run on Ubuntu.
