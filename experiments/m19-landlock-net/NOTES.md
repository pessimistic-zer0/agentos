# E3 — Landlock's network rules as a second wall

**Question (v3 scope §4, E3).** Landlock (the kernel's allow-list for a
process; norn uses it for files since M2) gained network rules in ABI 4
(Linux 6.7). Could they limit the sandbox to connecting to the proxy port
3128 only, as a second wall behind the empty network namespace? What about
Debian 12 (kernel 6.1)?

**Run.** `python3 probe.py` asks the kernel for its Landlock ABI version
(`landlock_create_ruleset` with the `VERSION` flag), then restricts itself to
"connect TCP to port 3128, bind TCP to port 8000" and tries five things. It
was run as is and inside `unshare -rn` (an empty network namespace with `lo`
up, like the sandbox's).

## What it showed (2026-11-26, NixOS, kernel 7.1.8; same result both ways)

```
kernel 7.1.8: Landlock ABI 9
  bind 127.0.0.1:8000 (listed): allowed
  bind 127.0.0.1:8001 (not listed): Permission denied
  connect 127.0.0.1:8000 (bind-only port): Permission denied
  connect 127.0.0.1:3128 (listed; nobody listening): Connection refused
  UDP sendto 127.0.0.1:53: allowed
```

So the rules work, and they are exactly as wide as the documentation says:

- **TCP only.** UDP is not covered at all.
- **By port only, not by address.** "Connect to 3128" allows 3128 on any
  address; "no connect to 8000" refuses even a server the command itself
  started on `127.0.0.1:8000`.

Debian 12 (kernel 6.1) has no ABI 4, so there the rules cannot be used at all.
The Ubuntu CI runners' kernel (7.0) was not probed here.

## Decision it leads to

Not used in v3. The empty network namespace stays the only wall, as today,
because a Landlock network rule here would:

- break ordinary test suites, which start a server on some local port and
  connect to it (a web framework's tests, a database test fixture); "only port
  3128" refuses them, and there is no way to say "any port on 127.0.0.1";
- add nothing for UDP, the other way out;
- exist on two of the three test systems only, so it could not be relied on.

Inside the empty namespace the only address that leads anywhere is the
proxy's listening socket, which the runtime holds (E2); a port rule would
not narrow that further. If the namespace ever failed, the file rules and
seccomp would still hold, but the network would be open: that is the risk
this decision accepts, and the threat model says so.
