# Fault-injection results, `fhs` preset, first CI run: FAILED

This is **not** a passing result. It is kept because it found a bug (B-007 in `docs/bugs.md`, D-070).

Source: GitHub Actions run <https://github.com/pessimistic-zer0/agentos/actions/runs/43274718767> on 2026-11-12, commit `8eaca2f` (tag `v0.1`), Ubuntu 24.04 runner, `AGENTOS_TEST_PRESET=fhs`, `make eval-recovery` (200 kills per cell), debug build, 8 workers. Raw report (the run's `recovery-report-fhs` artifact, unchanged): `recovery-200-fhs-run1-failed.json`.

Earlier steps of the same run passed: lint, and `make test` (unit tests, vertical slice, 32 security cases, benign suite, LLM mock, quick recovery).

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
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 50/50 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 65/65 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 197/197 |
| P3 | exec | 200 | 200 | - | 0 | 0 | 164/164 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 196/196 |
| P5 | exec | 200 | 200 | - | 1 | 0 | 199/199 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 121 | 0 | 0 | 200/200 |
| P8 | exec | 200 | 200 | 197 | 0 | 0 | 197/197 |

total: 4800 runs, 4800 killed, **1 invariant violation**, 0 duplicated effects, 3668/3668 cut-off calls classified correctly; `agentos recover` wall time p50 25.1 ms, p99 97.1 ms; harness took 1022 s with 8 workers.

The violation, from the run's log:

```text
VIOLATION P5 exec #155: ["leftover after recovery: checkpoints on disk [\".tmp-cp-0002\", \"cp-0000\", \"cp-0001\"], named in the log [\"cp-0000\", \"cp-0001\"]"]
```

A half-made checkpoint folder was on disk after recovery had finished. No effect was lost or doubled, and every verdict was right; the leftover is what the "no leftovers" check exists to catch.
