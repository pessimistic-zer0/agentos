# ADR 0007 — Checkpoints are directory copies, not git commits

**Status:** accepted · **Log entries:** D-007

## Question
Save the workspace's state as git commits (in a separate `GIT_DIR`) or as full copies of the folder?

## Decision
Full copies with `cp -a --reflink=auto`, written under a temporary name and renamed into place.

## Why
A test on 2026-11-10 snapshotted a folder with git and restored it. Git had not captured ignored files, empty folders, most file modes, or a nested `.git`; after the restore the folder differed from the original in four ways. Recovery promises "exactly as it was", and git cannot keep that promise.

A folder copy captures everything, and on btrfs or XFS a reflink copy shares disk blocks, so it is fast and small.

## Cost
On filesystems without reflinks (ext4) each checkpoint is a real copy. Checkpoints are never pruned in v1.

## Look again if
Disk use becomes a problem. Options: prune old checkpoints, or overlayfs layers (rejected for v1 as more to learn for no benefit).
