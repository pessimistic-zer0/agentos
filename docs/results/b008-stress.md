# B-008: stress runs on the development machine

The second CI run failed with "execution … is in use by another agentos process" (`recovery-200-fhs-run2-failed.md`). These runs were made on 2026-11-13 to reproduce it locally and to find out which commit brought it in. NixOS, `nix` preset, 12 logical CPUs, debug build.

Command (64 workers instead of 8, so that processes wait for CPU time; 60 kills per cell):

```sh
AGENTOS_RECOVERY_JOBS=64 AGENTOS_RECOVERY_RUNS=60 systemd-run --user --scope -p Delegate=yes -q \
    cargo test --test recovery recovery_holds -- --nocapture
```

The `total:` line each run printed:

| Code | Result |
|---|---|
| `4294b9d` (B-007 fix, before the B-008 fix) | 1440 runs, 1440 killed, **4 invariant violations**, 0 duplicated effects, 1194/1194 classified correctly. All four: `recover failed: … is in use by another agentos process` (cells `P5 fs.write` ×3, `P5 exec` ×1) |
| tag `v0.1` (`8eaca2f`), run 1 | 1440 runs, 1440 killed, 0 invariant violations, 0 duplicated effects, 1200/1200 |
| tag `v0.1` (`8eaca2f`), run 2 | 1440 runs, 1440 killed, 0 invariant violations, 0 duplicated effects, 1200/1200 |
| with the B-008 fix, run 1 | 1440 runs, 1440 killed, 0 invariant violations, 0 duplicated effects, 1200/1200 |
| with the B-008 fix, run 2 | 1440 runs, 1440 killed, 0 invariant violations, 0 duplicated effects, 1200/1200 |

Only the last run's JSON report was kept (`b008-stress-after-fix.json`); each run overwrites `target/recovery-report.json`, and the earlier ones were not copied in time. The lines above were copied from the terminal output.

What this shows: the error appears with the B-007 fix and not at `v0.1`, so the fix brought it in (or made a window that already existed wide enough to hit). What it does not show: that the `v0.1` code can never hit it. Recovery times in these runs are not comparable with the results of record, because 64 workers on 12 CPUs is deliberate overload.
