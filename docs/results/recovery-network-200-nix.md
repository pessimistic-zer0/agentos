# Fault injection with v3, `nix` preset, 200 kills per cell

Command: `make eval-recovery` on 2026-11-26, at `1fbe1d1` (v3: M19 to M22 before the version change), NixOS, kernel 7.1.8. It runs every kill test of `tests/recovery/main.rs` at 200 runs per cell, then the main cells again with `[network]` on (`NORN_RECOVERY_NETWORK=1`): every `exec` first asks the runtime's proxy for a tunnel, so the proxy is started, used and stopped inside every killed command (`docs/recovery-semantics.md` §11).

Raw reports: `recovery-200-nix-v3.json` (the main cells), `recovery-network-200-nix.json` (with the network).

```text
fs.delete total: 1600 runs, 1600 killed, 0 violations; harness took 59 s with 8 workers
fs.edit total: 1600 runs, 1600 killed, 0 violations; harness took 100 s with 8 workers
external total: 1600 runs, 0 invariant violations, 0 duplicated effects, 936/936 cut-off calls classified correctly, 306 stops with the call left uncertain; successful `norn recover` wall time (includes waiting for the service) p50 19.5 ms, p99 329.6 ms; harness took 162 s with 8 workers
total: 4800 runs, 4789 killed, 0 invariant violations, 0 duplicated effects, 3910/3910 cut-off calls classified correctly; `norn recover` wall time p50 19.7 ms, p99 50.1 ms; harness took 523 s with 8 workers
mcp total: 4800 runs, 4783 killed, 0 invariant violations, 0 duplicated effects, 3906/3906 cut-off calls classified correctly; restart to first answer p50 91.3 ms, p99 173.9 ms; harness took 514 s with 8 workers
network total: 4800 runs, 4799 killed, 0 invariant violations, 0 duplicated effects, 3931/3931 cut-off calls classified correctly; `norn recover` wall time p50 20.0 ms, p99 54.3 ms; harness took 558 s with 8 workers
```

No `VIOLATION` line was printed.
