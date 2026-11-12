# Architecture Decision Records

Short notes on the decisions that are hard to undo. Each one: the question,
the decision, the reason, what it costs, and what would make us look again.
The full reasoning, with the options that lost, is in `../decision-log.md`
(the entry numbers are given in each ADR).

| ADR | Question | Decision |
|---|---|---|
| [0001](0001-fs-tools-in-runtime.md) | File tools in the runtime or in a sandbox helper? | Runtime, confined by `openat2` |
| [0002](0002-checkpoint-every-mutating-call.md) | Checkpoint after every mutating call, or only after `exec`? | Every mutating call |
| [0003](0003-uncertain-exec-rolls-back.md) | Uncertain `exec`: roll back or ask a human? | Roll back and tell the agent |
| [0004](0004-hashes-in-log-contents-in-blobs.md) | File contents in the log, or only hashes? | Hashes in the log, contents in a blob store |
| [0005](0005-conversation-in-the-log.md) | How does the agent's context survive a crash? | Every message is an event |
| [0006](0006-rootless-cgroups.md) | Rootless cgroup delegation or require root? | Rootless, through systemd |
| [0007](0007-directory-checkpoints.md) | Git commits or directory copies for checkpoints? | Directory copies |
| [0008](0008-denied-files-never-copied.md) | How do deny globs bind code run by `exec`? | Denied files are never copied in |
| [0009](0009-write-globs-checked-after-exec.md) | How do write globs bind code run by `exec`? | Compare and revert after each `exec` |
| [0010](0010-workspace-on-disk.md) | Workspace on a size-capped tmpfs or on disk? | On disk; disk filling is a known limit |
| [0011](0011-system-folders-from-policy.md) | Which system folders does the sandbox see? | Listed in the policy; `fhs` and `nix` presets |
