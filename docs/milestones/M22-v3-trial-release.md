# M22 — Evidence and release of v3 (1.2.0)

| | |
|---|---|
| Weeks in the plan | v3 (D-153); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` waits for a CI run |
| Commits | `beaf41a` (security cases), `c32e42b` (B-016), `c5ac6f5` (lint), `1fbe1d1` (trial), then the release commit (tags `m21`, `m22`, `v1.2` after CI is green, D-164) |
| Status | Done. Verified on both presets and Debian 12 by CI run 45920326159 on `01edb37`; tagged there (D-167) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

M19 to M21 added three ways for the outside world to meet a run: a network, a person's approval, and secrets. M22 checks them the way the rest of norn is checked: adversarial cases sent through both front doors, a trial with a real client, the kill cells with the network on, the documents brought up to date, and a release. It changes no part of the design; it found one bug (B-016).

## 2. Requirements

From `docs/v3-scope.md`, M22, and §7.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| D1 security cases: escape the allow-list (other hosts, raw IPs, DNS, UDP, a second proxy hop, `CONNECT` to a non-allowed port); reach a secret (environment, `/proc`, an echoing service); approve oneself; replay an approval with other arguments; use an expired one; both front doors | Yes, `nix`, with one split | `docs/results/security-v3-nix.md`: three new cases, 36 passed through each front door, 0 escapes. Replaying with other arguments and an expired yes need a person between two calls, which the scripted front door cannot pause for; they are tested through `norn mcp` only (`tests/approvals.rs`) |
| D1 both presets | Not yet | CI |
| D2 a trial: a ticket that needs a package download through the allow-list, and one that ends in an approved `ext.call`, with a local fake HTTPS service | Yes, one ticket doing both | `docs/results/v3-trial-nix.md`: solved, 13 turns, 0.12 USD, checked on a clone |
| D3 documents: threat model (the new way out), recovery semantics, CHANGELOG, release | Yes | `docs/threat-model.md` §9 to §11; `docs/recovery-semantics.md` §11 to §13; `CHANGELOG.md` 1.2.0; README |
| §7 the kill cells still pass with the network on | Yes, `nix` and `fhs` | `docs/results/recovery-network-200-nix.md`: 4,800 runs, 4,799 killed, 0 violations; on Ubuntu in CI runs 45816245961 and 45817518838 (D-166, D-165): 4,800 runs, 0 violations each |
| `make test` on the release tree | Yes, `nix` | 267 passed, 0 failed; `packaging/release.sh --check`: `norn 1.2.0` ran the demo task from the archive |
| §7 CI green on both presets and Debian | Not yet | the release commit's run |

## 3. What you need to know first

**Front door.** How tool calls reach the gateway: norn's own agents (scripted or LLM) inside `norn run`, or an MCP client through `norn mcp`. The security suite sends every case both ways (D-144).

**Flaky test.** A test that passes or fails without a change in the code, because of timing. A flaky failure is still information: B-016 was found by one.

## 4. Experiments

None new; M22 used E1 to E5.

## 5. What was built

| File | Purpose |
|---|---|
| `tests/security/main.rs` | Three v3 cases: the allow-list, a secret, self-approval |
| `examples/fake_service.rs` | `FAKE_SERVICE_TLS`, `FAKE_SERVICE_AUTH` for the trial |
| `src/runtime.rs` | B-016: a clear message for a run stopped in setup |
| `tests/hardening.rs`, `tests/mcp.rs` | B-016's tests |
| `docs/results/security-v3-nix.md`, `v3-trial-nix.md`, `recovery-network-200-nix.md` | Records |
| `Cargo.toml` | 1.2.0 |

## 6. The code, explained

**`the_network_allow_list_cannot_be_escaped`.** Allows one host server by address and runs Python in the sandbox that tries twelve ways out: a direct connection to the allowed host (the proxy is the only way), to the internet, UDP, a DNS lookup, the proxy for another host, the allowed host on another port, `localhost` and `LOCALHOST.` (names for this machine), plain HTTP elsewhere, a URL with a user name before the host, an IPv6 literal, and a `CONNECT` sent *inside* an allowed tunnel (a second hop). Each must raise. Then it fetches from the allowed host, which must work, so that the refusals cannot come from a broken proxy. The log must hold at least five refused connections.

**`a_secret_cannot_be_reached_by_the_agent_or_its_commands`.** A fake HTTPS service that echoes headers; an `ext.call` that makes it do so; then Python that looks for the token in its environment, in PID 1's environment, and in the runtime's files beside the workspace. The echo must reach the agent as `[secret:TOKEN]`, the probes must fail, the service must have the token, and `grep` over the whole execution folder must find nothing.

**`the_agent_cannot_approve_its_own_call`.** Computes the approval id the way the runtime does (`norn::approval::approval_id`), then tries to write the yes file through `fs.write` (with `..` and through `/proc/self/cwd`) and from a command, and asks again. The call must still wait, and nothing may appear in the execution folder.

**B-016 (`runtime::agent_kind`).** A resume first finds out which agent drove the execution. It now also checks for `workspace.created`; without it the run was stopped in setup, and the message says so.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-163 | The TOCTOU swapper starts after the base checkpoint; a clear message for a run stopped in setup | Resumable setup; skipping such runs in `norn mcp` (it already does) |
| D-164 | 1.2.0; `m19`, `m20` on their commits after their runs; `m21`, `m22`, `v1.2` on the release commit | `m21` on its own commit (its run failed) |

## 8. Problems hit

- **B-016**, found by CI run 45818853651 on Debian 12: see `docs/bugs.md`. The console showed one line of the reason; the full log was in the job's artifact, opened only after several guesses (Findings).
- **A lint failure committed twice** (`beaf41a`, `c32e42b`): a clippy warning in a new test; the second commit was made in the same command as the clippy run that reported it (Findings). Fixed in `c5ac6f5`.
- **The trial's approver was a script.** It shows the mechanism, not how people answer.

## 9. How to run it

```sh
nix develop -c make test                 # all suites; security both ways; network kill cells
nix develop -c make eval-security        # the security record
nix develop -c make eval-recovery        # 200 kills per cell, then the same with the network on
```

The trial's setup is in `docs/results/v3-trial-nix.md`; the project lives outside this repository.

## 10. What this milestone does not do

- No verification on `fhs` or Debian until CI runs on the release commit.
- The trial is one ticket, one run: no rate is claimed.
- Approval replay and expiry are tested through MCP only.

## 11. Check your understanding

1. Why does each security case also prove that the allowed path works?
2. What is a "second proxy hop", and why can the allowed host not be used as one here?
3. Why are `localhost` and `LOCALHOST.` tried separately?
4. The secret case reads `/proc/1/environ`. Whose environment is that inside the sandbox, and why is the token not in it?
5. How does the self-approval case know which file the agent would have to write?
6. B-016 was found on Debian but never locally. What made the difference, and why did the test's own summary hide the cause?
7. Why was starting the swapper after the base checkpoint not a weakening of the test?
8. Why do `m19` and `m20` sit on their own commits, but `m21` does not?
