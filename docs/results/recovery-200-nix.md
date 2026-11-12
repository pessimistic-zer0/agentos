# Fault-injection results, `nix` preset

Command: `make eval-recovery` (200 kills per cell) on 2026-11-12, commit 9108c1f plus the M6 documents. One source change came after this run: execution ids gained milliseconds (D-067); `make test`, with 72 kills, passed after it, debug build, 8 workers, NixOS 26.05, kernel 7.1.8, btrfs. Raw report: `recovery-200-nix.json`. The run made at the end of M3, on older code, is kept as `recovery-200-nix-m3.*`.

| point | tool | runs | killed | recovery killed (P8) | violations | duplicated effects | cut-off calls classified correctly |
| P1 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 197/197 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 197 | - | 0 | 0 | 156/156 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 176/176 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 120 | 0 | 0 | 197/197 |
| P8 | exec | 200 | 200 | 191 | 0 | 0 | 192/192 |
total: 4800 runs, 4797 killed, 0 invariant violations, 0 duplicated effects, 3918/3918 cut-off calls classified correctly; `agentos recover` wall time p50 20.4 ms, p99 55.1 ms; harness took 575 s with 8 workers
