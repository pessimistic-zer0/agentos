# ADR 0006 — Rootless cgroups through systemd delegation

**Status:** accepted · **Log entries:** D-013, D-049

## Question
Memory, CPU and process limits need cgroups. Require root, or find a rootless way?

## Decision
Rootless. The runtime must be started inside a cgroup that systemd delegated to the user: `systemd-run --user --scope -p Delegate=yes agentos ...`. It moves the scope's processes into a leaf group, enables the memory, pids and cpu controllers, and makes one child group per `exec`.

## Why
A bug in a root process is a bug with root's power. A sandbox should not need the thing it protects against.

## Cost
- A systemd user session is required (true on most desktops and servers; needs setup in CI and containers).
- Users must remember the `systemd-run` prefix. Without it the runtime stops at startup and prints the command to use.
- Three bugs came from the details (concurrent setup, a half-working non-delegated scope): B-001, B-003 in `docs/bugs.md`.

## Look again if
AgentOS must run where there is no systemd (some containers). Then: accept running without resource limits, or use a setuid helper.
