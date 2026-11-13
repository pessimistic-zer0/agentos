# ADR 0003 — An uncertain `exec` is rolled back

**Status:** accepted · **Log entries:** D-016, D-053, D-055

## Question
The runtime died while a command was running. What it changed cannot be known. Roll the workspace back to the last checkpoint, or stop and ask a human?

## Decision
Roll back, record the call as NOT APPLIED, and tell the agent. It may issue the command again as a new call.

## Why
Everything a v1 command can change is inside `/work`, and `/work` can be restored exactly from a folder copy. Nothing irreversible exists in v1 (no network, no external tools), so a rollback loses work but never correctness.

## How
Recovery first kills the command's cgroup (it may still be running), then copies the checkpoint next to the workspace and swaps the two with one atomic `renameat2(RENAME_EXCHANGE)`. The verdict is logged only afterwards, so recovery can itself be killed and repeated.

## Evidence
Fault-injection cells `P2–P6 exec` and `P8 exec`: 0 violations, 0 duplicated effects (`docs/results/`).

## Look again if
A tool with an effect outside the workspace is added (the "Bonus" after v0.1). That effect cannot be rolled back; it needs idempotency keys or a human.

*Note, 2026-11-13: that tool now exists (`ext.call`, the bonus). Its rule is ADR 0012. This ADR still holds for `exec`.*
