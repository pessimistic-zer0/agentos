# Fault-injection results, `fhs` preset, second CI run: FAILED

This is **not** a passing result. It is kept because it found a bug that the fix for the first one had introduced (B-008 in `docs/bugs.md`, D-071).

Source: GitHub Actions run <https://github.com/pessimistic-zer0/agentos/actions/runs/43375633929> on 2026-11-13, commit `4294b9d` (the B-007 fix), Ubuntu 24.04 runner, `AGENTOS_TEST_PRESET=fhs`, `make eval-recovery` (200 kills per cell), debug build, 8 workers. Raw report (the run's `recovery-report-fhs` artifact, unchanged): `recovery-200-fhs-run2-failed.json`.

Earlier steps of the same run passed: lint, and `make test`, including the new B-007 regression test.

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
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 46/46 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 55/55 |
| P3 | fs.write | 200 | 200 | - | 2 | 0 | 198/198 |
| P3 | exec | 200 | 200 | - | 0 | 0 | 195/195 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 1 | 0 | 199/199 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 117 | 0 | 0 | 200/200 |
| P8 | exec | 200 | 200 | 200 | 0 | 0 | 200/200 |

total: 4800 runs, 4800 killed, **3 invariant violations**, 0 duplicated effects, 3693/3693 cut-off calls classified correctly; `agentos recover` wall time p50 34.7 ms, p99 108.3 ms; harness took 1458 s with 8 workers.

The violations, from the run's log (all three the same error):

```text
VIOLATION P3 fs.write #66:  ["recover failed: Error: execution 20261113-061232738-91188a is in use by another agentos process\n"]
VIOLATION P3 fs.write #122: ["recover failed: Error: execution 20261113-061249612-5f627d is in use by another agentos process\n"]
VIOLATION P5 fs.write #159: ["recover failed: Error: execution 20261113-061709532-08cbb0 is in use by another agentos process\n"]
```

Recovery refused to start because the execution's lock was still held. Nothing was damaged: a refused recovery changes nothing. The B-007 symptom (a leftover `.tmp-cp-…` folder) did not appear in this run.
