# ADR 0001 — File tools run in the runtime, confined by `openat2`

**Status:** accepted · **Log entries:** D-015, D-047, D-064

## Question
`fs.read`, `fs.list` and `fs.write` could run in the trusted runtime process, or in a helper process inside the sandbox. Which?

## Decision
In the runtime. Every path is opened with `openat2(RESOLVE_BENEATH | RESOLVE_NO_MAGICLINKS)` from a handle on the workspace folder, and the opened handle is what gets read or written.

## Why
- The kernel refuses, in one call, any path that leaves the workspace (`..`, absolute paths, symlinks out, `/proc` magic links). There is no gap between check and use for an attacker to swap a symlink in.
- No second process, no protocol between runtime and helper.
- The policy can be checked on the path the kernel actually resolved.

## Cost
The runtime, which is trusted and unsandboxed, opens files in a folder the agent shaped. Everything it does there must go through handles, never through path strings. Two places needed extra care: the post-`exec` revert (`RESOLVE_NO_SYMLINKS`) and special files (a FIFO once hung `fs.read`; bug B-005).

## Evidence
Security suite group 1 (7 tests) and the TOCTOU test: about 6,000 symlink flips during 300 calls, 0 escapes (`docs/results/security-nix.md`).

## Look again if
A tool is added that must run programs or parse complex files on the agent's behalf. That belongs inside the sandbox.
