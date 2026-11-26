# norn v3 — Scope

*Drafted 2026-11-26 (D-152); approved by Prince the same day (D-153). Road map: D-137 (v3 is Phase 2 of the spec: hardening).*

**Status: approved; this is the plan for v3.** All four milestones. TLS interception stays out. The trial uses a local fake HTTPS service. The release is 1.2.0. Nothing is built yet.

## 1. In one paragraph

Today a sandboxed command has no network at all, the only outside effect is `ext.call` to a service on `127.0.0.1`, and every allowed call runs at once. That keeps norn safe and makes it useless for two common jobs: anything that downloads dependencies, and anything that acts on a real service (open a pull request, file an issue). v3 adds the three controls the spec planned for exactly that (§9.8, §9.9, §9.12): **a network allow-list** for commands, **secrets** the runtime uses on the agent's behalf without the agent ever seeing them, and **approvals**, so that a risky call waits for a person. All three are off unless a policy turns them on, so every 1.x policy keeps its meaning.

## 2. Words used here

| Term | Plain meaning |
|---|---|
| Egress | Traffic leaving the sandbox for the outside |
| Allow-list | The only hosts a command may reach; everything else is refused |
| Proxy | A program that makes connections on someone else's behalf. Here: part of norn, outside the sandbox, the only way out |
| `CONNECT` | How a client asks an HTTP proxy for a tunnel to `host:443`. The proxy sees the host name and the byte counts, not the encrypted content |
| TLS interception | A proxy that decrypts HTTPS by posing as the server. Not done here |
| Credential brokering | The runtime holds a secret and uses it for the agent; the agent only names what it wants done |
| Approval | A person's yes to one exact call, recorded, with an expiry |

## 3. What is in v3

### M19 — A network allow-list for commands

| # | Item | Notes |
|---|---|---|
| N1 | `[network] allow = ["pypi.org", "files.pythonhosted.org"]` in the policy; absent or empty means no network, as today | Host names, optionally with a port (default 443); exact names, `*.` for subdomains |
| N2 | A proxy in the runtime, outside the sandbox: it accepts `CONNECT host:port` (HTTPS) and plain `GET`s (HTTP) for allowed hosts, resolves DNS itself, and refuses everything else | The sandbox keeps its empty network namespace. Inside, a small forwarder (part of the sandbox helper) listens on `127.0.0.1:3128` and passes each connection to the proxy through a Unix socket mounted into the sandbox. Commands get `HTTPS_PROXY`/`HTTP_PROXY` pointing at it |
| N3 | What went out is in the log: per `exec` call, each host, port, bytes each way, and whether it was allowed | A new event kind (allowed by D-126); `norn trace` shows it |
| N4 | Recovery: a command that used the network is rolled back like any command; what it sent cannot be | Stated in the recovery semantics; the allow-list is the control |

### M20 — Approvals

| # | Item | Notes |
|---|---|---|
| A1 | `[approvals] require = [...]`: calls that need a person's yes, matched by tool, command prefix, path glob or service | e.g. `["exec:npm publish", "fs.delete:**", "ext.call:github"]` |
| A2 | A call that needs approval is not run: the agent is answered "waiting for approval", the log records `approval.requested` with the exact call (and the diff for a write) | The gateway never blocks; no new waiting state for recovery to reason about. How long a client will wait for one answer is experiment E4 |
| A3 | `norn approve <id> <call>` / `norn deny`: a person's answer, recorded with who and when, valid for that exact call (same arguments, same hash) for a limited time | Run from the person's shell; commands in the sandbox cannot reach the state folder (design rule 9), so the agent cannot approve itself |
| A4 | The agent asks again; an approved identical call runs, with the policy checked again at that moment | Spec §9.12: "revalidated at execution time" |

### M21 — Secrets, used by the runtime, for real services

| # | Item | Notes |
|---|---|---|
| S1 | `[secrets] GITHUB_TOKEN = { from = "env" }` (or `{ from = "file", path = "..." }`): secrets named in the policy, read by the runtime at start | Never written to the log, the blobs, the workspace or the sandbox's environment |
| S2 | `ext.call` to HTTPS services outside the machine, for services the policy names, with a header built from a secret (`auth = "Bearer {secret:GITHUB_TOKEN}"`) | The runtime makes the request on the host side, so the token never enters the sandbox. The idempotency key and the recovery rule of D-077/D-078 stay; a service that ignores the key leaves an uncertain call uncertain, as today |
| S3 | Redaction as a backstop: any secret's value found in a tool result, the log or the patch is replaced by `[secret:NAME]` before it is stored or shown | Spec §9.9 |
| S4 | Remote `ext.call` services require approval by default (A1), unless the policy says otherwise | An irreversible effect on someone else's system is the case approvals exist for |

### M22 — Evidence and release

| # | Item | Notes |
|---|---|---|
| D1 | Security cases: escape the allow-list (other hosts, raw IPs, DNS, UDP, a second proxy hop, `CONNECT` to a non-allowed port), reach a secret (environment, `/proc`, a service that echoes headers back), approve oneself, replay an approval with other arguments, use an expired one | Both front doors, both presets |
| D2 | A trial: a ticket that needs a package download, through the allow-list; one that ends in an approved `ext.call` | With a local HTTPS fake service unless Prince wants a real one (question 3) |
| D3 | Documents (threat model: the new way out for data; recovery semantics), CHANGELOG, release | Version: question 4 |

## 4. Experiments before building

| # | Question | How |
|---|---|---|
| E1 | Do `pip`, `npm`, `cargo` and `git` work through an HTTPS `CONNECT` proxy given by `HTTPS_PROXY`, with only the registry hosts allowed? Which hosts does each really need? | A 50-line proxy that logs every `CONNECT`; install one package with each tool |
| E2 | A forwarder inside an empty network namespace: `127.0.0.1:3128` to a Unix socket mounted from outside. Does it hold up under parallel downloads? | Extend `experiments/m2-unshare` |
| E3 | Landlock's network rules (ABI 4, kernel 6.7+): can the sandbox be limited to connecting to port 3128 only, as a second wall? What happens on Debian 12 (kernel 6.1), which has no ABI 4? | Probe on all three systems; if absent, the empty namespace is the only wall, as today |
| E4 | How long does an MCP client wait for one `tools/call`, and what does a model do with "waiting for approval, ask again"? | Claude Code with a probe tool that sleeps, and one that answers "waiting" twice |
| E5 | HTTPS from the runtime: does `ureq` with its default TLS verify certificates against the system store on NixOS, Ubuntu and Debian? A new dependency would need its own entry | A request to a real host and to a self-signed one |

## 5. What is not in v3

- **TLS interception**, content inspection, injecting secrets into a command's own traffic (spec §9.8 leaning: no).
- **Inbound connections**, UDP, DNS inside the sandbox (the proxy resolves).
- **A package mirror or cache** (spec §9.8 "2+"): parked.
- **Approvals by webhook, Slack or GitHub comment**; organisation-wide policies: later (spec §9.12).
- **Secrets from a keyring or a vault**: only the runtime's environment and a file.
- **A daemon, an API, several agents**: v5.

## 6. What changes, and what does not

- **Changes:** "no network, ever" becomes "no network unless the policy allows hosts". This is the largest change to the threat model since v0.1, and the README, threat model and `norn check` say so plainly. The default stays: no network.
- **Unchanged:** the five parts; the ten design rules (rule 9 is what keeps an agent from approving itself); one crate; synchronous Rust (the proxy is threads); the compatibility promise (all new settings are optional, off by default).
- **Dependencies:** none expected; E5 decides whether HTTPS needs one.

## 7. How v3 counts as done

- With the allow-list off, every existing test passes unchanged.
- With it on, `pip install` (and `npm`, `cargo` as E1 allows) works for allowed registries and nothing else is reachable; every connection is in the log.
- A secret never appears in the log, blobs, workspace, patch or sandbox, under the new security cases.
- An approval is needed, recorded, exact, expiring; the agent cannot give it.
- The kill cells still pass, with network on.
- The trial is recorded; CI is green on both presets and Debian.

## 8. Questions only Prince can answer (answered 2026-11-26, D-153: all four milestones; TLS interception out; a local fake HTTPS service; 1.2.0)

1. **Approve the scope:** all four milestones, or a smaller v3 (for example M19 alone, network only, which removes the trials' biggest limit).
2. **TLS interception stays out?** Recommended: yes. Without it a secret cannot be added to a command's own HTTPS traffic, which is why secrets go through `ext.call` (S2).
3. **The trial's real service:** a local HTTPS fake service (recommended; no account needed), or a real one such as GitHub with a token you create for it, limited to a test repository.
4. **Version:** everything is optional and off by default, so semantic versioning gives **1.2.0** (recommended).
