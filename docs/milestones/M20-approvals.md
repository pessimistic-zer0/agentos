# M20 — Approvals: calls that wait for a person

| | |
|---|---|
| Weeks in the plan | v3 (D-153); no week plan |
| Started / finished | 2026-11-26 / 2026-11-26 on `nix`; `fhs` waits for a CI run |
| Commits | the experiment `e84a002` (E4), then the M20 commit (tag `m20` after CI is green, D-113) |
| Status | Done on `nix`. `fhs` and Debian 12 not yet verified |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

The policy says yes or no by itself, at once. Some calls deserve a person's look first, even when the policy would allow them: publishing a package, deleting files, changing CI configuration, and (in M21) calling a real service on someone's behalf. M20 lets a policy name such calls. They are not run; the agent is told they wait, a person approves or refuses from their own shell, and the agent asks again. It touches the policy (a new section), the gateway (one step after the policy), the event log (three events, one call state) and the command line (`norn approve`, `norn deny`).

## 2. Requirements

From `docs/v3-scope.md`, M20, and §7 ("an approval is needed, recorded, exact, expiring; the agent cannot give it").

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| A1 `[approvals] require`, matched by tool, command prefix, path glob or service | Yes, `nix` | `cargo test --lib policy`: `approvals_match_tool_and_what`, `bad_approval_entries_are_refused` |
| A2 the call is not run; the agent is told "waiting"; `approval.requested` logs the exact call, and a write's content | Yes, `nix` | `tests/approvals.rs`: `a_write_waits_runs_after_approval_and_only_once` (nothing written; event count; the diff in the listing) |
| A3 `norn approve` / `norn deny`: who and when, for that exact call, for a limited time; the agent cannot approve itself | Yes, `nix` | same test (`by` is logged); `an_expired_approval_is_not_used`; `a_refusal_is_final_for_that_exact_call`; `the_agent_cannot_write_its_own_approval` |
| A4 an approved identical call runs, with the policy checked again | Yes, `nix` | `a_write_waits_runs_after_approval_and_only_once`; `approval_is_on_top_of_the_policy_never_instead_of_it` |
| Without the section nothing changes | Yes, `nix` | `without_an_approvals_section_nothing_waits`; `make test` |
| Both presets, Debian 12 | Not yet | needs a CI run |

## 3. What you need to know first

**Non-blocking.** The gateway never waits for a person. A call that needs a yes ends at once with the answer "waiting". This keeps recovery simple: there is no half-finished call to reason about after a crash. E4 showed that a model handles this answer well (D-159).

**A hash as a name.** SHA-256 turns any text into a fixed-size number; equal text gives an equal number, and any change gives a different one. The approval id is the start of the hash of the call as the log records it, so it names exactly that call.

**Rename into place.** Writing a file under a temporary name and then renaming it makes it appear whole in one step, so a reader never sees half a file (the same trick `fs.write` uses).

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `m20-approval-wait` (E4) | Claude Code with a tool that answers "waiting" twice; and a tool that never answers | With a new id per attempt the model stopped; with the same id it waited 10 s, 30 s and succeeded. It waited 30 minutes for an unanswered call without cancelling | Non-blocking stays; one id per identical call (D-159) |

## 5. What was built

| File | Purpose |
|---|---|
| `src/policy.rs` | `[approvals] require`; `approval_rule(tool, subject)` |
| `src/approval.rs` (new) | The id; the verdict file; `pending`; `used`; `answer` for the command line |
| `src/gateway.rs` | `approval_rule` (what the call is, as resolved) and `approval_gate` (look for an answer, log it, or ask) |
| `src/log.rs` | `approval.requested`, `approval.used`, `approval.refused`; the `AwaitingApproval` state |
| `src/tools/mod.rs` | `ToolOutcome::Waiting`; `LoggedCall::path` |
| `src/main.rs`, `src/runtime.rs` | `norn approve`, `norn deny`, the listing with diffs |
| `src/agent/llm.rs`, `src/trace.rs`, `src/recovery.rs` | How "waiting" is shown to a model, in the trace, and after a resume |
| `tests/approvals.rs` (new) | Seven tests through `norn mcp` |

```text
agent ──call──▶ gateway: policy allows? ── no ──▶ DENIED (nobody asked)
                         │ yes
                         ▼
                approvals.require matches? ── no ──▶ run as before
                         │ yes
                         ▼
           approvals/<id>.json in the execution folder?
             no / expired / used ──▶ log approval.requested ──▶ "WAITING … norn approve <exec> <id>"
             a no                ──▶ log approval.refused   ──▶ DENIED
             a usable yes        ──▶ log approval.used ──▶ intent ──▶ effect ──▶ completed

person: norn approve <exec>          (list, with diffs)
        norn approve <exec> <id>     (writes approvals/<id>.json; checks that <id> is waiting)
```

## 6. The code, explained

**`Policy::approval_rule` (`src/policy.rs`).** Each entry is split at the first `:` into a tool and an optional "what". The subject is given by the caller as `ApprovalSubject::Path`, `Argv` or `Service`. A path is matched with the same glob code as the file rules; an argv prefix is compared word by word, so `exec:npm publish` matches `npm publish --tag x` but not `npm` alone or `npmx publish`. The function returns the rule's text, which goes to the log and to the agent.

**`Gateway::approval_rule` (`src/gateway.rs`).** Builds the subject from `Approved`, the result of the policy step, not from the call. For a write that is `target.rel`, the path the kernel resolved; a rule for `ci/**` cannot be dodged by writing `./ci/x`.

**`Gateway::approval_gate`.** Computes the id from `call.logged()`, reads the verdict file, and:

- a no → `approval.refused`, and the agent gets DENIED with who refused;
- a yes that has not expired and whose `at` time is not in an `approval.used` event yet → `approval.used`, then return `None` so `handle` carries on to the intent;
- anything else → store a write's new content in the blob store (so the listing can show a diff), log `approval.requested`, answer `Waiting`.

Why `granted_at` in `approval.used`: the same call can be approved again later under the same id. Comparing the time of the yes tells a new yes from the one already spent.

**`approval::answer` (`src/approval.rs`).** Opens the log read-only and refuses unless the id is pending, then writes the verdict under a temporary name, syncs it to disk, and renames it into place. It never writes the log: only the process driving the execution does.

**`list_approvals` (`src/runtime.rs`).** For each pending write, runs `diff -u` with the current file (or `/dev/null`) against the stored content, fed on standard input. The current file is read on the host, so the code checks first that the path is plain, that it resolves inside the workspace, and that it is a regular file, not a symlink a command may have left.

**Rust features met here.** `matches!` with a guard (`if approval == id && granted_at == at`) to search events. `Option::zip` and `is_some_and` to combine two lookups that may fail.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-159 | Non-blocking answer; one approval id per identical call | A new id per attempt; holding the call open |
| D-161 | Rules by tool and what; the resolved path; a verdict file beside the workspace; used once; expiring; a no is final; checked after the policy; the listing with diffs | `norn approve` writing the log; a yes until revoked |

## 8. Problems hit

- **A wrong test premise** (Findings): a test changed the policy file between two sessions and expected the call to be refused; a continued execution keeps the policy it started with (D-141). Replaced by tests of what does hold.
- **The id in a resumed agent's history**: the waiting outcome carries the execution id for the message; the call state in the log does not. `recovery::history` fills it in from `execution.created`, so a resumed LLM agent sees the same command.

## 9. How to run it

```sh
nix develop -c systemd-run --user --scope -p Delegate=yes cargo test --test approvals
```

By hand with Claude Code (see the README, "From an MCP client"), with `[approvals] require = ["fs.delete"]` in the policy: ask it to delete a file. It answers that the call waits and names the command. Then:

```sh
norn approve <execution>                 # what waits
norn approve <execution> a1b2c3d4e5f     # yes, once, for 60 minutes
```

and tell it to try again.

## 10. What this milestone does not do

- Approvals by webhook, chat or pull-request comment: later (v3 scope §5).
- An approval is per execution; there is no standing approval for a kind of call.
- For `exec`, the person sees the command line, not what the command will do.
- `ext.call` to remote services requiring approval by default is part of M21 (S4).

## 11. Check your understanding

1. Why does the gateway answer "waiting" at once instead of holding the call until a person answers?
2. What is the approval id made from, and why does that make an approval "exact"?
3. E4 showed a model stops retrying when the id changes on each attempt. Why would a careful model do that?
4. Where is the person's answer stored, and which two design rules stop the agent from writing it?
5. Why does `norn approve` not write the event log itself?
6. A yes is used once. Give an example where using it twice would be wrong.
7. Why is a rule matched against the path the policy resolved, not the path the agent wrote?
8. A call the policy refuses is named by an approval rule. What happens, and why is that the right order?
9. What does `granted_at` in `approval.used` make possible?
10. The listing runs `diff` on a file in the workspace from the host. What could go wrong without the checks on the path, and what do the checks prevent?
11. norn is killed between `approval.used` and the intent. What does recovery do, and what must the person do next?
