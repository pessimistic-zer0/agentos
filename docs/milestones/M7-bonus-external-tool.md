# M7 — Bonus: one external tool that cannot be rolled back

| | |
|---|---|
| Weeks in the plan | None. This is the "Bonus, only after everything above is done" section of the v1 document, built after `v0.1` |
| Started / finished | 2026-11-13 / 2026-11-13 |
| Commits | `a50a5ce` … the commit that adds this file (tag `m7`) |
| Status | Done with gaps (not yet verified on the `fhs` preset: these commits have not been pushed, so CI has not run on them) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Until now every effect of a tool call landed in the working folder, and a folder can always be put back from a copy. That made recovery's hardest case easy: "we cannot tell what happened, so go back to the last save point". This milestone adds the one thing a copy cannot undo: asking a program *outside* the runtime to do something. A new tool, `ext.call`, sends a text payload to a small fake service. The work is to make that safe to crash around: the request carries a label (an idempotency key) written down before it is sent, and after a crash or a timeout the same request is sent again with the same label, so the service does the work once.

It touches three of the five parts: the **gateway** (a fifth tool), the **policy** (which services may be called), and **recovery** (a rule that goes forwards instead of back). The sandbox and the checkpoints are untouched. It is not part of v0.1 and not part of the definition of done.

## 2. Requirements

There is no row in the v1 milestone table. The requirement is the Bonus section itself: *"One fake external tool: a small local service with injectable timeouts, called through the gateway with an idempotency key. It gives the 'uncertain call that cannot be rolled back' path something real to do."* Prince's note of 2026-11-12 added: its own reconciliation rule, fault-injection cells, decision-log entries, a threat-model update and a milestone document; and only after CI is green.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| Starts only after `v0.1` is tagged and CI is green | Yes | Tag `v0.1` is on `8eaca2f`. CI run 43385661105 on `c3c9502` is green (D-072). The first bonus commit, `a50a5ce`, comes after both |
| One fake external service, small and local | Yes | `tests/common/fake_service.rs`: standard library only, listens on `127.0.0.1`. `cargo run --example fake_service` runs it as a program |
| …with injectable timeouts | Yes | `Fault::DropBefore` (request lost), `Fault::DropAfter` (answer lost), `Fault::DelayMs`. `tests/external.rs::a_lost_request_and_a_lost_answer_look_the_same_from_outside` passes |
| Called through the gateway | Yes | `ToolCall::ExtCall` goes through requested → policy → intent → effect → completed in `gateway.rs`. `allowed_call_is_logged_keyed_and_applied_once` checks each record |
| …with an idempotency key | Yes | The intent's `plan.key` is `agentos-<id>-c<call>`; the service recorded the same key (same test) |
| Denied unless the policy allows it | Yes | `shipped_policy_denies_ext_call`: `tool.denied`, rule `no matching rule`, the service saw nothing |
| Its own reconciliation rule | Yes | `recovery::reconcile_external` (D-078, ADR 0012). `an_outage_leaves_the_call_uncertain_until_recovery_gets_an_answer` and `killed_after_the_intent_recovery_sends_the_request` pass |
| Fault-injection cells | Yes | `make eval-recovery`, external cells: 1,600 runs, 0 invariant violations, 0 duplicated effects, 932 of 932 verdicts matching the service's record (`docs/results/recovery-ext-200-nix.md`) |
| The harness can fail | Yes | Two seeded bugs gave 48 and 70 duplicated effects in 160 runs each; the real code 0 (`docs/results/recovery-ext-mutations.md`) |
| Nothing in v0.1 got worse | Yes | Same `make eval-recovery`, the 24 original cells: 4,800 runs, 4,791 killed, 0 violations, 0 duplicated effects, 3,895/3,895 verdicts correct, recovery p50 19.8 ms / p99 55.9 ms (`docs/results/recovery-200-nix-m7.md`). `make test`: 24 unit, 1 benign, 14 external, 2 LLM mock, 4 recovery, 32 security, 4 vertical slice, all pass; clippy clean with `-D warnings` |
| Decision-log entries | Yes | D-073 to D-081, three Findings rows, glossary terms |
| Threat-model update | Yes | `docs/threat-model.md` §7 |
| Recovery semantics written down | Yes | `docs/recovery-semantics.md` §9 |
| This document | Yes | |
| Same on the `fhs` preset | **Not yet verified** | Needs a push and a green CI run on these commits. `ci.yml` already runs the new tests (they are part of `cargo test`) and uploads the second report |

Not met: the `fhs` row. Nothing was weakened or skipped to pass.

## 3. What you need to know first

**An effect outside the workspace.** `fs.write` changes a file in the working folder. `exec` runs a command that can only change that folder. Both can be undone by copying a checkpoint back. A request to another program (charge a card, send an email, open a pull request) happens in *that* program's world. AgentOS has no copy of it.

**HTTP in four lines.** A client opens a connection and sends a *request*: a method and path (`POST /apply`), some *headers* (name: value lines), then a body. The server sends back a *status code* (200 = done, 4xx = "I refuse this request", 5xx = "I had a problem") and a body.

**A lost request and a lost answer look the same.** If no answer comes, the client cannot tell whether the request never arrived (nothing was done) or arrived and was carried out, with only the answer lost (it *was* done). From the client's chair both are "I waited and nothing came". This is the heart of the milestone. See the picture in `docs/recovery-semantics.md` §9.1.

**Idempotent.** An operation is idempotent if doing it twice has the same result as doing it once. "Set the light to on" is; "press the light switch" is not.

**Idempotency key.** A way to *make* an operation idempotent. The client attaches a label it chose to the request. The server remembers labels: if it sees one again, it does not do the work again; it repeats its first answer. Payment APIs work this way. The client can then safely send again whenever it is unsure.

**Timeout.** How long the client waits before it gives up on an answer. Giving up changes nothing at the server.

**Roll back and roll forward.** Two ways to leave a half-done state. Roll back: undo it (what `exec` recovery does). Roll forward: finish it (what this milestone does, because undoing is impossible).

**Loopback.** The address `127.0.0.1` always means "this same computer". A request to it never leaves the machine.

**Test double (a fake).** A simple stand-in for a real outside system, written so that tests can control it (make it lose an answer on demand) and look inside it (what did it really apply?).

## 4. Experiments

The v1 table lists none for the bonus. Instead of a throwaway program, the two halves of the mechanism were built and tested alone before the gateway used them (commit `3a1940d`, when nothing in the runtime called them yet):

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| The fake service and the client, by themselves (`tests/external.rs`, part 1) | Send with a key twice; lose a request; lose an answer; send again | Second send returns `"replayed": true` and the service still holds one entry. A lost request and a lost answer both come back as "unknown" after the timeout; one applied nothing, the other applied | The client really cannot tell the two apart. Sending again with the same key settles both |
| A server that redirects to the service | Does the HTTP library follow a redirect by default? | With redirects set to 0, the client returns an error and the service sees nothing | A URL check at policy load is not enough by itself; the request must also refuse to be sent elsewhere |
| Seeding two "obvious" designs into the finished code (`docs/results/recovery-ext-mutations.md`) | (1) After a crash, tell the agent the call did not happen. (2) Retry on timeout with a fresh key | 48 and 70 doubled effects in 160 runs each | Why neither is acceptable, in numbers |

## 5. What was built

| File | Purpose |
|---|---|
| `src/policy.rs` | The `[external]` section: named services with loopback-only URLs, a timeout, a number of attempts. `check_external` |
| `src/tools/ext.rs` (new) | One HTTP request; what its answer proves (`Answer`); the idempotency key; "send until answered" |
| `src/tools/mod.rs` | `ToolCall::ExtCall`, `LoggedCall::ExtCall`, `ToolOutput::External` |
| `src/gateway.rs` | The fifth tool: policy step, intent contents, the effect, and stopping when no answer comes |
| `src/recovery.rs` | `reconcile_external`: settle an uncertain call by sending it again |
| `src/agent/llm.rs` | One match arm so the code compiles; the LLM is *not* offered the tool |
| `tests/common/fake_service.rs` (new) | The fake service |
| `examples/fake_service.rs` (new) | The same service as a program, for trying things by hand |
| `tests/external.rs` (new) | 14 tests: the service and client alone, then `ext.call` through the real binary |
| `tests/recovery/main.rs` | Eight external cells added as a second big test; the 24 original cells are unchanged |
| `Makefile` | `eval-recovery` runs the two big tests one after the other |
| `.github/workflows/ci.yml` | Uploads the second report |
| `docs/decisions/0012-…md`, `docs/threat-model.md` §7, `docs/recovery-semantics.md` §9, `README.md`, the v1 document's Bonus status | The written record |

How the pieces talk to each other:

```text
 scripted agent ── ToolCall::ExtCall {service: "ledger", payload} ──► gateway
                                                                        │ 1 blob store: keep the payload
                                                                        │ 2 log tool.requested (payload as a hash)
                                                                        │ 3 policy: is "ledger" listed? -> its URL
                                                                        │ 4 log tool.intent {url, key, payload_hash,
                                                                        │                    timeout_ms, attempts}   (fsync)
                                                                        │ 5 tools::ext: POST url, Idempotency-Key: key
                                                                        │      2xx ─► log tool.completed (no checkpoint)
                                                                        │      4xx ─► log tool.failed
                                                                        │      none, after `attempts` tries ─► stop the run;
                                                                        ▼                                       intent stays open
                                                              fake service on 127.0.0.1
                                                              "have I applied this key? no -> apply, remember"

 after a kill or such a stop:  agentos recover ─► reads the intent ─► sends the SAME request, SAME key
                                                    2xx ─► tool.reconciled APPLIED      4xx ─► NOT APPLIED
                                                    none ─► error, no verdict, execution stays interrupted
```

## 6. The code, explained

### 6.1 `policy.rs` — which services exist

```rust
pub struct ExternalRules {
    #[serde(rename = "service", default)]
    pub services: Vec<ExternalService>,
    #[serde(default = "default_ext_timeout_ms")]
    pub timeout_ms: u64,
    #[serde(default = "default_ext_attempts")]
    pub attempts: u32,
}
```

`#[serde(rename = "service")]` makes the TOML read naturally: each `[[external.service]]` block becomes one element of the list. `#[serde(default = "…")]` names a function that supplies the value when the file leaves the key out. The whole section is `#[serde(default)]` on `Policy`, so the shipped policies, which have no such section, load exactly as before with an empty list. An empty list means every `ext.call` is refused (design rule 2: no matching rule means deny).

```rust
fn is_loopback_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("http://127.0.0.1:") else { return false };
    let port = rest.split('/').next().unwrap_or("");
    ...
}
```

`let … else` is a Rust form that says "if this pattern does not match, leave now". The function checks that after the prefix comes a port made only of digits, then nothing or a `/`. Design rule 7 says paths are never confined by string checks, and that rule is about *attacker-controlled* input. Here the input is the policy file, which is trusted; the check is a guard against mistakes and bad edits. The obvious weaker version, "starts with `http://127.0.0.1`", would accept `http://127.0.0.1:80@evil.example/`, where everything before `@` is a user name and the real host is `evil.example`. The test lists eleven such forms.

`check_external` returns the decision *and* the matching service, so the gateway uses the URL of the very rule that allowed the call.

### 6.2 `tools/ext.rs` — one request, honestly reported

```rust
pub enum Answer {
    Applied { status: u16, body: String },
    Refused { status: u16, body: String },
    Unknown { why: String },
}
```

Three outcomes, not two. `send` maps an HTTP 2xx to `Applied`, a 4xx to `Refused`, and *everything else* to `Unknown`: an error from the HTTP library (timeout, connection closed, nobody listening), a 5xx, a redirect. The obvious two-way version, `Result<String, Error>`, would push every caller towards treating a timeout as a failure, and a failure is recorded as "nothing was changed".

```rust
let agent: ureq::Agent = ureq::Agent::config_builder()
    .timeout_global(Some(timeout))
    .http_status_as_error(false)
    .max_redirects(0)
    .proxy(None)
    .build()
    .into();
```

This is the builder pattern: each call sets one option and returns the builder, and `.build().into()` turns the finished configuration into a client. `http_status_as_error(false)` means a 404 comes back as a response to classify, not as an error. `max_redirects(0)` and `proxy(None)` keep a request that the policy allowed for `127.0.0.1` from going anywhere else.

```rust
pub fn idempotency_key(execution_id: &str, call_id: u64) -> String {
    format!("agentos-{execution_id}-c{call_id}")
}
```

The key is computed from two things the log already holds. A random key would also work (it is stored in the intent before the request), but a computed one cannot differ between the runtime that sends first and the recovery that sends later.

`send_until_answered` is a loop: send; if the answer is not `Unknown`, return it with the number of requests sent; otherwise send again, up to `attempts` times, **with the same key**. That last part is the whole safety argument. Seeded bug 2 in §4 changes only this line.

### 6.3 `tools/mod.rs` — the new variants

`ToolCall` gets `ExtCall { service, payload }`. `#[serde(rename = "ext.call")]` is what lets a script say `tool = "ext.call"`. In the log, `LoggedCall::ExtCall` holds `payload_hash` and `bytes` instead of the payload, the same split as for `fs.write` (ADR 0004: hashes in the log, contents in the blob store).

`is_mutating()` stays `false` for it. That function means "changes the workspace, so take a checkpoint", and an external call does not change the workspace.

Adding a variant to an enum makes the compiler list every `match` that does not handle it. That is how the one arm in `llm.rs` was found. It is a Rust feature worth noticing: the compiler found every place that needed a decision.

### 6.4 `gateway.rs` — the fifth tool

Policy step:

```rust
ToolCall::ExtCall { service, payload } => match policy.check_external(service) {
    (d, Some(s)) if d.allowed => {
        Decided::Allow(d, Approved::Ext { service: service.clone(), url: s.url.clone(), payload: payload.clone() })
    }
    (d, _) => Decided::Deny(d),
},
```

`(d, Some(s)) if d.allowed` is a pattern with a *guard*: it matches a pair whose second part is `Some`, and only if the condition after `if` holds. The URL placed into `Approved::Ext` comes from the policy. The agent's request has no URL field at all.

The intent:

```rust
Approved::Ext { service, url, payload } => json!({
    "service": service, "url": url,
    "key": ext::idempotency_key(&self.dir.id, call_id),
    "payload_hash": crate::blob::content_hash(payload.as_bytes()),
    "timeout_ms": self.policy.external.timeout_ms,
    "attempts": self.policy.external.attempts,
}),
```

Everything recovery will need is here, written and fsynced before the request (design rule 3). Recovery never has to read the policy again or ask anyone what the request was.

The effect:

```rust
match answer {
    Answer::Applied { status, body } => Ok(ToolOutput::External { status, body, attempts: sent }),
    Answer::Refused { status, body } => Err(ToolError::new(...)),
    Answer::Unknown { why } => anyhow::bail!("call #{call_id} to external service `{service}` is UNCERTAIN: ..."),
}
```

`effect` returns `Result<Result<ToolOutput, ToolError>>`: the outer `Result` means "the runtime itself failed", the inner one "the tool failed". `Applied` and `Refused` use the inner one and become `tool.completed` and `tool.failed`. `Unknown` uses the *outer* one: `bail!` returns a runtime error, `handle` passes it up with `?`, and the process exits. No ending is written. The alternative, writing FAILED, would put a statement in the log ("nothing was changed") that may be false. Stopping leaves the log in the state a crash leaves, and that state already has a tested handler.

The existing crash points need no change: P1 and P2 sit before and after the intent, `faults::arm("P3")` starts its timer just before `effect`, and P7 follows the completion record. P4 to P6 are inside the `if mutating` block and are never reached for this tool.

### 6.5 `recovery.rs` — settling forwards

```rust
LoggedCall::ExtCall { service, payload_hash, .. } => reconcile_external(dir, c.call_id, service, payload_hash, plan),
```

`reconcile_external` reads `url`, `key`, `timeout_ms` and `attempts` from the intent's plan, reads the payload from the blob store by its hash, and calls the same `send_until_answered` the gateway used. Then:

- `Applied` → `Outcome { applied: true, … }`. No checkpoint and no rollback: the workspace is not involved.
- `Refused` → `not_applied(…)`.
- `Unknown` → `bail!`. Recovery returns an error. It has already written `recovery.started`, and it writes nothing else. The call is still uncertain in the log, so the next `agentos recover` does the same thing again. D-056 already allowed `recovery.started` to appear several times.

Why not ask the service "did you apply key K?" and write the answer down? Because "no" can be out of date: the dead runtime's request may still be on its way and land a moment later. The agent would then be told NOT APPLIED, ask again under a new key, and the work would be done twice. Sending again with the same key has no such gap: whichever request the service acts on, it acts once.

Design rule 5 says an uncertain call is never re-executed before it is reconciled. Sending the same request with the same key is not a re-execution; it is the reconciliation. What the rule forbids, the agent's *next* call going ahead first, cannot happen: `run --resume` calls recovery first and stops when it stops.

Recovery can itself be killed (P8). Its three P8 points are around this step. Running it again sends the request once more with the same key, which is safe for the same reason.

### 6.6 `tests/common/fake_service.rs` — the fake

One thread accepts connections; each connection gets its own thread. `read_request` reads bytes until the blank line that ends the headers, picks out `Idempotency-Key` and `Content-Length`, then reads that many body bytes. This is a hand-written, minimal HTTP parser, good enough for one client.

The promise is five lines:

```rust
fn apply(state: &Mutex<State>, key: &str, payload: &str) -> Result<(u64, bool), ()> {
    let mut st = state.lock().unwrap();
    if let Some(a) = st.applied.iter().find(|a| a.key == key) {
        return if a.payload == payload { Ok((a.seq, true)) } else { Err(()) };
    }
    ...push, return (seq, false)
}
```

A `Mutex` lets only one thread at a time into the code between `lock()` and the end of the function. "Have I seen this key?" and "apply it" happen under one lock, so two requests with the same key that arrive at the same instant cannot both apply. If the check and the apply were two separate locked steps, they could.

The faults: `DropBefore` sleeps and closes the connection without applying. `DropAfter` applies, then sleeps and closes without answering. `DelayMs` sleeps and then behaves normally, which lets a test kill the runtime while its request is still waiting inside the service. Faults are queued per payload, so a test can aim at one step of a script.

The service lives in the test program's process. That is what makes it "external" in the way that matters: killing `agentos` does not touch it, and the test reads `applied()` directly, without going through anything the runtime wrote.

### 6.7 `tests/recovery/main.rs` — the external cells

A second big test next to the original one. Workload: write a file, `ext.call order-1`, run a command, `ext.call order-2`, write a file. Each trial picks one of the two orders and one of eight cells:

| Cell | What happens |
|---|---|
| P1, P2, P7 | The runtime is killed before the intent, after it, or after the completion record |
| P3 | Killed while the request is in flight. The service is made slow, or loses the answer, so the kill lands inside the request |
| P8 | Killed at P2 or P3, then recovery is killed at one of its three points |
| T1 | Nothing is killed. The service loses one or two requests |
| T2 | Nothing is killed. The service loses an answer (it did the work) |
| T3 | Nothing is killed. The service answers none of the three attempts; half the time the first recovery gets no answer either |

After each trial, `recover` and `run --resume` are run until the script has finished. A run or recovery that stops "uncertain" is tried again, and the test checks that the stopped attempt wrote no verdict and made no new call. Then the oracle: each order must be in the service's record exactly once. Duplicates are counted **by payload**, not by key, because the service never applies one key twice; a doubled effect is the same order under two keys.

Each cell also counts how often its situation really occurred ("as intended": the kill happened, or the run finished despite the lost answer, or the run stopped uncertain) and fails if that is not more than half. A cell must not pass by not happening.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-073 | A fifth gateway tool `ext.call {service, payload}`; not offered to the LLM; not in the shipped policies | Network for `exec`; a general HTTP tool; adding it to the LLM's tool list |
| D-074 | Services are named in the policy; URLs must be `http://127.0.0.1:<port>` | Any URL; `localhost`; agent-supplied URLs with prefix rules |
| D-075 | The fake service: std-only HTTP, in memory, in the test process, three faults queued per payload | A second binary; a framework; Python; killing the service too |
| D-076 | 2xx applied, 4xx refused, everything else unknown; no redirects, no proxy | "Connection refused" or 5xx as failure; timeout as failure |
| D-077 | Key = execution id + call id, written in the intent with everything needed to re-send | Random key; payload hash as key; agent-supplied key |
| D-078 | Settle an uncertain call by sending it again with the same key; no answer settles nothing (ADR 0012) | Status query; guess NOT APPLIED; guess APPLIED; always ask a human; retry for ever |
| D-079 | No answer in a live run stops the run with the intent open | A new UNCERTAIN ending handed to the agent; recording FAILED |
| D-080 | Eight external cells as a second test; the service's record is the oracle; duplicates counted by payload | Adding the tool to the 24-cell matrix; counting by key; treating a stop as a failure |
| D-081 | `make eval-recovery` runs its tests one at a time | Leaving the two big tests to run together |

## 8. Problems hit

1. **The bonus could not start when it was asked for.** Prince's note made it wait for green CI, and the first CI run was red. Two real bugs had to be fixed first: a checkpoint copy that outlived the killed runtime (B-007), and, caused by that fix, a lock that outlived it for a few milliseconds (B-008). Both are in `docs/bugs.md`. They are v0.1 bugs, not bonus bugs, but they are why this milestone starts at commit `a50a5ce` and not earlier.
2. **A borrow error in the first policy test** (`demo().check_external("ledger")`): `check_external` returns a reference into the policy, and `demo()` made a temporary policy that was dropped at the end of the statement. The compiler refused it (E0716). Fixed by keeping the policy in a variable. A small, typical lifetime lesson: a reference cannot outlive the thing it points into.
3. **The first harness version judged the wrong call.** The P3 timer can fire after the chosen `ext.call` has ended, inside a later file write. The test would then have compared that file write's verdict with the service's record. No failure was seen in 80 runs, but an applied `fs.write` would have been reported as a wrong verdict. Fixed before the first full run by checking which call was actually cut off (Findings, 2026-11-13).
4. **Two tests at once.** The first full-size run started the original 24 cells and the external cells together, 16 workers on 12 CPUs. Both passed, but the recovery times of the v0.1 cells would no longer be comparable with earlier runs. `make eval-recovery` now runs one test at a time (D-081), and the results of record are from such a run.

## 9. How to run it

All tests of the bonus:

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes -q cargo test --test external
# test result: ok. 14 passed
nix develop -c make eval-recovery          # about 13 minutes; RUNS=20 for a quick look
# ... the 24 original cells, then:
# external total: 1600 runs, 0 invariant violations, 0 duplicated effects, ...
```

By hand, in three terminals. Terminal 1, the service, told to lose the first answer for `order-1`:

```sh
FAKE_SERVICE_FAULTS='order-1=drop_after' nix develop -c cargo run --example fake_service
```

Terminal 2, a policy and a script:

```sh
cp policies/nix.toml /tmp/ext-policy.toml
cat >> /tmp/ext-policy.toml <<'EOF'

[external]
timeout_ms = 1000
attempts = 3
[[external.service]]
name = "ledger"
url = "http://127.0.0.1:8099/apply"
EOF
cat > /tmp/ext-script.toml <<'EOF'
task = "try the external tool"
[[step]]
tool = "ext.call"
service = "ledger"
payload = "order-1"
EOF
RUN="systemd-run --user --scope -p Delegate=yes -q target/debug/agentos --state-dir /tmp/agentos-ext"
$RUN run --repo examples/demo-repo --policy /tmp/ext-policy.toml --script /tmp/ext-script.toml
#   #1   ext.call ledger (7 bytes)      allow  HTTP 200 after 2 requests
```

Terminal 1 shows `applied-then-dropped` and then `replayed` with the same key. `curl http://127.0.0.1:8099/state` shows one entry.

To see the uncertain stop: stop the service (Ctrl-C) and run the same command again. It ends with `call #1 to external service 'ledger' is UNCERTAIN: no answer after 3 request(s)`. `$RUN status` says interrupted. `$RUN recover <id>` says "still UNCERTAIN" while the service is down. Start the service again, and `$RUN recover <id>` prints `#1 ext.call APPLIED`.

To see a kill in mid-request: start the service with `FAKE_SERVICE_FAULTS='order-1=delay:3000'`, start the run, and `kill -9` the `agentos` process within three seconds. Then `recover`.

## 10. What this milestone does not do

- **Not verified on `fhs`.** Needs a push and a green CI run.
- **One fake service.** No real API, no TLS, no credentials. The policy refuses anything but `127.0.0.1`. A real external tool is a new decision, not a configuration change.
- **No human approval.** v1 cut it. For a real irreversible effect it would be the first thing to add.
- **The LLM agent cannot use the tool.** Only the scripted agent can.
- **Exactly-once depends on the service.** A service that ignores the key turns every retry into a second effect.
- **The service is never crashed** in the tests, and keeps its state in memory.
- **The answer of a reconciled call is not kept** beyond 200 characters in the verdict.
- **No transaction across the workspace and the service.** A later rollback of the workspace does not undo an earlier external call.
- **Allowing the tool opens a way out for data** (the payload is the agent's choice). See `docs/threat-model.md` §7.
- **Recovery times of the external cells are not benchmarks.** They include waiting for a deliberately silent service.

## 11. Check your understanding

1. Why can an uncertain `exec` be rolled back, and an uncertain `ext.call` not?
2. The runtime waits 300 ms and gets no answer. Name the two things that may have happened at the service, and say why the runtime cannot tell them apart.
3. What is an idempotency key, who chooses it here, and when is it written to disk relative to the request?
4. Why is the key computed from the execution id and the call id instead of being random? Why not a hash of the payload?
5. The gateway got no answer after three attempts. Why does it write neither `tool.completed` nor `tool.failed`? What does the log look like afterwards, and which existing mechanism deals with it?
6. Recovery could ask the service "did you apply key K?". Describe the sequence of events in which trusting a "no" leads to the work being done twice.
7. Design rule 5 says an uncertain call is never re-executed before it is reconciled. Recovery sends the request again. Why is that not a violation?
8. The harness counts duplicated effects by payload, not by key. Why would counting by key always give zero?
9. In the mutation check, seeded bug 1 left cells P1 and P2 clean. Why were those two correct by accident?
10. The policy check `is_loopback_url` is a string check. Design rule 7 forbids string checks for confinement. Why is this one acceptable, and what else (in `ext.rs`) keeps a request from leaving the machine?
11. What can a hostile agent do with `ext.call` that it could not do with the four v0.1 tools? What stops it under the shipped policies?
12. An interviewer says: "So you have exactly-once delivery." What is the accurate answer?

## Notes added after the milestone

- **2026-11-13, `fhs` preset.** The "not yet verified" above describes the state when this document was written. Prince pushed the same day, and CI run <https://github.com/pessimistic-zer0/agentos/actions/runs/43436681321> on commit `1bc27da` is green on Ubuntu 24.04: lint, all tests including `tests/external.rs`, and `make eval-recovery`. External cells: 1600 runs, 0 invariant violations, 0 duplicated effects, 942/942 cut-off calls classified correctly, 303 stops with the call left uncertain. Result of record: `docs/results/recovery-ext-200-fhs.md`.
