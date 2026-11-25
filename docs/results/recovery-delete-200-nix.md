# fs.delete kill cells, 200 per point (M17, `nix` preset)

*2026-11-26, development machine (NixOS 26.05, kernel 7.1.8), 8 workers. Command: `nix develop -c env NORN_RECOVERY_RUNS=200 systemd-run --user --scope -p Delegate=yes cargo test --test recovery a_delete_is_recovered -- --nocapture`.*

The run is killed while it deletes `src/a.txt` (a 400 KB file, call 2); `norn recover`, then `norn run --resume`. The third step writes `src/a.txt` again, so a lost delete or one applied after that write would show in the end state. Checks: the workspace equals the last checkpoint after recovery and after the resume, no leftover checkpoints, recovery's verdict on the cut-off delete matches whether the file is there, the run completes, and `src/a.txt` ends as `after\n` with no failed call (D-147).

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

**fs.delete total: 1600 runs, 1600 killed, 0 violations; harness took 62 s with 8 workers**

P3 kills at a random moment during the effect; an `unlink` is so short that the kill nearly always lands after it, so P3 here mostly tests the same state as P4. One run, one machine; `fhs` not yet run.
