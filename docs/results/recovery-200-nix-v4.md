# The 200-kill suite on the v4 tree, `nix` preset

*2026-11-28, development machine (NixOS, kernel 7.1.8), norn at the M26 working tree (after `d68aed0`: M23 to M25 committed, M26 uncommitted), debug build; `nix develop -c make eval-recovery`. Run because v4 changed what the gateway writes (`cause` on `tool.requested`, `usage` on `agent.message`) and what a resume writes (`execution.resumed`). The raw report of the main cells: `recovery-200-nix-v4.json`.*

## Totals, as the harness printed them

- fs.delete total: 1600 runs, 1600 killed, 0 violations; harness took 60 s with 8 workers
- fs.edit total: 1600 runs, 1600 killed, 0 violations; harness took 110 s with 8 workers
- external total: 1600 runs, 0 invariant violations, 0 duplicated effects, 946/946 cut-off calls classified correctly, 287 stops with the call left uncertain; successful `norn recover` wall time (includes waiting for the service) p50 18.0 ms, p99 328.8 ms; harness took 162 s with 8 workers
- total: 4800 runs, 4795 killed, 0 invariant violations, 0 duplicated effects, 3929/3929 cut-off calls classified correctly; `norn recover` wall time p50 18.6 ms, p99 76.4 ms; harness took 539 s with 8 workers
- mcp total: 4800 runs, 4779 killed, 0 invariant violations, 0 duplicated effects, 3908/3908 cut-off calls classified correctly; restart to first answer p50 88.2 ms, p99 158.6 ms; harness took 492 s with 8 workers
- network total: 4800 runs, 4800 killed, 0 invariant violations, 0 duplicated effects, 3956/3956 cut-off calls classified correctly; `norn recover` wall time p50 18.7 ms, p99 60.4 ms; harness took 583 s with 8 workers

All of them: 0 invariant violations and 0 duplicated effects. The tables follow, copied from the harness output.

## Tables

```text
| point | tool | runs | killed | violations |
|---|---|---|---|---|
| P1 | fs.delete | 200 | 200 | 0 |
| P2 | fs.delete | 200 | 200 | 0 |
| P3 | fs.delete | 200 | 200 | 0 |
| P4 | fs.delete | 200 | 200 | 0 |
| P5 | fs.delete | 200 | 200 | 0 |
| P6 | fs.delete | 200 | 200 | 0 |
| P7 | fs.delete | 200 | 200 | 0 |
| P8 | fs.delete | 200 | 200 | 0 |
| point | tool | runs | killed | violations |
|---|---|---|---|---|
| P1 | fs.edit | 200 | 200 | 0 |
| P2 | fs.edit | 200 | 200 | 0 |
| P3 | fs.edit | 200 | 200 | 0 |
| P4 | fs.edit | 200 | 200 | 0 |
| P5 | fs.edit | 200 | 200 | 0 |
| P6 | fs.edit | 200 | 200 | 0 |
| P7 | fs.edit | 200 | 200 | 0 |
| P8 | fs.edit | 200 | 200 | 0 |
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
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 199/199 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 197 | - | 0 | 0 | 166/166 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 185/185 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 197/197 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 118 | 0 | 0 | 193/193 |
| P8 | exec | 200 | 198 | 189 | 0 | 0 | 189/189 |
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
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 182 | - | 0 | 0 | 151/151 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 176/176 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 197/197 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 126 | 0 | 0 | 195/195 |
| P8 | exec | 200 | 197 | 189 | 0 | 0 | 190/190 |
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
| P3 | fs.read | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 199/199 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 200 | - | 0 | 0 | 190/190 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 180/180 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 198/198 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 119 | 0 | 0 | 197/197 |
| P8 | exec | 200 | 200 | 192 | 0 | 0 | 192/192 |
```
