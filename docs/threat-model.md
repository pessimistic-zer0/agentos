# Threat model

What Norn v0.1 protects, against whom, how, with which test, and where it stops. Plain words first; the Linux term is given next to it. Terms are explained in the glossary of `decision-log.md`.

## 1. Who is trusted

| Trusted | Not trusted |
|---|---|
| The host kernel | The agent (scripted or LLM) and every request it makes |
| The `norn` binary and the user who runs it | Everything in the target repository (it may contain text that steers the model, or code that runs under `pytest`) |
| The policy file | Every process started by an allowed command, and its output |
| The programs in the read-only system folders, as files (not what they do with hostile input) | |

The agent may be hostile from the start or become so by reading a file ("prompt injection"). Norn does not try to tell the two apart, and does not try to stop the model from being talked into things. It limits what any request can do.

**The allowlist is not the boundary.** `pytest` runs the repository's `conftest.py`; `cargo test` runs `build.rs`. Any allowed test command is arbitrary code execution. The argv allowlist limits what the agent *asks for*; the **sandbox** limits what the code *can do*. Every test below that says "from code" assumes the attacker runs arbitrary code inside the sandbox.

## 2. What is protected

1. Host files outside the workspace, especially `~/.ssh`, `~/.config`, tokens.
2. Files in the target repository that the policy denies (for example `.env`).
3. The runtime's environment variables and credentials (including the LLM API key).
4. Network egress. (v0.1: none at all. With the bonus tool: requests to `127.0.0.1` services named in the policy; see §7. Since v3, with `[network] allow`: connections from commands to the hosts listed there, through the runtime's proxy; see §9.)
5. The event log, the checkpoints and the blob store (the agent must not rewrite its own history).
6. Host CPU, memory and process ids.
7. The original repository (never modified; the output is a patch).

## 3. Attacks in scope, the defence, and the test

All tests are in `tests/security/main.rs` unless another file is named. Results: `docs/results/security-nix.md` and `docs/results/security-fhs.md` (32 passed, 0 escapes, on each preset).

| # | Attack | Defence | Test |
|---|---|---|---|
| 1 | `../` path traversal through a file tool | `openat2(RESOLVE_BENEATH)`: the kernel refuses any step out of the workspace | `fs_read_dotdot_traversal_is_denied` |
| 2 | Absolute paths | same | `fs_read_absolute_path_is_denied` |
| 3 | Symlinks pointing outside (absolute, relative, to `/`) | same | `fs_read_through_symlinks_pointing_outside_is_denied` |
| 4 | `/proc/self/root/...` magic links | `RESOLVE_NO_MAGICLINKS` | `fs_read_proc_magic_link_is_denied` |
| 5 | Writing through a symlinked folder inside the write globs | The parent folder is opened beneath the workspace; policy is checked on the resolved path | `fs_write_through_symlinked_folder_is_denied` |
| 6 | Writing over a symlink to a host file | `O_NOFOLLOW`; `renameat` replaces the name, never the target | `fs_write_over_a_symlink_does_not_follow_it` |
| 7 | Symlink swapped between check and use (TOCTOU) | Check and open are one kernel call; the handle is reused for the read or write | `toctou_symlink_swap_during_writes_and_reads_never_escapes` (≈6,000 flips, 300 calls) |
| 8 | Reaching `../log.db` or `../checkpoints` with file tools | `RESOLVE_BENEATH` | `fs_tools_cannot_reach_log_or_checkpoints` |
| 9 | Reading a denied file from code | Denied files are never copied into the workspace (ADR 0008) | `exec_cannot_read_a_denied_file`, `conftest_cannot_read_a_denied_file` |
| 10 | Reading host files from code (`~`, `~/.ssh`, `/etc/passwd`, the original repo) | New root built from nothing: only `/work`, read-only system folders, 5 devices, own `/proc`, own `/tmp`; Landlock behind it | `exec_cannot_read_host_home_or_original_repo` |
| 11 | Reading or changing the log and checkpoints from code | They are outside `/work` and never mounted | `exec_cannot_read_or_change_the_event_log_and_checkpoints` |
| 12 | Secrets in environment variables or `/proc/*/environ` | Environment cleared (PATH, HOME, LANG only); PID namespace with its own `/proc` | `exec_sees_no_host_secrets_in_environment_or_proc` |
| 13 | Writing outside the write globs from code | Compare with the checkpoint after each `exec`, revert, log `policy.violation` (ADR 0009) | `exec_changes_outside_write_globs_are_reverted` |
| 14 | Planting a symlink so the revert writes outside | Revert works through handles opened with `RESOLVE_NO_SYMLINKS` | `exec_cannot_replace_a_folder_with_a_symlink_to_escape_the_revert` |
| 15 | Network: TCP, UDP, IPv6, DNS, the host's loopback | Empty network namespace (own loopback only) | `exec_has_no_network` |
| 16 | Host abstract unix sockets | Network namespace | `exec_cannot_reach_host_abstract_unix_sockets` |
| 17 | Fork bomb | cgroup `pids.max` | `fork_bomb_is_stopped_by_pids_max` |
| 18 | Memory exhaustion | cgroup `memory.max`, swap off for the group | `memory_exhaustion_is_stopped_by_memory_max` |
| 19 | Infinite loop | Watchdog kills the cgroup at `call_timeout_s`; `cpu.max` | `infinite_loop_is_killed_at_the_time_limit` |
| 20 | A daemon that outlives the command and writes later | PID namespace (PID 1 exit kills all) and cgroup destroyed after each call | `background_processes_die_with_the_command` |
| 21 | Gaining privileges (`setuid(0)`, capabilities) | No capabilities in any set; `no_new_privs`; uid mapped to itself | `exec_has_no_capabilities_and_cannot_gain_root` |
| 22 | Re-arranging mounts, also from a nested user namespace | Landlock forbids mounting for restricted processes | `exec_cannot_mount_even_in_a_nested_user_namespace` |
| 23 | Signalling host processes | PID namespace | `exec_cannot_signal_host_processes` |
| 24 | Leaving files in the host's `/tmp` | Private tmpfs at `/tmp` | `exec_tmp_is_private` |
| 25 | Leaking a host file through the output patch (symlink in the workspace) | `diff --no-dereference`; one-sided links listed as comments | `patch_does_not_follow_symlinks_to_host_files` |
| 26 | Hanging the runtime with a FIFO | `fs.read` opens non-blocking and reads regular files only | `fs_read_of_a_fifo_planted_by_a_command_does_not_hang_the_runtime` |
| 27 | Hard links to system files; device files | Separate mounts; no `CAP_MKNOD`; `nodev` | `exec_cannot_hardlink_host_files_or_create_devices` |
| 28 | `/proc/1/root`, `chroot` | Own PID namespace; no capability | `exec_cannot_escape_through_proc_root_links` |
| 29 | Shells and interpreters under the shipped policy (`sh -c`, `python3 -c`, `env pytest`, path tricks) | Default deny; argv prefix match on exact words; `execve`, no shell | `shipped_policy_refuses_shells_interpreters_and_path_tricks` |
| 30 | Creating or reading a denied path later | Deny rules checked on the requested and the resolved path; revert after `exec` | `denied_paths_cannot_be_created_or_read_later` |
| 31 | Output flood | Output read on threads and cut at `max_output_kb`; log stores hashes | `output_flood_is_truncated_and_does_not_fill_the_log` |
| 32 | Rewriting history (the log) | No tool reaches it (rows 8, 11); events are only ever inserted | covered by 8 and 11 |
| 33 | Reaching more of the kernel from inside: nested user or mount namespaces, `mount`, `bpf`, `ptrace`, `io_uring`, `perf_event_open`, key rings (added for v1.0, D-116) | A seccomp deny list installed after Landlock; these calls fail with `EPERM`, `clone3` with `ENOSYS` (so libc falls back to `clone`, whose namespace flags are checked). Passes on both presets (`fhs`: CI run 43844369387, D-119) | `seccomp_refuses_namespaces_mounts_and_kernel_interfaces` |
| 34 | Using `fs.edit` to reach or learn about a file the agent may not touch (added for v1.0, D-120) | The same path handling as `fs.write` (`openat2` beneath the workspace, no symlink followed, deny rules), and the file must be readable by policy as well as writable, because the answers ("not found", "found at lines 6 and 13") describe its content | `tests/edit.rs`: `an_edit_needs_write_and_read_access_and_stays_in_the_workspace` |
| 35 | Changing the original repository through a folder mounted from it (`filesystem.ro_dirs`, added for v1.0, D-122), or using its mount point to mount somewhere else | The mount is read-only (kernel flag), `umount` needs capabilities the command does not have and is refused by seccomp; a write by the file tools is denied whatever the write globs say; the revert puts a swapped mount point back, and the sandbox checks every folder on the way to it before mounting | `tests/rodirs.rs`: `commands_use_the_folder_and_cannot_change_it`, `the_file_tools_read_it_from_the_repository_and_never_write_it`, `a_command_that_swaps_the_way_to_the_mount_point_is_reverted`, `the_sandbox_refuses_a_mount_point_that_is_a_symlink` |
| 36 | Using `fs.delete` to remove something outside the workspace, through a symlink, or a whole folder (added for v2, D-147) | The folder is opened beneath the workspace like `fs.write`'s; `unlinkat` removes the name and never follows it; a symlink, a folder or a missing file is refused before the intent; write globs and deny rules apply | `tests/agent_tools.rs`: `fs_delete_removes_a_file_and_the_patch_says_so`, `fs_delete_never_follows_a_symlink` |
| 37 | Using `fs.search` to read what the agent may not read (added for v2, D-147) | The walk starts from a folder opened beneath the workspace and opens every folder and file with `O_NOFOLLOW` through the folder above; each file must pass the read rules and no deny rule; caches and non-text files are skipped | `tests/agent_tools.rs`: `fs_search_finds_lines_in_readable_files_only_and_skips_caches` |
| 38 | Attacks sent through the MCP front door (`norn mcp`, added for v2, D-139), including hostile protocol messages | The same gateway: every `tools/call` becomes one `ToolCall`. Messages are parsed with a size limit and the parser's depth limit; nothing reaches the gateway before a valid call. The runtime's environment, which over MCP is the client's (and may hold an API key), is not passed to commands | The security suite a second time with `NORN_SECURITY_FRONT=mcp` (31 of 33 tests through MCP, D-144); `tests/mcp.rs`: `hostile_messages_never_crash_the_server_or_reach_the_gateway` |
| — | The API key ending up on disk | Sent as a header only; excluded from serialization | `tests/llm_agent.rs`: `llm_loop_fixes_the_demo_and_logs_the_conversation` |
| — | Refusing ordinary work (false denials) | — | `tests/benign.rs`: 23 operations, 0 false denials |

The second line of defence for rows 10, 11, 22, 27: Landlock allows only `/work`, `/tmp`, the read-only system folders, five device files and `/proc` (read-only). It would still hold if the mount setup had a bug that left a host folder visible.

## 4. Out of scope

These are not defended against. They are limits, not oversights.

- **Kernel exploits.** The sandbox is kernel features; a kernel bug that breaks namespaces or Landlock breaks Norn. Since D-116 a seccomp filter refuses about 45 system calls (row 33), among them new namespaces, `bpf`, `io_uring` and `ptrace`. It is a deny list: every other call an unprivileged process may make is still allowed, which is more kernel surface than the allow list of a hardened container runtime.
- **A malicious host user or root.** Anyone who can run commands as the user can read the log and edit the workspace.
- **Side channels.** Timing, cache, CPU frequency.
- **A malicious or leaky LLM provider.** With a hosted API, everything the agent reads is sent to it. Denied files are never in the workspace, so they cannot be sent; anything else can.
- **Several mutually hostile agents on one host.** v1 runs one agent per execution and makes no claim about two executions attacking each other.
- **Filling the disk, in part.** Since D-117 `limits.workspace_mb` bounds the workspace: one file can never be larger (`RLIMIT_FSIZE`, enforced by the kernel), and the whole workspace is measured every 50 ms while a command runs and once when it ends; a command over the limit is killed and everything it changed is undone (`tests/hardening.rs`; both presets, D-121). It is not a hard cap (ADR 0010): between two measurements a command can write about its write speed times the interval past the limit (20 to 40 MB in the experiment `m11-disk-limit`), so a nearly full disk can still be filled. And the limit is for the workspace, not the execution: every mutating call adds a checkpoint that copies the workspace, so an execution can take up to about `workspace_mb` × (`max_tool_calls` + 1). Sandbox `/tmp` is bounded by the memory limit.
- **Power loss.** The tests kill the process; they do not cut power. Checkpoint contents are not fsynced. No power-loss claim is made (D-022).
- **What `norn apply` copies.** It is the user's command, and it puts the agent's result into the user's repository: changed code, new executable files, symlinks (to anywhere), changed test hooks and CI files. It lists every change and refuses a repository that differs from the copy, and it never writes through a symlink or outside the target folder (`tests/apply.rs`). It does not judge the content. Read the patch, or the `--dry-run` list, first (D-108).
- **What happens after the patch is applied.** A patch can change test hooks, CI files or build scripts that later run unsandboxed on the human's machine. Containment ends when the patch leaves Norn. Read the patch.
- **The quality of the model's work.** A wrong but harmless patch is not a security failure.

## 5. Known weaknesses inside the scope

Stated so nobody has to find them.

- **Information about the host is visible inside the sandbox.** `/proc/self/mountinfo` shows the workspace's path on the host (which usually contains the user name) and the disk device; `uname` shows the host name and kernel version; `/proc/cpuinfo` and `/proc/meminfo` show the hardware; `/proc/self/cgroup` shows the cgroup path. These are names and facts, not file contents, and without a network they cannot be sent anywhere except into the patch or the model's conversation. There is no UTS or cgroup namespace because the v1 design lists only user, mount, PID and network.
- **`skip`, `scratch` and `deny_except` are the policy author's word.** A path in `scratch` may be changed by any command and is left out of the patch and of the violation report, and of checkpoints (a rollback empties it), so it is the wrong place for anything that matters; a later allowed command will see what an earlier one left there. A name in `deny_except` is copied and readable even if it holds a secret. `skip` only shapes the copy and protects nothing (D-099, D-100).
- **A folder in `filesystem.ro_dirs` is the live original, not a copy.** Commands and the agent's `fs.read` see the repository's folder as it is at that moment: a change someone makes to it during a run is seen at once, and it is not in any checkpoint, so a rollback does not bring an older state of it back. Everything in it is readable by the code the agent runs; the run refuses to start if a path in it matches a deny rule, but a secret under a name no rule matches is visible, as it would be in a copy. Symlinks in it that point outside it lead nowhere for the file tools and, for commands, to whatever the sandbox has at that path (D-122).
- **Whatever a policy mounts or sets is trusted.** A folder added to `sandbox.ro_mounts` (a virtualenv, say) can be read by any code the agent runs; do not mount a folder that holds secrets. It cannot be written (`tests/deps.rs`). Variables in `[sandbox.env]` are fixed text from the policy; a variable like `LD_PRELOAD` or `PYTHONSTARTUP` changes what runs, so it is the policy author's responsibility. The agent can change neither (D-091, D-092).
- **The read-only system folders are fully readable.** On NixOS that is all of `/nix/store`: every installed package, and anything a user ever put into the store. Do not keep secrets in the Nix store. On other systems it is `/usr`, `/bin`, `/lib`, `/lib64`.
- **Mounts below a read-only folder keep their own flags.** The read-only remount is not recursive. Landlock still limits them to read access.
- **A command can make its own call fail.** If it leaves files the runtime cannot scan or copy (a name that is not UTF-8, no permission bits), the call ends as FAILED and everything it changed is undone (D-085; `tests/hardening.rs`). Before 2026-11-14 this stopped the run, and one form made recovery impossible (B-009, B-010 in `bugs.md`). These tests pass on both presets (`fhs`: CI run 43573592353, D-087).
- **The post-`exec` revert walks the whole workspace** and compares file contents. A command can make that slow by creating many or large files. It is bounded by the disk, not by a limit.
- **Checkpoints are never pruned**, and each mutating call makes one. Paths in `filesystem.scratch` are left out of them (D-118), so a build folder is not stored again at every change; everything else is.
- **An interpreter in the allowlist means arbitrary code.** The shipped policies allow `pytest` and `python3 -m pytest`, which grant the same thing: the repository's own code runs, inside the sandbox. A policy that allows `["python3"]` or `["sh"]` lets the agent run any code directly. That is still confined by the sandbox, but the allowlist then limits nothing.
- **Three systems were tested, no others.** (The third: Debian 12, kernel 6.1, in a virtual machine on the development machine, one run of `make test`, 167 passed; `docs/results/debian-12.md`. In CI since run 43903121563, job `debian`, D-129.) The `nix` preset on NixOS 26.05, kernel 7.1.8, and the `fhs` preset on Ubuntu 24.04 in CI (<https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105>, 2026-11-13; `docs/results/security-fhs.md`). Other distributions that use the `fhs` preset (Fedora, Arch) have not been run. The runner's kernel was first recorded in run 43903121563: 7.0 (D-129).

## 6. What is and is not claimed

Claimed, with tests: on the two tested systems, none of the 32 attacks above got out of the sandbox or past the file tools, and none of 23 ordinary operations was refused.

Not claimed: perfect isolation, safety against kernel bugs, safety on systems that were not tested, or that the list of attacks is complete. A passing suite shows the attacks someone thought of.

## 7. The bonus tool `ext.call` (added after v0.1)

Sections 1 to 6 describe v0.1, which had no way to reach anything outside the workspace. The bonus adds one: `ext.call` sends a text payload to a service named in the policy (D-073). This section says what that changes. The shipped policies do not allow it; everything here applies only when someone writes a policy with an `[external]` section. The tests named below pass on both presets: on NixOS on the development machine, and on Ubuntu 24.04 in CI (<https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321>).

**What is new in the trust picture**

| Trusted | Not trusted |
|---|---|
| The service's promise to apply an idempotency key at most once (Norn cannot check it) | The payload: the agent writes it |
| The policy's choice of services | The service's answer: it is shown to the agent like any tool output, and could carry text that steers a model |

**Attacks, defences, tests** (tests in `tests/external.rs` unless another file is named)

| # | Attack | Defence | Test |
|---|---|---|---|
| E1 | Calling a service the policy does not list, or any service under the shipped policy | No rule means deny; the agent supplies a name, never a URL | `shipped_policy_denies_ext_call`, `allowed_call_is_logged_keyed_and_applied_once` (the `nobody` step) |
| E2 | A policy that points at another machine (by mistake or by a hostile edit) | URLs must be `http://127.0.0.1:<port>`; anything else stops the run at startup, before an execution exists | `a_policy_naming_another_machine_is_refused_at_startup`; `policy::tests::external_urls_must_be_loopback_by_number` (11 refused forms, including `http://127.0.0.1:80@evil.example/`) |
| E3 | A local service that redirects the request somewhere else | Redirects are never followed; the result counts as "no answer" | `a_redirect_is_not_followed` |
| E4 | A proxy variable in the runtime's environment sending the request off the machine | The client is built with no proxy | none; by reading the HTTP library's source (D-076) |
| E5 | Code run by an allowed command talking to the service directly, skipping policy, log and key | Unchanged from v0.1: the sandbox has its own empty network | `sandboxed_code_cannot_reach_the_service` |
| E6 | Making an irreversible effect happen twice by crashing or timing out the runtime | Intent with the key before the request; retries and recovery reuse the same key; no answer leaves the call uncertain instead of guessed | Fault-injection cells P1, P2, P3, P7, P8, T1, T2, T3 in `tests/recovery/main.rs` (`docs/results/recovery-ext-200-nix.md`, `recovery-ext-200-fhs.md`) |
| E7 | A huge answer flooding the log or the agent | The answer body is cut at `limits.max_output_kb`; the log stores a hash | `long_answers_are_cut_at_the_limit` |

**What is not defended, and should be known before allowing `ext.call` in a policy**

- **It is a way out for data.** The payload is whatever the agent chooses, so anything the agent can read (every file in the workspace, every command output) can be sent to the listed service. Denied files are still never in the workspace. With v0.1's four tools the only way out was the patch; this is a second one. The loopback rule limits *where* it goes, not *what* goes.
- **It is a way in for text.** The service's answer becomes a tool result. A hostile service could use it for prompt injection. Only the scripted agent can call `ext.call` today (the LLM agent is not offered the tool), so this is a limit to remember, not an open hole.
- **There is no human approval.** A model that has been talked into it could trigger a real, irreversible effect with one call. v1 cut approval workflows. That is acceptable for a fake ledger and would not be for a real payment service.
- **The exactly-once result depends on the service.** If a service ignores the idempotency key, every retry is a second effect. Norn cannot detect that from outside.
- **`127.0.0.1` means "this machine", not "safe".** Any program listening on the named port receives the request. Which port to name is the policy author's decision.
- **Budget:** a call can hold the runtime for `attempts × timeout_ms` (6 seconds with the defaults).

## 8. The MCP front door (added for v2)

`norn mcp` lets an agent that norn did not write call the tools (D-139). Two things change:

- **norn governs only the calls that reach it.** A client that keeps its own shell or file tools can act outside norn entirely. With Claude Code, `--tools ""` and `--strict-mcp-config` left norn's tools as the only ones (`experiments/m15-mcp/NOTES.md`, E2); for any other client this has to be checked. This is the client's setup, not something norn can enforce.
- **The client chooses the environment.** The server inherits the client's environment, which may hold API keys. Commands get an environment built from the policy, not this one (row 38; the environment test runs through MCP too).

Everything else is as above: the tools, the policy, the sandbox, the log and recovery are the same for both front doors.

## 9. The network allow-list (added for v3)

Until v3 a command could not connect anywhere. With `[network] allow` it can reach the hosts listed there (M19, D-153). This is the largest change to this document since v0.1, so it is stated plainly: **a command that may reach a host may send that host anything it can read in the sandbox**, which is the workspace copy (without denied files) and the read-only folders. What it sends cannot be undone. Without `[network]` nothing changes: the sandbox's network is empty.

How the way out is built:

- The sandbox keeps its empty network namespace. Its init process makes one listening socket on `127.0.0.1:3128` there and hands the socket itself to the runtime (D-155). The runtime's proxy accepts on it. No other address leads anywhere, and no program besides the command runs in the sandbox.
- The proxy reads only the request head: `CONNECT host:port` (HTTPS: the proxy sees the name and the byte counts, never the content; there is no TLS interception, D-153) or a plain-HTTP request with a full URL. A `host:port` that no `network.allow` entry matches is refused with `403` (design rule 2). A name without a port matches port 443 only.
- The proxy resolves names itself (there is no DNS in the sandbox) and refuses a name that resolves only to an address of this machine or its link (loopback, link-local such as a cloud machine's metadata service at `169.254.169.254`, unspecified, multicast). An entry written as an address (`127.0.0.1:8080`) is the only way to reach such an address, and it must be written on purpose.
- Every connection is in the log, allowed or not: event `exec.network`, with host, port, method, rule, and bytes each way (N3). The agent sees refusals in the command's stderr.

| # | Attack | Defence | Test |
|---|---|---|---|
| N1 | Connect directly, without the proxy, to an allowed host or anywhere else | Empty network namespace; the proxy socket is the only listener that leads out | `a_listed_host_is_reached_through_the_proxy_and_logged` (`tests/network.rs`) |
| N2 | Ask the proxy for a host or port not on the list | `403`, logged as refused | same; `hosts_not_listed_are_refused_and_recorded` (`src/sandbox/proxy.rs`) |
| N3 | An allowed name that resolves to the runtime's own machine | Local addresses refused for names | `an_allowed_name_that_resolves_to_this_machine_is_refused` |
| N4 | Malformed or tricky requests (no `http://`, `https://` without a tunnel, a user name in the URL, IPv6 literals) | `400`, nothing connected | `malformed_requests_get_400_and_no_record` |
| N5 | A command with `[network]` off | No proxy, no proxy variables | `without_a_network_section_a_command_reaches_nothing` |

The security suite of M22 adds more cases (raw IPs, UDP, a second proxy hop, both front doors).

What this does not stop: anything sent *to an allowed host* (a package registry that accepts uploads is a way out for data, if the command has credentials for it; norn gives commands none). A wildcard entry (`*.example.com`) allows every name under it, including names someone else controls. If the network namespace itself ever failed, the network would be open: Landlock's network rules are not used as a second wall, because they match TCP ports only (D-156).
