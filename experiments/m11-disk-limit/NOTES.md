# m11-disk-limit

**Question (D-114, A2):** without root there is no hard cap on a folder's
size that survives the runtime being killed (ADR 0010: tmpfs vanishes;
quotas and loop-mounted images need root). So: can the runtime *measure*
the workspace often enough while a command runs, and how far past a limit
does a fast writer get before it is stopped?

**Tried (2026-11-16, NixOS, kernel 7.1.8, btrfs on NVMe):**
`cargo build --release`, then
`target/release/m11-disk-limit walk <dir>` (time a size walk: sum of
allocated blocks, `st_blocks`, symlinks not followed) and
`target/release/m11-disk-limit race <dir> 200 <ms> -- python3 -c <writer>`
(a writer as fast as Python can go; measured every `<ms>`; its process group
killed when over 200 MB). Separately, `prlimit --fsize` (the kernel's
per-file size limit, `RLIMIT_FSIZE`) on Python and `dd`.

**Showed:**

| Check | Result |
|---|---|
| Walk of `inventory-service` with its virtualenv (3,203 entries, 37 MB) | 5.4 to 6.2 ms |
| Walk of `wordfreq-rs` with `target/` (263 entries) / `shortlink-node` (94) | 0.6 ms / 0.3 to 0.6 ms |
| One big file, measured every 100 ms | stopped at 237 MB: 37 MB over |
| 1 MB files, every 100 ms | 239 MB: 39 MB over |
| One big file, every 20 ms | 223 MB: 23 MB over |
| 1 MB files, every 20 ms | 220 MB: 20 MB over |
| `RLIMIT_FSIZE` 10 MB, Python writing 20 MB | `OSError: [Errno 27] File too large`; file exactly 10 MB |
| The same, `dd` | killed by `SIGXFSZ`; file exactly 10 MB |

The writer reached about 2.3 GB/s (into the page cache).

**Taught:**

- Measuring is cheap: about 2 µs per entry. Walking every 50 ms costs a few
  percent of one core for a workspace of a few thousand files.
- The overshoot is about *write speed × interval*. It is a bound, not a
  cap: at 2.3 GB/s and 50 ms, up to roughly 115 MB past the limit before
  the kill. That must be said in the threat model as it is.
- `RLIMIT_FSIZE` is a real kernel cap, but for **one file**. Set to the
  workspace limit, it stops the single-file case exactly; many files are
  left to the measuring.
- A command killed for going over should be undone like any other call
  that cannot be recorded (D-085): otherwise the checkpoint after it would
  copy everything it wrote.
