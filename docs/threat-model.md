# Threat model

What AgentOS v0.1 protects, against whom, how, with which test, and where it stops. Plain words first; the Linux term is given next to it. Terms are explained in the glossary of `decision-log.md`.

## 1. Who is trusted

| Trusted | Not trusted |
|---|---|
| The host kernel | The agent (scripted or LLM) and every request it makes |
| The `agentos` binary and the user who runs it | Everything in the target repository (it may contain text that steers the model, or code that runs under `pytest`) |
| The policy file | Every process started by an allowed command, and its output |
| The programs in the read-only system folders, as files (not what they do with hostile input) | |

The agent may be hostile from the start or become so by reading a file ("prompt injection"). AgentOS does not try to tell the two apart, and does not try to stop the model from being talked into things. It limits what any request can do.

**The allowlist is not the boundary.** `pytest` runs the repository's `conftest.py`; `cargo test` runs `build.rs`. Any allowed test command is arbitrary code execution. The argv allowlist limits what the agent *asks for*; the **sandbox** limits what the code *can do*. Every test below that says "from code" assumes the attacker runs arbitrary code inside the sandbox.

## 2. What is protected

1. Host files outside the workspace, especially `~/.ssh`, `~/.config`, tokens.
2. Files in the target repository that the policy denies (for example `.env`).
3. The runtime's environment variables and credentials (including the LLM API key).
4. Network egress. (v0.1: none at all. With the bonus tool: requests to `127.0.0.1` services named in the policy; see §7.)
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
| — | The API key ending up on disk | Sent as a header only; excluded from serialization | `tests/llm_agent.rs`: `llm_loop_fixes_the_demo_and_logs_the_conversation` |
| — | Refusing ordinary work (false denials) | — | `tests/benign.rs`: 23 operations, 0 false denials |

The second line of defence for rows 10, 11, 22, 27: Landlock allows only `/work`, `/tmp`, the read-only system folders, five device files and `/proc` (read-only). It would still hold if the mount setup had a bug that left a host folder visible.

## 4. Out of scope

These are not defended against. They are limits, not oversights.

- **Kernel exploits.** The sandbox is kernel features; a kernel bug that breaks namespaces or Landlock breaks AgentOS. There is no seccomp filter (cut from v1), so sandboxed code can call any system call an unprivileged process may, including creating nested user namespaces. That is more kernel surface than a hardened container runtime exposes.
- **A malicious host user or root.** Anyone who can run commands as the user can read the log and edit the workspace.
- **Side channels.** Timing, cache, CPU frequency.
- **A malicious or leaky LLM provider.** With a hosted API, everything the agent reads is sent to it. Denied files are never in the workspace, so they cannot be sent; anything else can.
- **Several mutually hostile agents on one host.** v1 runs one agent per execution and makes no claim about two executions attacking each other.
- **Filling the disk.** The workspace is on disk with no size cap (ADR 0010). A command can write until the disk is full. Sandbox `/tmp` is bounded by the memory limit.
- **Power loss.** The tests kill the process; they do not cut power. Checkpoint contents are not fsynced. No power-loss claim is made (D-022).
- **What `agentos apply` copies.** It is the user's command, and it puts the agent's result into the user's repository: changed code, new executable files, symlinks (to anywhere), changed test hooks and CI files. It lists every change and refuses a repository that differs from the copy, and it never writes through a symlink or outside the target folder (`tests/apply.rs`). It does not judge the content. Read the patch, or the `--dry-run` list, first (D-108).
- **What happens after the patch is applied.** A patch can change test hooks, CI files or build scripts that later run unsandboxed on the human's machine. Containment ends when the patch leaves AgentOS. Read the patch.
- **The quality of the model's work.** A wrong but harmless patch is not a security failure.

## 5. Known weaknesses inside the scope

Stated so nobody has to find them.

- **Information about the host is visible inside the sandbox.** `/proc/self/mountinfo` shows the workspace's path on the host (which usually contains the user name) and the disk device; `uname` shows the host name and kernel version; `/proc/cpuinfo` and `/proc/meminfo` show the hardware; `/proc/self/cgroup` shows the cgroup path. These are names and facts, not file contents, and without a network they cannot be sent anywhere except into the patch or the model's conversation. There is no UTS or cgroup namespace because the v1 design lists only user, mount, PID and network.
- **`skip`, `scratch` and `deny_except` are the policy author's word.** A path in `scratch` may be changed by any command and is left out of the patch and of the violation report, so it is the wrong place for anything that matters; a later allowed command will see what an earlier one left there. A name in `deny_except` is copied and readable even if it holds a secret. `skip` only shapes the copy and protects nothing (D-099, D-100).
- **Whatever a policy mounts or sets is trusted.** A folder added to `sandbox.ro_mounts` (a virtualenv, say) can be read by any code the agent runs; do not mount a folder that holds secrets. It cannot be written (`tests/deps.rs`). Variables in `[sandbox.env]` are fixed text from the policy; a variable like `LD_PRELOAD` or `PYTHONSTARTUP` changes what runs, so it is the policy author's responsibility. The agent can change neither (D-091, D-092).
- **The read-only system folders are fully readable.** On NixOS that is all of `/nix/store`: every installed package, and anything a user ever put into the store. Do not keep secrets in the Nix store. On other systems it is `/usr`, `/bin`, `/lib`, `/lib64`.
- **Mounts below a read-only folder keep their own flags.** The read-only remount is not recursive. Landlock still limits them to read access.
- **A command can make its own call fail.** If it leaves files the runtime cannot scan or copy (a name that is not UTF-8, no permission bits), the call ends as FAILED and everything it changed is undone (D-085; `tests/hardening.rs`). Before 2026-11-14 this stopped the run, and one form made recovery impossible (B-009, B-010 in `bugs.md`). These tests pass on both presets (`fhs`: CI run 43573592353, D-087).
- **The post-`exec` revert walks the whole workspace** and compares file contents. A command can make that slow by creating many or large files. It is bounded by the disk, not by a limit.
- **Checkpoints are never pruned**, and each mutating call makes one.
- **An interpreter in the allowlist means arbitrary code.** The shipped policies allow `pytest` and `python3 -m pytest`, which grant the same thing: the repository's own code runs, inside the sandbox. A policy that allows `["python3"]` or `["sh"]` lets the agent run any code directly. That is still confined by the sandbox, but the allowlist then limits nothing.
- **Two systems were tested, no others.** The `nix` preset on NixOS 26.05, kernel 7.1.8, and the `fhs` preset on Ubuntu 24.04 in CI (<https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105>, 2026-11-13; `docs/results/security-fhs.md`). Other distributions that use the `fhs` preset (Debian, Fedora, Arch) have not been run. The CI log does not record the runner's kernel version.

## 6. What is and is not claimed

Claimed, with tests: on the two tested systems, none of the 32 attacks above got out of the sandbox or past the file tools, and none of 23 ordinary operations was refused.

Not claimed: perfect isolation, safety against kernel bugs, safety on systems that were not tested, or that the list of attacks is complete. A passing suite shows the attacks someone thought of.

## 7. The bonus tool `ext.call` (added after v0.1)

Sections 1 to 6 describe v0.1, which had no way to reach anything outside the workspace. The bonus adds one: `ext.call` sends a text payload to a service named in the policy (D-073). This section says what that changes. The shipped policies do not allow it; everything here applies only when someone writes a policy with an `[external]` section. The tests named below pass on both presets: on NixOS on the development machine, and on Ubuntu 24.04 in CI (<https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321>).

**What is new in the trust picture**

| Trusted | Not trusted |
|---|---|
| The service's promise to apply an idempotency key at most once (AgentOS cannot check it) | The payload: the agent writes it |
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
- **The exactly-once result depends on the service.** If a service ignores the idempotency key, every retry is a second effect. AgentOS cannot detect that from outside.
- **`127.0.0.1` means "this machine", not "safe".** Any program listening on the named port receives the request. Which port to name is the policy author's decision.
- **Budget:** a call can hold the runtime for `attempts × timeout_ms` (6 seconds with the defaults).

