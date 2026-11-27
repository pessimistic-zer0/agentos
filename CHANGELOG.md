# Changelog

What changed for someone who uses Norn, newest first. The reasons are in `docs/decision-log.md` (the `D-` numbers); the bugs in `docs/bugs.md` (the `B-` numbers).

## 1.2.0 (2026-11-26; the plan "v3": `docs/v3-scope.md`)

Tagged `v1.2` once a CI run on Ubuntu 24.04, Ubuntu with AppArmor and Debian 12 has passed (D-164). Everything new is off unless a policy turns it on; policy files and execution folders of 1.1 work unchanged (`docs/compatibility.md`).

### Added

- **`[network] allow`**: hosts that commands may reach, through a proxy in the runtime; nothing else is reachable, and every connection is logged as `exec.network` and shown by `norn trace` (M19, D-153 to D-155, D-160). Without the section, commands have no network, as before. `norn check` says which hosts a policy opens.
- **`[approvals] require`**, **`norn approve`**, **`norn deny`**: chosen calls (`exec:npm publish`, `fs.delete`, `fs.write:ci/**`, `ext.call:github`) wait for a person's yes instead of running. The agent is told "waiting for approval" with the command to run; the same call asked again runs once approved. A yes is used once and expires (60 minutes by default); a no is final for that call. `norn approve <execution>` lists what waits, with a diff for writes (M20, D-159, D-161).
- **`[secrets]`** and **`ext.call` to `https://` services**: a service may be `https://host[:port][/path]`, with `ca_file` for a private CA and `headers` that use secrets (`Authorization = "Bearer {secret:TOKEN}"`). Secrets come from the runtime's environment or a file; they never enter the sandbox, the log, the workspace or the patch, and any value that comes back is shown as `[secret:NAME]`. Remote services wait for approval unless `approval = false`. `norn check` reads the secrets and names a missing one (M21, D-157, D-162).

### Changed

- An `[[external.service]]` URL may now start with `https://` (D-162). Plain `http://` is still allowed only for `127.0.0.1` by number, as before.

### Fixed

- B-016: resuming a run that was stopped during its setup (before its first checkpoint) failed with a bare "No such file or directory"; it now says that setup was not finished and to start a new run.

## 1.1.0 (2026-11-26; the plan "v2": `docs/v2-scope.md`)

Tagged `v1.1` on the commit CI run 45728656310 passed on Ubuntu 24.04, Ubuntu with AppArmor, and Debian 12 (D-158). Policy files and execution folders of 1.0 work unchanged.

### Added

- **`norn mcp`**: norn as an MCP server over standard input and output, so that an agent such as Claude Code runs its file and command calls through norn's policy, sandbox and log (D-139 to D-143). Both MCP revisions are served (2026-07-28 and 2025-11-25). Tool names on the wire: `fs_read`, `fs_list`, `fs_write`, `fs_edit`, `exec`, `ext_call`.
- **`norn finish <id>`**: ends an execution nobody is driving (one made by `norn mcp`) and writes its patch (D-141).
- **`fs.delete`** (delete one file) and **`fs.search`** (find lines containing a piece of text in the files the policy lets the agent read) (D-147).
- **`replace_all`** for `fs.edit`: replace every occurrence (D-147).
- **A test baseline** (D-145): a command's result before the agent's first file change is remembered, and later runs say what is new and what is fixed. The LLM agent may finish in a project that was already red when fewer tests fail than before and none newly fail.

### Fixed

- B-015: a file call on a path through `..` could fail at random with "Try again" while something else on the machine renamed files; `openat2` is now retried as its manual says.

### Changed

- `norn status` shows an unfinished MCP execution as `open` (or `serving` while a server uses it), not `interrupted`.
- A refused call tells the agent what the policy does allow: the command prefixes, the write or read globs, the services (D-146). The event log records the rule as before.
- An LLM run whose conversation grows larger than the model's server accepts (HTTP 413) ends at once as `stopped`, with its patch (D-148).

## 1.0.0 (2026-11-25)

Tag `v1.0`, on the commit that CI run 45489487156 passed on Ubuntu 24.04, Ubuntu with AppArmor's restriction on, and Debian 12 (D-136).

### Renamed

- **The project is now called norn** (D-133). The program is `norn`, the settings are `NORN_LLM_BASE_URL`, `NORN_LLM_MODEL`, `NORN_LLM_API_KEY`, `NORN_STATE_DIR` and so on; the old `AGENTOS_…` names are still read when the new one is not set (D-134).
- The default state folder is `~/.local/share/norn`. Executions made before are in `~/.local/share/agentos`: move that folder (`mv ~/.local/share/agentos ~/.local/share/norn`, if the new one does not exist yet) or pass `--state-dir`.
- The AppArmor profile is `packaging/apparmor/norn`, for `/usr/local/bin/norn`. Remove an installed old one with `sudo apparmor_parser -R /etc/apparmor.d/agentos && sudo rm /etc/apparmor.d/agentos`.
- Unchanged on purpose: the idempotency key of `ext.call` keeps the form `agentos-<execution id>-c<call id>`, so that a call left uncertain before the rename is not applied twice (D-134).

### Added

- **`fs.edit`**: replace one piece of text in an existing file, instead of sending the whole file. The text must occur exactly once (D-120).
- **Binary files and the executable bit** through the file tools: `encoding = "base64"` on `fs.read` and `fs.write`, `executable = true/false` on `fs.write`, and `fs.list` shows which files are executable (D-123).
- **`filesystem.ro_dirs`**: a folder of the repository (`node_modules`) that is not copied; commands see the original read-only at its place (D-122).
- **`limits.workspace_mb`**: a command that makes the workspace larger than this is stopped and undone. A measured limit, not a hard cap (D-117).
- **A seccomp filter** for sandboxed commands: new namespaces, mounts, `bpf`, `ptrace`, `io_uring` and similar system calls are refused (D-116).
- **`norn apply`**: copy a finished run's result into the repository, including what a patch cannot carry; refuses if the repository changed since (D-108).
- **`norn check`** (what a run would do, without running) and **`norn rm`** (delete executions) (D-103, D-104).
- **Policy lists** `skip`, `scratch`, `deny_except` (D-099, D-100), **`exec.helpers`** for programs that commands start themselves (D-112), **`[sandbox.env]`** and extra read-only mounts for packages outside the repository (D-091, D-092).
- **`fs.read` with a line range** (`start_line`, `max_lines`) for files larger than the output limit (D-102).
- **An AppArmor profile** for Ubuntu, so user namespaces need not be allowed for the whole system (D-093).
- **The bonus tool `ext.call`**, off by default: a call to a local service with an idempotency key, and its recovery rule (D-073 to D-081).
- **A compatibility promise** for policy files and execution folders (`docs/compatibility.md`, D-126).

### Changed

- `norn run` and `check` start themselves in a delegated cgroup scope; the `systemd-run` prefix is no longer needed (D-105).
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

### Tested on

- NixOS 26.05 (kernel 7.1), Ubuntu 24.04 in CI (kernel 7.0), and Debian 12 (kernel 6.1) in a virtual machine, also in CI (`docs/results/debian-12.md`).

## 0.1 (2026-11-12)

The first tagged version: the gateway, the policy, the sandbox (namespaces, cgroups, Landlock, `openat2`), the event log, checkpoints and recovery; four tools (`fs.read`, `fs.list`, `fs.write`, `exec`); a scripted agent and an LLM agent; the fault-injection, security and benchmark suites.
