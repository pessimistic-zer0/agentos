# ADR 0002 — A checkpoint after every mutating call

**Status:** accepted · **Log entries:** D-011, D-054, D-061

## Question
Take a checkpoint after every `fs.write` and `exec`, or only after `exec`?

## Decision
After every mutating call, whether or not it changed anything. The COMPLETED record names the checkpoint.

## Why
- One rule: between calls, the workspace equals the last checkpoint the log names. Recovery and the post-`exec` revert both rely on it.
- An uncertain `fs.write` can be settled as APPLIED only if a checkpoint can then be taken for it.

## Cost (measured, as the plan asked)
`fs.write` of 4 KB costs p50 13.7 ms / p99 23.8 ms, against 1.9 / 2.8 ms for the same durable write done directly (`docs/results/bench-nix/summary.md`). The checkpoint is one `cp -a --reflink=auto` process per call; on btrfs it shares blocks. On a filesystem without reflinks it is a full copy of the workspace per call.

## Look again if
Workspaces get large on ext4, or calls become frequent enough for 14 ms to matter. Options then: checkpoint only when the tree changed, or copy only changed files.
