# Changelog

What changed for someone who uses AgentOS, newest first. The reasons are in `docs/decision-log.md` (the `D-` numbers); the bugs in `docs/bugs.md` (the `B-` numbers).

## Unreleased (will be 1.0.0)

Not released yet. Open before the release: the project name (D-023), the trial with a hosted model (M13, blocked on an API key), and a CI run on Ubuntu for the newest changes.

### Added

- **`fs.edit`**: replace one piece of text in an existing file, instead of sending the whole file. The text must occur exactly once (D-120).
- **Binary files and the executable bit** through the file tools: `encoding = "base64"` on `fs.read` and `fs.write`, `executable = true/false` on `fs.write`, and `fs.list` shows which files are executable (D-123).
- **`filesystem.ro_dirs`**: a folder of the repository (`node_modules`) that is not copied; commands see the original read-only at its place (D-122).
- **`limits.workspace_mb`**: a command that makes the workspace larger than this is stopped and undone. A measured limit, not a hard cap (D-117).
- **A seccomp filter** for sandboxed commands: new namespaces, mounts, `bpf`, `ptrace`, `io_uring` and similar system calls are refused (D-116).
- **`agentos apply`**: copy a finished run's result into the repository, including what a patch cannot carry; refuses if the repository changed since (D-108).
- **`agentos check`** (what a run would do, without running) and **`agentos rm`** (delete executions) (D-103, D-104).
- **Policy lists** `skip`, `scratch`, `deny_except` (D-099, D-100), **`exec.helpers`** for programs that commands start themselves (D-112), **`[sandbox.env]`** and extra read-only mounts for packages outside the repository (D-091, D-092).
- **`fs.read` with a line range** (`start_line`, `max_lines`) for files larger than the output limit (D-102).
- **An AppArmor profile** for Ubuntu, so user namespaces need not be allowed for the whole system (D-093).
- **The bonus tool `ext.call`**, off by default: a call to a local service with an idempotency key, and its recovery rule (D-073 to D-081).
- **A compatibility promise** for policy files and execution folders (`docs/compatibility.md`, D-126).

### Changed

- `agentos run` and `check` start themselves in a delegated cgroup scope; the `systemd-run` prefix is no longer needed (D-105).
- `fs.write` makes missing folders (D-084).
- Paths listed as `scratch` are no longer stored in checkpoints; after a rollback a cache is empty (D-118).
- The patch lists what it cannot carry: binary files, modes, empty files and folders, symlinks (D-090, D-095).
- A run whose model stops while its last command still fails ends as `stopped`, not `completed` (D-107). A reply the model's server cannot use is told to the model, then ends the run as `stopped` with its patch (D-111). A rate-limited server is given the time it asks for (D-124).
- Stopping a run (Ctrl-C) prints how to go on (D-106). First messages name the fix first (D-089).

### Fixed

- B-009 to B-014: files a command leaves that the runtime cannot record no longer stop the run; a workspace with locked folders can be recovered; a relative `--state-dir` works; empty new files are reported; the startup check works when the code is used from another program; a false note about an inactive virtualenv on Ubuntu.
- A write that only changes a file's mode is classified correctly after a kill (found while building D-123, before any release had the feature).

### Compatibility

- Policy files and execution folders written by 0.1 work unchanged (`tests/compat.rs`).

## 0.1 (2026-11-12)

The first tagged version: the gateway, the policy, the sandbox (namespaces, cgroups, Landlock, `openat2`), the event log, checkpoints and recovery; four tools (`fs.read`, `fs.list`, `fs.write`, `exec`); a scripted agent and an LLM agent; the fault-injection, security and benchmark suites.
