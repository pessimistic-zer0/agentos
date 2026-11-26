# E2 — Reaching the proxy from an empty network namespace

**Question (v3 scope §4, E2).** A sandboxed command lives in its own network
namespace (a private copy of the network: only `lo`, no route out). How does
it reach a proxy that runs outside, in the runtime? The plan said: a small
forwarder inside listens on `127.0.0.1:3128` and passes each connection to a
Unix socket mounted in from outside. Does that hold up under parallel
downloads? Is there something simpler?

**Two ways tried.**

| Mode | Inside the sandbox | Outside |
|---|---|---|
| `unix` (the plan) | a forwarder process: listens on `127.0.0.1:3128`; for each client, opens the Unix socket and copies bytes both ways | a proxy listening on the Unix socket |
| `fdpass` | makes the listening TCP socket on `127.0.0.1:3128`, sends **the socket itself** out through a Unix socket (`SCM_RIGHTS`), closes its own copy, then runs the command | receives the socket and calls `accept()` on it directly |

`fdpass` works because a socket belongs to the network namespace it was made
in, for its whole life, whoever holds it. The runtime, outside, holds a
listening socket that lives *inside* the sandbox's network: commands there
connect to `127.0.0.1:3128` and the runtime gets the connection. The runtime's
own outgoing connections are made in the host's network as usual. Passing a
file descriptor through a Unix socket (`SCM_RIGHTS`, "socket control message:
rights") is the standard way to hand an open file or socket to another
process.

**Run.** `./run.sh` (Python, `curl`, `unshare -rn`, `ip`). It serves a 4 MiB
random file from `127.0.0.1:8765` on the host, then inside `unshare -rn`
(a user and network namespace; `lo` brought up) checks that a direct
connection fails and runs 64 `curl -p -x http://127.0.0.1:3128` downloads at
once (`-p`: through a `CONNECT` tunnel), and compares each file's SHA-256.

## What it showed (2026-11-26, NixOS, kernel 7.1.8, three runs)

| Mode | Direct connect to 1.1.1.1:80 | 64 × 4 MiB through the proxy | Time (s) |
|---|---|---|---|
| `unix` | `Network is unreachable` | 64 intact, 3 of 3 runs | 2.53, 0.24, 2.35 |
| `fdpass` | `Network is unreachable` | 64 intact, 3 of 3 runs | 1.56, 1.55, 1.17 |

The times are a Python proxy and Python's toy web server on one machine; they
say only that neither way chokes, not which is faster. Outside, the received
descriptor reports `AF_INET`, bound to `('127.0.0.1', 3128)`.

## Decision it leads to

`fdpass` replaces the planned forwarder (see the decision log). It needs:

- no process inside the sandbox besides the command (nothing to keep alive,
  nothing for the command to kill or confuse);
- no Unix socket mounted into the sandbox (nothing new visible inside; design
  rule 9 is untouched);
- one listening socket per `exec`, so the runtime always knows which call a
  connection belongs to, which the log needs.

The sandbox's init process makes the socket after bringing `lo` up and before
dropping privileges, and sends it over a socket pair the runtime made before
starting the helper. Nothing else changes in the sandbox.

## Not tried

`fdpass` across the seccomp filter: the filter refuses no socket call, and the
socket is made before the filter is installed anyway. A command that binds
`127.0.0.1:3128` itself first: it cannot, init has already bound it. Both are
checked by tests in M19 rather than here.
