# E5 (v4, M24): how long does the diff of one call take?

**Question.** `norn trace <id> --diff N` (C4) shows what call N changed, as a
diff between the checkpoint before it and the one after it, with the patch
code norn already has (`checkpoint::write_patch`, which runs GNU `diff -ruN`).
Is that fast enough to run on demand, on the largest workspace the trials
kept?

**How.** `src/main.rs` links the `norn` library and, for each pair of
neighbouring checkpoints of a kept execution, runs `write_patch` five times.
Two executions from `~/Temp/AgentOS_temp/` (outside this repository): the
largest kept workspace (29 MB, 2,779 files, a scripted run of 2026-11-14) and
the relay run of INV-104 (74 files, 11 checkpoints).

```sh
nix develop -c bash -c 'cd experiments/m24-call-diff && cargo build --release'
./target/release/m24-call-diff <execution folder> <out folder>
```

**What happened** (2026-11-28, one machine, run once: a rough look, not a
record):

| Execution | Pairs | Median per diff | Slowest | Patches |
|---|---|---|---|---|
| 29 MB, 2,779 files | 2 | 53.3 ms, 58.4 ms | 687.5 ms (the first, with a cold file cache) | 0 bytes: both calls were `pytest` runs that changed no file a patch shows |
| INV-104, 74 files | 10 | 3.3 to 6.5 ms | 15.7 ms | 448 to 2,249 bytes for the `fs.edit` calls; 0 bytes for the two `pytest` runs |

**What it taught.**

- On demand is fast enough: tens of milliseconds on the largest kept
  workspace, a few on a normal one. No cache of per-call diffs is needed.
- A changing call (every `exec` gets a checkpoint) often changes nothing a
  patch shows. The view must say "no change to the files" rather than print
  nothing.
- The two checkpoints sit in the same folder, which is what `write_patch`
  expects (it runs `diff` from the parent folder, so the paths in the patch
  are short), so it can be used as it is.
