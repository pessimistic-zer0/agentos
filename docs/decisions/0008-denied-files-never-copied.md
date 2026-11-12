# ADR 0008 — Denied files are never copied into the workspace

**Status:** accepted · **Log entries:** D-008

## Question
The policy can deny files (`.env`, `secrets/**`). Checking that in `fs.read` is easy. But an allowed command (`pytest`) runs the repo's own code, which can open any file in the folder. How do deny rules bind that code?

## Decision
When the workspace is created, paths matching a deny glob are not copied. A denied file does not exist inside the sandbox. The file tools still check deny rules, for paths created later.

## Why
A rule that only the gateway checks is not a rule for code the gateway does not see. Landlock cannot carve one file out of an allowed folder. A file that is not there cannot be read by anything.

## Cost
A task that legitimately needs a denied file cannot be done; the policy must be changed. A denied path that a command creates later is removed again by the post-`exec` revert (deny rules win over write rules).

## Evidence
`conftest_cannot_read_a_denied_file`, `exec_cannot_read_a_denied_file`, `denied_paths_cannot_be_created_or_read_later`; the canary string is searched for in the whole execution folder.
