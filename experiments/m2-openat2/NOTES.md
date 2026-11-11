# m2-openat2

**Tried:** open eight paths relative to a workspace folder handle, once with
plain `openat` and once with `openat2(RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS)`.
Uses the `nix` crate's `openat2` (present in nix 0.31, so no raw syscall is
needed). Run: `nix develop -c cargo run` in this folder.

**Showed (2026-11-11, kernel 7.1.8):**

```text
path                         openat         openat2 BENEATH
src/ok.txt                   opened         opened
good-link                    opened         opened
src/../src/ok.txt            opened         opened
../secret.txt                opened         EXDEV (Cross-device link)
/etc/passwd                  opened         EXDEV (Cross-device link)
abs-link                     opened         EXDEV (Cross-device link)
rel-link                     opened         EXDEV (Cross-device link)
/proc/self/root/etc/passwd   opened         EXDEV (Cross-device link)
```

**Taught:**

- With `RESOLVE_BENEATH` the kernel refuses any path whose resolution would
  leave the starting folder, whether by `..`, an absolute path, a symlink
  (absolute or relative) or a `/proc` "magic link". The error is `EXDEV`.
- Paths that stay inside, including symlinks that stay inside and `..` that
  comes back, still work. So the agent loses nothing legitimate.
- The M1 text check (`tools/fs.rs::resolve`) would accept `abs-link` and
  `rel-link`, because as text they are plain names. Only the kernel knows
  where a symlink points at the moment of opening.
