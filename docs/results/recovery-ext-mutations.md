# External-call harness: can it fail?

A test that has never failed has not been tested. On 2026-11-13 two bugs were put into the runtime on purpose, one at a time, and the external cells of the fault-injection harness were run against each (20 runs per cell, `nix` preset, debug build, 8 workers). The real code was run the same way first. The source was restored after each run; `git status` showed no change under `src/`.

Command:

```sh
AGENTOS_RECOVERY_RUNS=20 systemd-run --user --scope -p Delegate=yes -q \
    cargo test --test recovery external_calls -- --nocapture
```

Columns: cell, what happens, runs, as intended, killed, recovery killed, stopped uncertain, violations, duplicated effects, cut-off calls classified correctly. The tables are the test's own output.

## The real code: passes

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 20 | 20 | 20 | - | 0 | 0 | 0 | 20/20 |
| P2 | killed after the intent, before the request | 20 | 20 | 20 | - | 0 | 0 | 0 | 20/20 |
| P3 | killed while the request is in flight | 20 | 20 | 20 | - | 0 | 0 | 0 | 20/20 |
| P7 | killed after the completion record | 20 | 20 | 20 | - | 0 | 0 | 0 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 20 | 20 | 20 | 19 | 0 | 0 | 0 | 18/18 |
| T1 | request lost (no kill) | 20 | 20 | 0 | - | 0 | 0 | 0 | 0/0 |
| T2 | answer lost (no kill) | 20 | 20 | 0 | - | 0 | 0 | 0 | 0/0 |
| T3 | no answer to any attempt (no kill) | 20 | 20 | 0 | - | 29 | 0 | 0 | 20/20 |

external total: 160 runs, 0 invariant violations, 0 duplicated effects, 98/98 cut-off calls classified correctly, 29 stops with the call left uncertain.

## Seeded bug 1: recovery says NOT APPLIED without asking the service

In `recovery.rs`, the `ext.call` branch was replaced by "not applied", the rule that is right for a read. This is the naive design: "we do not know, so tell the agent it did not happen". The agent then asks again, under a new key.

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 20 | 20 | 20 | - | 0 | 0 | 0 | 20/20 |
| P2 | killed after the intent, before the request | 20 | 20 | 20 | - | 0 | 0 | 0 | 20/20 |
| P3 | killed while the request is in flight | 20 | 19 | 19 | - | 0 | 24 | 13 | 2/13 |
| P7 | killed after the completion record | 20 | 20 | 20 | - | 0 | 0 | 0 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 20 | 20 | 20 | 20 | 0 | 16 | 9 | 10/17 |
| T1 | request lost (no kill) | 20 | 20 | 0 | - | 0 | 0 | 0 | 0/0 |
| T2 | answer lost (no kill) | 20 | 20 | 0 | - | 0 | 0 | 0 | 0/0 |
| T3 | no answer to any attempt (no kill) | 20 | 20 | 0 | - | 28 | 45 | 26 | 1/20 |

external total: 160 runs, **85 invariant violations, 48 duplicated effects**, 53/90 cut-off calls classified correctly. Test result: FAILED.

Reading it: P1 and P2 stay clean, correctly, because no request had left when the runtime died, so "not applied" happens to be true. Every cell in which a request could have arrived (P3, P8, T3) shows orders applied twice.

## Seeded bug 2: a new key for every attempt

In `tools/ext.rs`, each attempt was sent with `<key>-try<n>` instead of the same key. This is "retry on timeout" without idempotency.

| cell | what happens | runs | as intended | killed | recovery killed | stopped uncertain | violations | duplicated effects | classified correctly |
|---|---|---|---|---|---|---|---|---|---|
| P1 | killed before the intent | 20 | 20 | 20 | - | 0 | 94 | 5 | 20/20 |
| P2 | killed after the intent, before the request | 20 | 20 | 20 | - | 0 | 112 | 7 | 0/20 |
| P3 | killed while the request is in flight | 20 | 17 | 17 | - | 0 | 138 | 0 | 0/16 |
| P7 | killed after the completion record | 20 | 20 | 20 | - | 0 | 131 | 9 | 0/0 |
| P8 | killed at P2 or P3, then recovery killed too | 20 | 20 | 20 | 20 | 0 | 137 | 7 | 0/20 |
| T1 | request lost (no kill) | 20 | 20 | 0 | - | 0 | 182 | 0 | 0/0 |
| T2 | answer lost (no kill) | 20 | 20 | 0 | - | 0 | 172 | 20 | 0/0 |
| T3 | no answer to any attempt (no kill) | 20 | 20 | 0 | - | 33 | 271 | 22 | 0/20 |

external total: 160 runs, **1237 invariant violations, 70 duplicated effects**, 20/96 cut-off calls classified correctly. Test result: FAILED.

Reading it: the many violations are mostly the "I1" check (the service received a key that no intent in the log names) and the "log and service disagree" check, which fire on every request. The duplicated effects are the real damage: in T2 (answer lost) all 20 runs applied the order twice. T1 (request lost) shows 0 duplicates, correctly: a request that never arrived cannot be doubled by a retry.
