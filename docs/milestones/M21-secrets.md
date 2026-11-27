# M21 — Secrets used by the runtime, for real services

| | |
|---|---|
| Weeks in the plan | v3 (D-153); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` waits for a CI run |
| Commits | the experiment `41c7125` (E5), then the M21 commit (tag `m21` after CI is green, D-113) |
| Status | Done. Verified on both presets and Debian 12 by CI run 45920326159 on `01edb37`; tagged there (D-167) |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Acting on a real service (opening an issue, a pull request) needs a token. Handing the token to the agent would put it in the model's conversation, in commands, in logs. M21 has the runtime hold the token instead: the policy says where a secret comes from, and which service gets it in which header; the agent only names the service and writes the payload. The bonus tool `ext.call` grows from "a local HTTP service" to "an HTTPS service anywhere", with that header added by the runtime. It touches the policy, the gateway, `ext.call`, recovery, and the fake service of the tests.

## 2. Requirements

From `docs/v3-scope.md`, M21, and §7 ("a secret never appears in the log, blobs, workspace, patch or sandbox").

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| S1 `[secrets]` from the environment or a file, read at start; never in the log, blobs, workspace or sandbox | Yes, `nix` | `tests/secrets.rs`: `the_service_gets_the_secret_and_nothing_else_does` (searches every file of the execution; prints a command's environment), `a_missing_secret_stops_the_run_before_it_starts`; `cargo test --lib secrets` (2) |
| S2 `ext.call` to HTTPS services with a header built from a secret; the key and recovery rule of D-077/D-078 stay | Yes, `nix` | same test (the service saw the header once); `recovery_sends_the_same_request_with_the_secret_again`; `a_service_with_an_untrusted_certificate_never_gets_the_secret`; `tests/external.rs` 14 passed unchanged |
| S3 redaction as a backstop | Yes, `nix` | the `echo` payload: the agent saw `Bearer [secret:TRACKER_TOKEN]` |
| S4 remote services wait for approval by default | Yes, `nix` | `a_remote_service_waits_for_approval_by_default` |
| Tested with a local fake HTTPS service (D-153) | Yes | `FakeService::start_tls` with `tests/fixtures/tls/` |
| Both presets, Debian 12 | Not yet | needs a CI run |

## 3. What you need to know first

**TLS and certificates.** HTTPS is HTTP inside TLS, which encrypts the connection and proves the server's identity: the server shows a certificate, signed by a certificate authority (CA) the client trusts, for the name the client asked for. If any of that fails, the client stops before sending anything. norn's client trusts Mozilla's list of CAs, compiled in (E5), or, for one service, only the CA in its `ca_file`.

**Bearer token.** A secret string that grants access to whoever presents it, usually in the header `Authorization: Bearer <token>`. Whoever has it can use it; that is why the agent must not.

**Redaction.** Replacing a secret's value with a placeholder in text before it is stored or shown. Here it is a backstop: the design keeps values out of every place by construction, and redaction catches a service that sends one back.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m21-https` (E5) | `ureq` against a real host, a self-signed one, and a local test server with its own CA | Real host OK; unknown CA and wrong name refused; one trusted CA works and narrows trust; a single self-signed certificate is refused (`CaUsedAsEndEntity`) | No new dependency; `ca_file` per service; a test CA plus a server certificate (D-157) |

## 5. What was built

| File | Purpose |
|---|---|
| `src/secrets.rs` (new) | `Secrets`: load from environment or file, fill in `{secret:NAME}`, redact text and JSON, find values in bytes. `Debug` prints names only |
| `src/policy.rs` | `[secrets]`; `ca_file`, `headers`, `approval` on a service; `https://` URLs; checks that keep a secret off plain HTTP |
| `src/tools/ext.rs` | `Target`: URL, filled-in headers, CA; `send` uses it |
| `src/gateway.rs` | Holds the secrets; builds the target just before sending; S4; redacts outputs and errors |
| `src/recovery.rs` | Rebuilds the same target from the intent and the execution's policy |
| `src/runtime.rs` | Reads secrets at start and resume; `norn check`; the patch backstop |
| `tests/common/fake_service.rs` | HTTPS mode, the `Authorization` header recorded, `require_auth`, `echo` |
| `tests/fixtures/tls/` | Test CA, server certificate and key, and the script that made them |
| `tests/secrets.rs` (new) | Six tests |

```text
policy:  [secrets] TRACKER_TOKEN = { from = "env" }
         service "tracker": https://…, headers: Authorization = "Bearer {secret:TRACKER_TOKEN}"

agent ──ext.call tracker "payload"──▶ gateway
          intent: url, key, headers = templates ─────────────▶ event log (no value)
          Target::build: templates + secrets ──▶ HTTPS ──────▶ service (gets the token)
          answer ──▶ redact values ──▶ blob store, log summary, agent
```

## 6. The code, explained

**`Secrets::load` (`src/secrets.rs`).** For each `[secrets]` entry, read the variable (by default the secret's own name) or the file (without its last newline). The error names the secret and where it looked, never a value. Values under 8 characters or with a line break are refused: the first would make redaction mangle text, the second could split a header.

**`Secrets::redact` and `redact_json`.** Replace each value by `[secret:NAME]`, longest value first, so a value that contains another is replaced whole. `redact_json` walks a JSON value and redacts every string in it; the gateway uses it on a tool's output by turning the output into JSON and back.

**`ext::Target::build` (`src/tools/ext.rs`).** Fills the header templates and reads the CA file. Built in `effect`, just before sending, and dropped after: the only place in the program where a filled-in header exists. `send` gives `ureq` a TLS configuration that trusts only the CA when one is given, and adds the headers.

**The intent (`Gateway::plan`).** Adds `headers` (the templates) and `ca_file`. Recovery needs them to send the same request again (D-077); the log can hold them because they hold no value.

**`reconcile_external` (`src/recovery.rs`).** Reads `headers` and `ca_file` from the intent (absent in intents from before v3, which then mean "none"), loads the secrets from the execution's saved policy if a header needs one, and sends. If the variable is missing, it stops with the error that names it: the call stays uncertain.

**S4 (`Gateway::approval_rule`).** After the `[approvals]` rules, an `ext.call` to a service whose URL is `https://` and whose `approval` is not `false` needs a yes. The reason text says how to switch it off.

**The patch (`finish_parts` in `src/runtime.rs`).** After the patch is written, it is searched for the values; if one is found it is replaced and a note printed. The agent never sees a value, so this should never fire.

**Rust features met here.** A hand-written `impl Debug` so that printing the struct (in an error, a log line) cannot show a value. `Option::transpose` to turn `Option<Result<T>>` into `Result<Option<T>>` while reading the optional `headers` from the intent.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-157 | `ureq`'s TLS as it is; `ca_file`; `rustls` for the test service | The system's store; turning checks off; `rcgen` |
| D-162 | Secrets from env or file; `https://` services (replaces part of D-074); templates in the intent; redaction in the gateway and the patch; remote services wait for approval; a TLS failure counts as "no answer" | Redaction inside the event log; sending a secret over plain HTTP |

## 8. Problems hit

- **A test of D-074 failed** because the rule changed on purpose: `https://127.0.0.1:8099/` used to be refused as "not plain http" and is now a valid service. The test was changed to the new rule, with six refused HTTPS forms added; D-074 is marked as replaced in part by D-162.
- **An untrusted certificate leaves the call uncertain.** A failed handshake sends nothing, but norn cannot tell it from other errors without matching on text, so the run stops as for any missing answer (D-077), and the token is never sent. Recorded in D-162 as a known rough edge.

## 9. How to run it

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test secrets
```

By hand, against the fake service over HTTPS: the example program serves plain HTTP only, so the tests are the way to see HTTPS. The README section "Secrets for a real service" shows a policy for a real one.

## 10. What this milestone does not do

- Secrets in a command's own traffic (a token for `git push` through the proxy): that would need TLS interception, which is out (D-153).
- Secrets from a keyring or a vault (v3 scope §5).
- Telling "the handshake failed, nothing was sent" apart from "no answer" (D-162).
- The trial with a ticket that ends in an approved `ext.call` is M22.

## 11. Check your understanding

1. Why does the runtime, and not the agent or a command, add the token to the request?
2. What does the intent of an `ext.call` record about the header, and why not the value?
3. After a crash, how does `norn recover` send the same request, and what happens if the token's variable is not set?
4. Why is a header with a secret refused for a plain `http://` service?
5. What does `ca_file` do, and why does it *narrow* trust rather than add to it?
6. A service echoes the request headers back. Trace what the agent sees and where the value is replaced.
7. Why are secrets shorter than 8 characters refused?
8. Why is redaction called a backstop here, and what is the main defence?
9. Why does a call to an `https://` service wait for approval by default?
10. An untrusted certificate stops the run with the call uncertain. Was anything sent? Why does norn still call it uncertain?
11. Why does `Secrets` have its own `Debug`?
