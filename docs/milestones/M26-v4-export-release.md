# M26 — OpenTelemetry, search, the trial and release of v4 (1.3.0)

| | |
|---|---|
| Weeks in the plan | v4 (D-169); no week plan |
| Started / finished | 2026-11-28 / 2026-11-28 on `nix`; `fhs` waits for a CI run |
| Commits | the M26 and release commit (tags `m26`, `v1.3` after CI is green, D-176) |
| Status | Done on `nix`; not yet verified on `fhs` |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

The last part of TraceWeaver, then its evidence and release. **OpenTelemetry**: the run as a trace that existing tools can show, written to a file and never sent by norn. **Search**: calls across every run on the machine. **The trial**: TraceWeaver used on the runs kept from earlier trials, to answer questions written down before looking. Then the documents and version 1.3.0. On the way, two views were changed by the trial, and one gap was closed: text from the log can no longer act on the person's terminal.

## 2. Requirements

From `docs/v4-scope.md`, M26 (X1 to X4) and §7.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| X1 `norn trace <id> --otel`: OTLP JSON, an execution span, a span per call, parents from the links; nothing sent | Yes, `nix`, with a part span between the two (D-175) | `a_run_as_opentelemetry_spans_keeps_its_parts_causes_and_approvals` (`tests/trace.rs`): same file twice; one trace id; hex ids; every parent and link in the file; the denied read an error span; call 7 inside the resumed part; the approval link |
| §7 the OTLP file is accepted by a collector | Yes | `docs/results/otlp-collector-nix.md`: OpenTelemetry Collector 0.151.0, four files, 31 spans, all 200, parents and the link kept |
| X2 `norn search --tool --path --outcome [--since]` across executions, no index | Yes, `nix` | `search_finds_calls_across_executions`: by outcome, tool and path, date; a bad date and a bad outcome refused; a broken log named, the others searched |
| X3 the trial: three or more questions written before looking, what could and could not be answered | Yes | `docs/results/v4-trial-nix.md`: five questions at 16:44 UTC; four answered with the views alone, one ("why") only for 1.3 logs; two views changed by it |
| X4 documents: README, the event schema, threat model, CHANGELOG, release | Yes | `README.md` "Reading a run afterwards"; `docs/event-schema.md`; `docs/threat-model.md` §12; `docs/recovery-semantics.md` §14; `docs/compatibility.md`; `CHANGELOG.md` 1.3.0 |
| §7 every existing test passes unchanged; the frozen v0.1 trace reads as before | Yes, `nix` | `nix develop -c make test`: 292 passed, 0 failed; `tests/compat.rs` v0.1 tests unchanged; `packaging/release.sh --check`: `norn 1.3.0` ran the demo task from the archive `norn-1.3.0-x86_64-linux.tar.gz` |
| §7 the kill cells still pass | Yes, `nix` | `make eval-recovery`, `docs/results/recovery-200-nix-v4.md`: 4,800 runs through norn's agents, 4,800 through MCP, 4,800 with the network on, 1,600 each for `fs.edit`, `fs.delete` and `ext.call`; 0 invariant violations and 0 duplicated effects in each |
| §7 CI green on both presets and Debian | Not yet | the release commit's run |

## 3. What you need to know first

**OpenTelemetry (OTel).** A shared format for traces, so that one viewer can show traces from many programs. A *trace* is a tree of *spans*; a span is one timed step (a start, an end, a name, attributes) with a *parent*. A span may also have *links* to other spans that are related but not its parent. *OTLP* is the protocol; its JSON form is a file of `resourceSpans` → `scopeSpans` → `spans`. A *collector* receives OTLP and passes it on to a viewer.

**Escape sequences.** A terminal shows text, but some bytes are commands to it: ESC (`\x1b`) followed by a few characters can move the cursor, clear the screen or set the window title, and some terminals have had worse ones. Text from the log (what a command printed, what a model wrote) is written by the agent's side, so it must not reach the terminal as raw bytes.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m26-otlp` (E4) | Hand-written OTLP JSON to a real collector, in five variants | Hex ids accepted, base64 refused (400); a missing parent accepted silently | Hex ids; norn's own test checks parents (D-171) |

## 5. What was built

| File | Purpose |
|---|---|
| `src/trace/otel.rs` (new) | `render_otel`: ids from SHA-256, spans for the execution, parts and calls, attributes, status, approval links |
| `src/trace/search.rs` (new) | `search`, `Query`, `Outcome`: every log read-only, filters, unreadable logs named |
| `src/trace/mod.rs` | `for_terminal` |
| `src/trace/rundiff.rs` | The order of lines inside a stretch (from the trial) |
| `src/main.rs` | `trace --otel`, `norn search`; every text view goes through `for_terminal` |
| `Cargo.toml` | 1.3.0 |
| `tests/trace.rs` | OTLP, search, escape sequences |
| `docs/results/v4-trial-nix.md`, `otlp-collector-nix.md` | The trial and the collector check |
| `docs/threat-model.md` §12, `docs/recovery-semantics.md` §14, `docs/compatibility.md`, `README.md`, `CHANGELOG.md` | Documents |

```text
execution span ── part "start" ──────── call 1, call 2, … call 6
               ├─ part "recovery after a stop"
               └─ part "resumed by norn run --resume after 6 call(s)" ── call 7
call 4 (used approval a02b…) ┄┄link┄┄▶ call 3 (waited for it)
```

## 6. The code, explained

**`otel::hex_id`.** SHA-256 of a few fixed words and the execution id (with a zero byte between parts, so `ab`+`c` and `a`+`bc` differ), cut to 16 bytes for the trace id and 8 for a span id, written as lower-case hex: E4 showed the collector refuses base64. The same run therefore always gives the same file.

**`otel::render_otel`.** The execution span covers the first to the last event and carries the agent, policy, status and, if recorded, the token totals. One pass over the events opens a new *part* at each `recovery.started` and `execution.resumed`, and notes which part each call's request falls into. Each call span starts at its request and lasts its own time (M23's timing); its attributes hold the tool, a short form of the arguments, the rule, the cause, the summary, the checkpoint, network counts, and approval facts. A call that used an approval gets a *link* to the span of the call that waited for it. A denied, failed, uncertain or not-applied call has an error status with a message; the others leave it unset, which OTLP reads as "fine".

**`search::search`.** Lists executions as `norn status` does, reads each log read-only, skips those created before `--since` (comparing the first event's time as text: UTC in a fixed format sorts as time), and prints each call that matches every filter given. A log that cannot be read is listed at the end; one broken folder does not stop the search. For a refused or failed call the line ends with the rule or the error.

**`trace::for_terminal`.** Goes through the text once. Every control character except a line break and a tab becomes `\u{..}`, visible and harmless. `main.rs` calls it at the one place each text view is printed. The JSON and OTLP outputs skip it: `serde_json` already writes control characters as `\u001b`.

**The stretch order in `rundiff`.** Between two common calls, the calls only A has are printed first, then B's calls in B's own order; a B call with the same tool and path as one of A's becomes a `~` line at B's place. Before the trial, the `~` lines came first.

**Rust features met here.** `sha2::Sha256` used through its `Digest` trait (`new`, `update`, `finalize`). `char::is_control`. `Option::transpose` to turn an `Option<Result<…>>` (an optional `--outcome` that may not parse) into a `Result<Option<…>>`.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-175 | Spans by part of the run, approvals as links; search without an index; `for_terminal`; the two trial changes | Messages as spans; escaping in each view |
| D-176 | 1.3.0; `m23` to `m25` on their own commits after green runs, `m26` and `v1.3` on the release commit | All tags on the release commit |

## 8. Problems hit

- **The v3 trial's runs were not kept**, so the trial used the 32 runs kept from v1 and v2 and the frozen 1.2 runs.
- **`norn diff` printed lines out of order** on two real runs (the trial's question 2). Fixed; the test still passes, as its runs had one difference per stretch.
- **`norn search` did not say why a call was refused** (question 5). It does now.
- **Raw terminal output.** Writing the threat model's §12 showed that `--call` would print a command's output with any escape sequence in it. `for_terminal` fixes it for every text view, with a test that a command's `ESC ] 0 ; … BEL` comes out as text.
- **A clippy suggestion that would have been wrong.** Clippy asked to compare an OTLP `intValue` with the number directly; the value is a string in OTLP JSON, so that comparison would never match. The test compares the text instead.
- **A blocked edit.** One edit to the decision log, written as a shell command, was stopped by the session's permission check; the same entries were then added with the file-edit tool.

## 9. How to run it

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test trace
nix develop -c make test
nix develop -c make eval-recovery
norn trace <id> --otel > run.json
curl -H 'Content-Type: application/json' --data-binary @run.json http://127.0.0.1:4318/v1/traces   # to a collector you run
norn search --outcome denied --since 2026-11-01
```

## 10. What this milestone does not do

- Send traces anywhere (D-169).
- Draw them: a viewer such as Jaeger was not run (not in `nixpkgs`).
- Index runs for search: v5, with the daemon.
- Hide anything in the state folder from someone who can read it.

## 11. Check your understanding

1. Why are span ids taken from a hash of the execution id rather than chosen at random?
2. Why is a message or script step not a span of its own, and where does the cause go instead?
3. What is a span link, and why is the approval a link and not a parent?
4. The collector accepted a span whose parent was not in the file. What does norn do about that, and where?
5. Why does `--since` compare text, and when would that be wrong?
6. A command prints `\x1b]0;owned\x07`. What does `norn trace --call` show, and why is `--json` not filtered?
7. Why is escaping done once at printing, rather than in each view?
8. Which trial question could not be answered for old logs, and what would answer it for new ones?
9. Why do `m23` to `m25` get their own commits as tags while `m26` shares the release commit with `v1.3`?
