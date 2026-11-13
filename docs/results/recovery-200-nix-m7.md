# Fault-injection results, `nix` preset, after the bonus (M7)

Command: `make eval-recovery` (200 kills per cell, tests run one at a time, D-081) on 2026-11-13, on the working tree that became the M7 commit, debug build, 8 workers, NixOS 26.05, kernel 7.1.8, btrfs. Raw report: `recovery-200-nix-m7.json`. This is the same run as `recovery-ext-200-nix.md`; it shows that the 24 original cells are unaffected by the fifth tool. The v0.1 result of record stays `recovery-200-nix.md`.

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
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 195/195 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 197/197 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 192 | - | 0 | 0 | 147/147 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 178/178 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 194/194 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 117 | 0 | 0 | 195/195 |
| P8 | exec | 200 | 199 | 188 | 0 | 0 | 189/189 |

total: 4800 runs, 4791 killed, 0 invariant violations, 0 duplicated effects, 3895/3895 cut-off calls classified correctly; `agentos recover` wall time p50 19.8 ms, p99 55.9 ms; harness took 530 s with 8 workers.
