
| point | tool | runs | killed | recovery killed (P8) | violations | duplicated effects | cut-off calls classified correctly |
| P1 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P1 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P2 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 195/195 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 196/196 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 199/199 |
| P3 | exec | 200 | 192 | - | 0 | 0 | 144/144 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 165/165 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 185/185 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 126 | 0 | 0 | 192/192 |
| P8 | exec | 200 | 196 | 189 | 0 | 0 | 190/190 |

total: 4800 runs, 4788 killed, 0 invariant violations, 0 duplicated effects, 3866/3866 cut-off calls classified correctly; `agentos recover` wall time p50 19.1 ms, p99 49.8 ms; harness took 492 s with 8 workers

