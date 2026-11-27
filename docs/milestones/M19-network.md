# M19 — A network allow-list for commands

| | |
|---|---|
| Weeks in the plan | v3 (D-153); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` waits for a CI run |
| Commits | the experiments `41c7125` and `e84a002`, then the M19 commit (tag `m19` after CI is green, D-113) |
| Status | Done. Verified on both presets and Debian 12 by CI run 45816245961 on `f2a662d`; tagged `m19` there (D-166) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Until now a sandboxed command could not connect anywhere. That is safe, and it makes norn useless for the most common first step of real work: installing dependencies. M19 lets a policy list hosts (`[network] allow`) that commands may reach, through a proxy that lives in the runtime, outside the sandbox. Everything not on the list is refused, and every connection is written to the event log. Without the section nothing changes. It touches the policy (a new section), the sandbox (one socket handed out), the gateway (variables, one event) and the event log (a new event kind).

## 2. Requirements

From `docs/v3-scope.md`, M19, and §7.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| N1 `[network] allow`; absent or empty means no network | Yes, `nix` | `cargo test --lib policy` (4 network tests); `tests/network.rs`: `without_a_network_section_a_command_reaches_nothing`, `a_bad_network_entry_stops_the_run_before_it_starts` |
| N2 a proxy in the runtime: `CONNECT` and plain HTTP for allowed hosts, its own DNS, everything else refused; commands get the proxy variables | Yes, `nix` | `cargo test --lib sandbox::proxy` (8); `tests/network.rs`: `a_listed_host_is_reached_through_the_proxy_and_logged`, `a_tunnel_to_a_listed_host_carries_bytes_both_ways` |
| N3 each connection in the log (host, port, bytes, allowed); `norn trace` shows it | Yes, `nix` | `a_listed_host_is_reached_through_the_proxy_and_logged` checks the `exec.network` event, its place before `tool.completed`, and the trace lines |
| N4 recovery: a command that used the network is rolled back like any; what it sent is not | Yes, `nix` (documented and tested) | `docs/recovery-semantics.md` §11; kill cells with `NORN_RECOVERY_NETWORK=1`: 72 runs, 72 killed, 0 violations, 0 duplicated effects |
| With the allow-list off, every existing test passes unchanged | Yes, `nix` | `make test`: 207 in the main pass (16 of them new), 33 security cases through MCP, and the network kill cells; 0 failed |
| Both presets, Debian 12 | Yes | CI run 45816245961 on `f2a662d` (D-166): `make test` 241 passed on Ubuntu 24.04 and Debian 12; network kill cells on Ubuntu, 200 per cell: 4,800 runs, 0 violations |
| `pip install` etc. through the allow-list from a real registry | Shown in E1 only, with a Python proxy | The trial in M22 (D2) does it through norn |

## 3. What you need to know first

**Network namespace.** A private copy of the kernel's networking for a group of processes: its own interfaces, addresses and ports. A new one holds only a loopback interface (`lo`, the address `127.0.0.1`) and no route anywhere. Since M2 every command runs in a new one, so "no network" was never a rule to enforce but a fact of where the command lives.

**Proxy, `CONNECT`.** A program that makes connections on a client's behalf. An HTTPS client asks it `CONNECT pypi.org:443`; if the proxy agrees, it opens the connection, answers `200`, and from then on copies bytes both ways without understanding them (they are encrypted). It sees the host name and how many bytes pass, nothing else. Plain HTTP goes through a proxy as `GET http://host/path`.

**Socket and file descriptor passing.** A listening socket is the kernel object a server `accept()`s connections on. A socket belongs to the network namespace it was made in, for its whole life, whoever holds it. A process can give an open socket to another one through a Unix socket (`SCM_RIGHTS`). M19 relies on both facts.

**Resolving a name.** Turning `pypi.org` into addresses (DNS). The sandbox has no DNS server to ask; the proxy resolves names itself, on the host.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m19-connect-proxy` (E1) | pip, npm, cargo, git through a 70-line logging proxy | All four used `HTTPS_PROXY` alone; each needed only its registry's hosts; a missing host failed closed | The plan's proxy shape is enough; the list of hosts per ecosystem (D-154) |
| `m19-forwarder` (E2) | Reaching an outside proxy from an empty network namespace: a forwarder process, or handing the listening socket out | Both carried 64 parallel 4 MiB downloads intact | No process needs to run inside the sandbox (D-155) |
| `m19-landlock-net` (E3) | Landlock's TCP port rules as a second wall | They work, but match ports only, not addresses, and Debian 12 lacks them | Not used (D-156) |

## 5. What was built

| File | Purpose |
|---|---|
| `src/policy.rs` | `[network] allow`, parsed into `HostRule`s; `Policy::host_rules`; the proxy variables reserved when the section is on |
| `src/sandbox/proxy.rs` (new) | The proxy: receives the sandbox's socket, accepts, checks, resolves, connects, copies, counts; `Report` of connections |
| `src/sandbox/ns.rs` | `SandboxSpec::proxy`; the sandbox's init process makes the socket and sends it (`hand_out_proxy_socket`) |
| `src/sandbox/mod.rs` | The socket pair on file descriptor 4; proxy started before the command, finished after it; notes in stderr |
| `src/gateway.rs` | The six proxy variables; the `exec.network` event |
| `src/log.rs` | The event, and its place in the call state machine |
| `src/trace.rs`, `src/runtime.rs` | `norn trace` lines; `norn check` says which hosts a policy opens |
| `tests/network.rs` (new), `tests/recovery/main.rs`, `Makefile` | Tests; the kill cells again with the network on |

How the pieces meet, for one `exec` with hosts listed:

```text
runtime                                   sandbox (own network namespace)
───────                                   ───────────────────────────────
socketpair ── fd 4 ─────────────────────▶ init: lo up; listen 127.0.0.1:3128;
                                                send the socket over fd 4; close
proxy thread ◀── the listening socket ───┘
   accept() ◀──── connect 127.0.0.1:3128 ── command (HTTPS_PROXY=...)
   check host:port against network.allow
   resolve, connect from the host's network ─────▶ pypi.org:443
   copy bytes both ways, count
command ends → close what is open → Report → log `exec.network` → checkpoint → completed
```

## 6. The code, explained

**`HostRule::parse` and `matches` (`src/policy.rs`).** An entry is split at its last `:` into host and port (443 if none). An address parses as `HostPattern::Ip`; `*.example.com` becomes `Suffix(".example.com")`; anything else must be a valid DNS name. `matches` compares the port first, then refuses to compare a name with an address in either direction: a rule for `pypi.org` must never match a request for whatever address a command typed. A suffix needs at least one more label, so `*.example.com` does not match `example.com`.

**`hand_out_proxy_socket` (`src/sandbox/ns.rs`).** Runs in the sandbox's init process (PID 1 inside), after `lo` is up and before privileges are dropped. `TcpListener::bind(("127.0.0.1", 3128))` makes the socket *in the sandbox's namespace*, because that is where this process lives. `sendmsg` with `ControlMessage::ScmRights` sends it over file descriptor 4. When the function returns, the listener is dropped, which closes this process's copy; the runtime's copy stays open. The command is started later and inherits neither (its parent closes everything above fd 3 before `execve`).

**The descriptors (`src/sandbox/mod.rs`, `run`).** Between `fork` and `exec` only a few system calls are safe. The child copies the status pipe and the socket end above 10 first (`F_DUPFD_CLOEXEC`), then `dup2`s them onto 3 and 4. Copying first matters: if the socket end happened to be number 3, putting the pipe on 3 would close it before it was copied.

**`Proxy` (`src/sandbox/proxy.rs`).** `start` spawns one thread that waits for the socket (`recvmsg`); if the sandbox fails before sending it, the other ends close and the thread ends. Then `serve` accepts in a loop and starts one thread per connection, at most 128 at a time. `handle` reads the head (at most 16 KiB, 10 s), parses `CONNECT host:port` or `METHOD http://host[:port]/path`, and:

- no rule matches → `403`, a record with `allowed: false`;
- the name resolves only to local addresses (`is_local`) → `403`; this stops an allowed name from being pointed at the runtime's own machine;
- otherwise connect (10 s), answer `200` for a tunnel or send the head with the URL made into a path, and `pump` both ways on two threads, counting bytes.

`finish` sets a stop flag, `shutdown`s the listening socket (on Linux that wakes a thread blocked in `accept`), `shutdown`s every open connection, joins the threads and returns the `Report`. It is called after the command's processes are all gone, so no new client can arrive.

**Why the record is written after the command (`src/gateway.rs`).** A connection's byte counts exist only when it ends, and tools keep connections open until they exit (E1). So the gateway appends one `exec.network` event with the whole list after `sandbox::run` returns and before the checkpoint. The call state machine in `src/log.rs` lists which event may follow which; the new event is allowed only while the call's intent is open.

**Rust features met here.** `std::os::fd::OwnedFd`: a file descriptor that closes itself when dropped, so "who closes this" is answered by ownership. `Arc<Mutex<…>>` and atomics (`AtomicBool`) for state shared between the proxy's threads. `JoinHandle::is_finished` to forget finished threads without waiting for them.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-154 | A `CONNECT` proxy with a host list is enough; the hosts per ecosystem | Allowing by address range |
| D-155 | The sandbox hands its listening socket to the runtime; no forwarder | A forwarder and a mounted Unix socket; slirp4netns; `setns` |
| D-156 | No Landlock network rules | "Port 3128 only"; using them where present |
| D-160 | How the proxy is built: entry syntax, local addresses refused, one event per call, limits, variables, `nix` socket feature | Private ranges refused; one event per connection; raw `libc` for `SCM_RIGHTS` |

## 8. Problems hit

- **Two sessions in one folder** (Findings, 2026-11-26): another session committed this milestone's first policy code inside its own commit `23dba65`, and numbered an entry D-154 twice. Renumbered to D-158 in `e84a002`.
- **urllib and `NO_PROXY`** (Findings): a test client given the proxy explicitly still went direct, because `NO_PROXY` names `127.0.0.1`. The test was wrong, not the proxy.
- **The state machine** (Findings): the new event would have been refused by the call state machine in `src/log.rs`; found by reading before the first run.

## 9. How to run it

```sh
nix develop -c make test                   # includes tests/network.rs and the network kill cells
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test network
```

By hand, in a copy of a preset policy, allow a command and a host, then look at the trace:

```toml
[exec]
allow = [["python3", "-m", "pip"]]
[network]
allow = ["pypi.org", "files.pythonhosted.org"]
```

```sh
norn check --policy my.toml --repo .       # "network: commands may connect to pypi.org, files.pythonhosted.org ..."
norn run --repo . --policy my.toml --script steps.toml
norn trace <id>                            # "network: CONNECT pypi.org:443 2184 B up, 16884 B down"
```

## 10. What this milestone does not do

- No content inspection or TLS interception (D-153): what a command sends to an allowed host is not seen.
- A command cut off by a crash leaves no `exec.network` record (`docs/recovery-semantics.md` §11).
- No UDP, no DNS inside the sandbox, no inbound connections, no IPv6 literals (v3 scope §5).
- Not yet verified on `fhs` and Debian 12 (CI). The security cases of D1 come in M22.

## 11. Check your understanding

1. Why can a command in the sandbox not simply connect to `pypi.org`, even with M19 on?
2. The runtime accepts connections on a socket "inside" the sandbox while it lives outside. What fact about sockets makes that work?
3. Why is the listening socket made by the sandbox's init process and not by the command or the runtime?
4. Why does `pypi.org` in the list not allow port 80? How would you allow it?
5. A wildcard `*.example.com` is listed. Name a risk this entry carries that `pypi.org` does not.
6. Why does the proxy refuse a listed name that resolves to `127.0.0.1`? What is SSRF, and what lives at `169.254.169.254`?
7. Why is one `exec.network` event written after the command ends, rather than one per connection as it happens? What does that cost after a crash?
8. Why was Landlock's network support not used as a second wall?
9. `NO_PROXY` is set to `localhost,127.0.0.1,::1`. What would break in a project's own tests without it?
10. A command downloads a package, then norn is killed before the checkpoint. What does recovery do with the workspace, and what can it not undo?
11. Why does `pre_exec` copy both descriptors above 10 before putting them on 3 and 4?
