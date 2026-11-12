# m2-landlock

**Tried:** the `landlock` crate (0.4): handle all filesystem rights (ABI V5),
allow read+execute under the folders given on the command line, allow
everything under one work folder, `restrict_self()`, then run Python children
that try to read and write various places. Run:
`nix develop -c sh -c 'cargo build && PY=$(readlink -f $(which python3)) ./target/debug/m2-landlock /nix/store'`.

**Showed (2026-11-11, kernel 7.1.8):**

```text
landlock status: FullyEnforced, no_new_privs: true
  write in work: ok
  PermissionError: [Errno 13] Permission denied: '/etc/passwd'
  PermissionError: [Errno 13] Permission denied: '/home/<user>'
  PermissionError: [Errno 13] Permission denied: '/tmp/landlock-escape'
```

**Surprise:** the first version failed before running anything: spawning the
child returned `EACCES`. `strace` showed the failing call was
`openat("/dev/null")`: Rust's `Command::output()` opens `/dev/null` for the
child's stdin, and Landlock refused it. Fixed by allowing read+write on
`/dev/null` alone.

**Taught:**

- Landlock is "allow only": after `restrict_self`, every path not under a rule
  is refused, for this process and every child it starts. It cannot be undone.
- `restrict_self` turns on `no_new_privs` itself (the crate does it), because
  the kernel requires it for unprivileged use.
- The sandbox must list the few device files programs expect (`/dev/null`,
  `/dev/zero`, `/dev/urandom`, …) explicitly, or ordinary programs break in
  confusing ways.
- Landlock rules are checked when a file is *opened*; the error is `EACCES`,
  the same as a normal permission error.
