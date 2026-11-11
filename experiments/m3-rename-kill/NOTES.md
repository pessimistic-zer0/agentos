# m3-rename-kill

**Tried:** a writer replaces a 2 MB file again and again (all `A`s, then all
`B`s, …) and is killed with SIGKILL after a random 20–80 ms. Two ways of
writing: *direct* (open the target and write into it) and *atomic* (write a
temp file, fsync, rename over the target). After each kill the target is read
back. Run: `nix develop -c python3 experiments/m3-rename-kill/run.py 200`.

**Showed (2026-11-11, btrfs, kernel 7.1.8):**

```text
direct: 200 kills, 149 torn target files, 0 leftover temp files
atomic: 200 kills, 0 torn target files, 164 leftover temp files
```

**Taught:**

- Writing into the target directly leaves a half-written file about three
  times in four. With temp + rename the target is always the whole old file or
  the whole new file.
- The price: the temp file is left behind most of the time. Recovery must
  delete it, so the `fs.write` intent records the temp file's name.
- This says nothing about power loss: `kill -9` does not lose data the kernel
  already holds. The fsync is there for the power-loss case, which these tests
  cannot check (D-022).
