# Kill cells through `norn mcp`, 200 per cell (M16, `nix` preset)

*2026-11-26, development machine (NixOS 26.05, kernel 7.1.8), 8 workers, the M16 working tree. Command:*

```sh
nix develop -c env NORN_RECOVERY_RUNS=200 systemd-run --user --scope -p Delegate=yes \
  cargo test --test recovery recovery_holds_through_mcp -- --nocapture --test-threads=1
```

The workload, crash points and invariants are those of `recovery_holds_at_every_crash_point`; the driver is an MCP client that starts `norn mcp` again after each kill, as Claude Code does (D-144). Raw report: `recovery-mcp-200-nix.json`.

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
| P3 | fs.list | 200 | 200 | - | 0 | 0 | 199/199 |
| P3 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P3 | exec | 200 | 189 | - | 0 | 0 | 140/140 |
| P4 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P4 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P5 | fs.write | 200 | 200 | - | 0 | 0 | 178/178 |
| P5 | exec | 200 | 200 | - | 0 | 0 | 198/198 |
| P6 | fs.write | 200 | 200 | - | 0 | 0 | 200/200 |
| P6 | exec | 200 | 200 | - | 0 | 0 | 200/200 |
| P7 | fs.read | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.list | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | fs.write | 200 | 200 | - | 0 | 0 | 0/0 |
| P7 | exec | 200 | 200 | - | 0 | 0 | 0/0 |
| P8 | fs.write | 200 | 200 | 115 | 0 | 0 | 198/198 |
| P8 | exec | 200 | 193 | 183 | 0 | 0 | 185/185 |

**mcp total: 4800 runs, 4782 killed, 0 invariant violations, 0 duplicated effects, 3897/3897 cut-off calls classified correctly; restart to first answer p50 85.3 ms, p99 141.1 ms; harness took 480 s with 8 workers**

Notes:

- **Killed** counts the first session. P3 kills at a random moment inside the effect and can come after the call is done; 11 of the P3 `exec` runs and 7 of the P8 `exec` runs (whose first kill is drawn from P2 to P6) ended without a kill, and then simply carried on.
- **Recovery killed (P8)**: the kill lands on the n-th pass of P8 inside recovery, n from 1 to 5; recovery with less work than that finishes first. Those trials still check everything else.
- **Restart to first answer** is the time from starting the server again to the answer of its first call, which includes reopening the execution and recovery. It is not comparable with `norn recover`'s time in the other suites: it adds starting a process, the delegated-scope check, and one `fs.list`.
- One run, on one machine. The `fhs` preset is not yet run (needs CI).
