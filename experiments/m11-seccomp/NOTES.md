# m11-seccomp

**Tried (2026-11-16, NixOS, kernel 7.1.8, x86_64):** a seccomp filter written
by hand as classic BPF with only the `libc` crate (no seccomp library), in the
shape planned for the sandbox: a **deny list**. Everything is allowed except
about 45 system calls that a test command has no use for and that widen the
kernel surface (new namespaces, mounts, `bpf`, `ptrace`, `keyctl`, `io_uring`,
kernel modules, clock setting, ...). Refused calls return `EPERM` ("not
permitted"), so a program sees an ordinary error instead of being killed.
`clone3` returns `ENOSYS` ("not implemented"): its flags live in memory, which
the filter cannot read, and libc then falls back to `clone`, whose flags it
can read; `clone` with any namespace flag returns `EPERM`. A wrong
architecture kills the process; the x32 ABI returns `ENOSYS`.

Build and run: `nix develop ../.. -c cargo build --offline`, then
`target/debug/m11-seccomp selftest` or `target/debug/m11-seccomp <program> ...`.

**Showed:**

| Check | Result |
|---|---|
| Filter size | 105 instructions |
| `unshare(CLONE_NEWUSER)`, `clone(CLONE_NEWUSER)` | `EPERM` |
| `clone3` | `ENOSYS` |
| `mount`, `bpf`, `io_uring_setup`, `keyctl`, `ptrace` | `EPERM` |
| `getpid`, `fork` + `waitpid` | work |
| `cargo test --offline` on `wordfreq-rs` (rustc, the `cc` linker) | builds and runs; the 1 seeded failure, as without the filter |
| `node --test` on `shortlink-node` (Node v24) | 6 tests, 4 pass, 2 fail: the same as without the filter |
| `pytest -q` on `inventory-service` (virtualenv) | `3 failed, 75 passed`: the same as without the filter |
| Python threads, `subprocess`, `multiprocessing.Pool` | work |
| `sh -c 'echo … \| cat'` | works |

The three projects were copies in the session's scratch folder, not the
originals in `~/Temp/AgentOS_temp/`.

**Taught:**

- A deny list fits this project better than an allow list. An allow list
  (Docker's approach) has to know every call that cargo, node, a linker and
  Python make; one missing entry breaks a real command in a way that is hard
  to see. The deny list breaks nothing the trials use, and still takes away
  the calls that matter most for kernel exposure: nested user namespaces
  (`threat-model.md` §4 names exactly that), `io_uring`, `bpf`, `ptrace`.
- `EPERM` instead of killing: programs already handle "not permitted" (libuv
  falls back when `io_uring` is refused). A killed process would look like a
  crash of the command.
- `clone3` must return `ENOSYS`, not `EPERM`: glibc only falls back to
  `clone` on `ENOSYS`.
- No new crate is needed. Writing the filter by hand is about 80 lines, and
  each instruction can be explained in the milestone document.
- Not tried here: inside the real sandbox (after Landlock, as PID 1 of its
  namespace), and on `aarch64`. The sandbox tests do that.
