# M23 — Timeline, inspector and security view

| | |
|---|---|
| Weeks in the plan | v4 (D-169); no week plan |
| Started / finished | 2026-11-28 / 2026-11-28 on `nix`; `fhs` waits for a CI run |
| Commits | the experiments `1daa8e1` (E1 to E5), then the M23 commit (tag `m23` after CI is green, D-113) |
| Status | Done on `nix`; not yet verified on `fhs` |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Every run of norn leaves a full record: the event log, each call's whole output in the blob store, and a copy of the workspace after each call that could change it. Until now `norn trace` showed that record in only two ways: one line per call, or every event raw. M23 adds the first views of TraceWeaver (spec §9.13): how long each step took, filters to find the calls that matter, everything about one call on one screen, and a view of what the rules stopped. It touches only the **event log** part, and only to read it, with one small addition: a resumed run now leaves an event.

## 2. Requirements

From `docs/v4-scope.md`, M23 (T1 to T6), and the plan changes of D-170.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| T1 a written, versioned event schema, tested against the frozen v0.1 logs and a new frozen 1.2 log | Yes, `nix` | `docs/event-schema.md`; `cargo test --test compat`: `the_event_schema_document_describes_every_event_type` (one section per type of `norn::log::TYPES`, in order), `the_views_of_v4_read_executions_that_1_2_wrote` (fixture `v1.2-executions.tar`, made by norn 1.2.0); the v0.1 tests pass unchanged |
| T2 filters `--denied`, `--failed`, `--tool`, `--path`, `--uncertain` | Yes, `nix` | same compat test (seven filter cases on the 1.2 logs); `tests/trace.rs`: `filters_and_views_refuse_what_does_not_fit_together` |
| T3 durations: each call's time, and the agent's time before it | Yes, `nix` | `cargo test --lib trace::timing` (2 passed: a ten-hour stop followed by a resume is not counted as the agent's); `a_running_execution_can_be_traced_and_its_call_shows_as_running` (the 4-second command shows at least 4 s) |
| T4 the inspector `--call N` | Yes, `nix` | compat test: plan, rule, network lines and the command's output from the blob store; a write's content; an `ext.call` payload; recovery's verdict; an unknown call number refused by name |
| T5 the security view `--security` | Yes, `nix` | compat test: the denial, the refused and the reached host with bytes, the approval from request to use, the call cut off by a stop |
| T6 `norn trace` reads the log read-only, also while the run writes it | Yes, `nix` | `cargo test --lib log::` (6 passed: nothing created for a missing log; a reader beside an open writer; a read-only folder); `a_running_execution_can_be_traced_and_its_call_shows_as_running`; the compat test checks that the log's bytes are unchanged after all the views |
| D-170 a resume is recorded (`execution.resumed`) | Yes, `nix` | `a_resume_is_recorded_and_shown_between_the_calls` (`norn run --resume`, after recovery, before call 7); `a_new_mcp_session_on_an_open_execution_is_recorded_as_a_resume` |
| Every existing test passes unchanged | Yes, `nix` | `nix develop -c make test`: 278 passed, 0 failed (all suites, the security suite through both front doors, the network kill cells) |
| Both presets | Not yet | needs a CI run |

## 3. What you need to know first

**The event log.** A SQLite database (`log.db`) with one row per event, numbered by `seq` with no gaps. Rows are only added, never changed. `docs/event-schema.md` describes every kind of event; that page is new in M23.

**WAL mode and readers.** SQLite in *write-ahead log* mode appends each change to a side file, `log.db-wal`, and keeps a small shared index of it in `log.db-shm`. A reader takes a *snapshot*: it sees the database as it was when its read began, and never half of a change. That is why `norn trace` can read while `norn run` writes (E1).

**Read-only open.** Opening a file with `SQLITE_OPEN_READ_ONLY` means SQLite never writes the database. It still needs the `-shm` index and creates it if it is missing; in a folder it cannot write, that fails. `immutable=1` tells SQLite that the file cannot change, so it skips the index. That is only true when no writer is active, which is why norn uses it only when there is no `-wal` file at all (D-172).

**Wall-clock times.** Each event also has `ts`, the time it was written. Times are fine for "how long", but never for "what caused what": two events can have the same millisecond, and clocks can jump (spec §9.13).

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m23-readonly-log` (E1) | A read-only reader in a loop beside a fast writer; after `kill -9`; in a read-only folder | 24,264 reads beside 10,000 appends, 0 errors, 0 half events, the writer not slower. The "read-only folder, clean log" case was reported as working, which was wrong (see §8) | A read-only open of its own for readers (D-170) |
| `m24-trial-logs` (E2) | What the kept trial logs can answer | Nothing links a call to its message by an id; no token counts; a resume 10.7 hours later left no event | `execution.resumed` (D-170); the agent's time must restart at a resume |
| `m24-call-diff`, `m25-determinism`, `m26-otlp` (E5, E3, E4) | For M24 to M26 | See their notes | D-171 |

## 5. What was built

| File | Purpose |
|---|---|
| `src/log.rs` | `read_events` (read-only, with the narrow `immutable` fallback); `parse_ts_ms` and `days_from_civil` (times back to numbers); `TYPES` (every event kind); `ExecutionResumed` |
| `src/trace/mod.rs` (was `src/trace.rs`) | `Filter`; `render_calls` with the two time columns, the stop and resume lines, RUNNING for a live run; `is_live` |
| `src/trace/timing.rs` (new) | The agent's time and each call's time; the lines for resumes and recovery; `fmt_ms` |
| `src/trace/inspect.rs` (new) | `render_call`: the inspector |
| `src/trace/security.rs` (new) | `render_security` |
| `src/main.rs` | The new options of `norn trace` and which ones exclude each other |
| `src/runtime.rs`, `src/mcp.rs` | Write `execution.resumed`; readers use `read_events` |
| `src/approval.rs` | `norn approve` reads the log read-only |
| `docs/event-schema.md` (new) | Every event kind, its fields, the version that added it |
| `tests/fixtures/compat/v1.2-executions.tar`, `make-v1.2.sh` (new) | Three executions written by norn 1.2.0, and how they were made |
| `tests/compat.rs` | The views on the 1.2 logs; the schema page is complete |
| `tests/trace.rs` (new) | A running execution; resumes by `run --resume` and by MCP; option conflicts and paths |

```text
log.db ──read_events (read-only)──▶ Vec<Event> ──┬─ render_calls ◀─ Filter, timing::of_calls, timing::markers
                                                  ├─ render_call  ◀─ blob store (outputs, contents, payloads)
                                                  └─ render_security
```

## 6. The code, explained

**`log::read_events`.** Opens the file with `SQLITE_OPEN_READ_ONLY`. It first counts the tables `meta` and `events`: a log that `norn run` is creating at this very moment may not have them yet, and that means "no events", not an error. Then it checks the schema version, as the writer does, and reads the rows. If SQLite answers `SQLITE_READONLY_DIRECTORY` and there is no `-wal` file, it opens again as `file:<path>?immutable=1`. The path goes into a URI, so `%`, `?` and `#` in a folder name are escaped (`uri_path`).

**`log::parse_ts_ms`.** The inverse of `now_rfc3339`: it only accepts exactly the format norn writes (`2026-11-28T16:18:10.192Z`), and gives `None` for anything else, so a strange time shows as `-` instead of a wrong number. `days_from_civil` is the inverse of the existing `civil_from_days`, from the same public-domain algorithm.

**`trace::timing::of_calls`.** Walks the events once and keeps a *mark*: the time the agent last got control. The workspace being ready, any ending of a call (completed, failed, denied, waiting for approval, settled by recovery), and a resume all set it. At each `tool.requested`, the agent's time is "now minus the mark". At a call's first ending, its own time is "now minus its request". Because a resume moves the mark, a night between a stop and a resume is not counted as the agent's.

**`trace::Filter::keeps`.** Two groups. The *kinds* (`denied`, `failed`, `uncertain`) widen each other: a call is kept if it is any of the kinds asked for. `tool` and `path` narrow: the call must also match them. `failed` also keeps a command that ran and exited with a non-zero code: it reads `exit_code` from the call's output in the blob store, because the summary line is text for people. `path` uses the policy's own glob function, after taking off a leading `./` and a final `/`.

**`trace::render_calls`.** The same lines as before, with two columns after the call number. Lines for a resume or for recovery after a stop are placed by `seq`, just before the first call that comes after them. `live` comes from the execution's lock (`is_live`): when a process holds it, a call with an intent and no ending is RUNNING, not UNCERTAIN.

**`trace::render_call`.** Finds the call, then goes through its events in order and prints each in plain words. What the log keeps only as a hash (the new file of a write, the texts of an edit, the payload of an `ext.call`, the output) is read from the blob store and printed as text, 40 lines unless `--full`.

**`trace::render_security`.** Gathers what is already in the log into sections: denials, changes the runtime put back, refused connections, the hosts reached (added up per host), approvals from request to use, calls cut off by a stop. An empty section says `none`, so a reader knows it was looked at.

**`execution.resumed`.** `runtime::resume` writes it after recovery and before the agent goes on; `norn mcp` writes it when a new session continues an open execution. Old logs do not have it; the views then show nothing between the calls.

**Rust features met here.** `Iterator::peekable` with `next_if`, to take the resume lines that come before a call without losing the first one that comes after. `Option::zip`, to compute a difference only when both times are known. `Option::is_none_or`, for "no filter, or the filter matches".

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-168, D-169 | The v4 scope, drafted and approved with the recommended answers | A smaller v4; an HTML report or TUI |
| D-170 | A read-only reader; `execution.resumed`; `cause` through MCP is the request id | Reading through `EventLog::open`; guessing pauses or causes from times |
| D-171 | For M24 to M26: model name with tokens; hex OTLP ids; diffs on demand | |
| D-172 | M23 as built: the `immutable` fallback, filter rules, times, RUNNING, the 1.2 fixture | Always `immutable`; `--failed` from the summary text |

## 8. Problems hit

- **E1 had a wrong row.** Its note said a read-only reader works in a read-only folder on a cleanly closed log. The first unit test of `read_events` failed on exactly that. Repeating the case alone showed why: the earlier case had left the side files behind. The note is corrected, and the fix is the narrow fallback (D-172, Findings).
- **"UNCERTAIN" while running.** The first version of the timeline printed UNCERTAIN for the call a running execution was doing. The test that reads a running execution showed it; the lock now decides.
- **A clippy warning** (`write_literal`) and an unneeded `mut` in a test: fixed before the commit this time, by running clippy before committing.

## 9. How to run it

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test trace --test compat
nix develop -c make test
norn trace <id>                         # calls with times
norn trace <id> --failed --path 'src/**'
norn trace <id> --call 6 [--full]       # everything about call 6
norn trace <id> --security
```

## 10. What this milestone does not do

- Link a call to the message that asked for it (`cause`): M24.
- Show which files a call changed: M24.
- Token counts, replay, run diff: M25. OpenTelemetry, search: M26.
- Mark a stop that had no recovery and no resume: a run that just ended with an error and was never resumed still has no closing event.

## 11. Check your understanding

1. Why does `norn trace` not use `EventLog::open`, which already reads events?
2. A reader opens the log while `norn run` is appending. Why can it never see half of an event?
3. When is `immutable=1` safe, and why does norn not use it whenever it can?
4. Call 7 comes ten hours after call 6, with `execution.resumed` between them. What does the "agent" column of call 7 show, and why?
5. The same call shows as RUNNING in one trace and UNCERTAIN in another. What is different, and how does norn know?
6. `--denied --failed --path 'src/**'`: which calls does it show?
7. Why does `--failed` read the output from the blob store instead of the summary on the line?
8. Why is the 1.2 fixture made by the binary of the tag `v1.2`, and why must it never be made again?
9. What does a test of `docs/event-schema.md` check, and what would it catch?
10. Why are causes never taken from the `ts` field?
