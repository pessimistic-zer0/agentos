# ADR 0009 — Write globs bind `exec` by compare-and-revert

**Status:** accepted · **Log entries:** D-009, D-048

## Question
The policy says where the agent may write (`src/**`, `tests/**`). Code run by a command can write anywhere in `/work`. How are write rules enforced on it?

## Decision
After every `exec`, compare the workspace with the last checkpoint. Any change outside the write globs is undone from the checkpoint and logged as a `policy.violation` event; the agent is told which paths were put back. The call still completes.

## Why
Globs do not map onto mounts, and making parts of the tree read-only breaks tools that write scratch files in unexpected places. A comparison works for any glob and uses the checkpoint that exists anyway.

## The dangerous part
The undo runs in the trusted runtime on a folder the attacker shaped. If `tests` was replaced by a symlink to the home folder, a naive "restore `tests/data.txt`" would write there. Every delete and restore therefore goes through a handle on the parent folder opened with `RESOLVE_BENEATH | RESOLVE_NO_SYMLINKS`, and files are compared by content, never by timestamp.

## Evidence
`exec_changes_outside_write_globs_are_reverted`, `exec_cannot_replace_a_folder_with_a_symlink_to_escape_the_revert`.

## Cost
A full tree walk and content comparison after every command. Fine for small repos; not measured on large ones.
