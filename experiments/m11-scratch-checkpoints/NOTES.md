# m11-scratch-checkpoints

**Question (D-114, A3):** how to make a checkpoint that does not hold a
scratch folder (a build cache), and what each way costs.

**Tried (2026-11-16, NixOS, btrfs on NVMe):** `bash run.sh <project> target`
on a copy of `wordfreq-rs` after a build: 14 MB, all of it `target/` (202
entries). Three ways, three runs each, with `cp --reflink=auto` (btrfs
shares blocks) and `--reflink=never` (stands in for ext4, the CI runner):
**full** (today), **delete** (copy, then delete `target/*` from the copy),
**stash** (move `target` out, copy, move it back).

**Showed:**

| | reflink=auto, ms | reflink=never, ms | one checkpoint holds |
|---|---|---|---|
| full | 17, 10, 9 | 155, 33, 33 | 14 MB |
| delete | 15, 21, 24 | 45, 45, 33 | 1 MB |
| stash | 10, 10, 13 | 8, 11, 9 | 1 MB |

**Taught:**

- Both ways fix the size. Stash is faster where blocks are not shared,
  because the cache is never copied.
- Stash changes the workspace while a checkpoint is made; a kill in between
  leaves the cache outside it. Delete only ever touches the temporary copy,
  which recovery already throws away. For a project whose point is recovery,
  delete was taken (D-118) and its cost written down.
- Three runs on one small project: enough to choose, not a benchmark.
