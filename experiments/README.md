# Experiments

Small throwaway programs, each trying one mechanism alone before it is built
into AgentOS. Each folder has a `NOTES.md` saying what it tried and what it
showed. None of this code is used by the runtime.

| Folder | Milestone | What it tries |
|---|---|---|
| `m0-ramp/` | M0 | Rust ramp: read a TOML file, write rows to SQLite |
| `m2-unshare/` | M2 | What a process sees after `unshare -Urmpn --fork --mount-proc` |
| `m2-pids/` | M2 | A fork bomb under `pids.max` in a delegated cgroup |
| `m2-openat2/` | M2 | `openat2(RESOLVE_BENEATH)` against `..`, absolute paths and symlinks |
| `m2-landlock/` | M2 | The `landlock` crate: allow a few folders, refuse the rest |
| `m3-rename-kill/` | M3 | Direct write vs temp-file + rename under `kill -9` |
| `m3-sqlite-kill/` | M3 | SQLite inserts killed mid-transaction |
| `m15-mcp/` | M15 | What Claude Code does as an MCP client: protocol revision, tool names, refusals, tools switched off, a server that dies mid-call, how a session is stopped |
| `m19-connect-proxy/` | M19 | E1: pip, npm, cargo and git through a logging `CONNECT` proxy with a host list |
| `m19-forwarder/` | M19 | E2: reaching a proxy from an empty network namespace; the sandbox's listening socket handed out with `SCM_RIGHTS` |
| `m19-landlock-net/` | M19 | E3: Landlock's TCP port rules (ABI 4+) as a second wall; not used |
| `m21-https/` | M21 | E5: `ureq`'s certificate checks, and trusting one test CA |
