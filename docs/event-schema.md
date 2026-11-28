# The event log: what each event means

Every execution keeps an **event log**: a list of everything that happened, in order, that is only ever added to (D-005). It is a SQLite file, `log.db`, in the execution's folder. This page describes each kind of event: what it means, its fields, and the version of norn that first wrote it. It is the reference for anyone who reads a log without norn, and for the views of `norn trace`.

The promise behind it is `docs/compatibility.md`: the log's `schema_version` is 1; a later 1.x version may add new kinds of event and new fields, but keeps the names and meanings below. A test (`tests/compat.rs`) checks that this page has one section per event kind, in the order of `norn::log::TYPES`.

## The table

```sql
CREATE TABLE meta   (key TEXT PRIMARY KEY, value TEXT NOT NULL);   -- schema_version = '1'
CREATE TABLE events (seq INTEGER PRIMARY KEY, ts TEXT NOT NULL, type TEXT NOT NULL, call_id INTEGER, body TEXT NOT NULL);
```

| Column | Meaning |
|---|---|
| `seq` | 1, 2, 3, … with no gap (invariant 5). **Order always comes from `seq`**, never from `ts` |
| `ts` | Wall-clock time in UTC, `YYYY-MM-DDTHH:MM:SS.mmmZ`. For people only: durations in `norn trace` come from it, causes never do (spec §9.13) |
| `type` | The event kind, the headings below |
| `call_id` | The tool call the event belongs to, or empty |
| `body` | The event as one JSON object, with `"type"` and the fields below |

`norn trace <id> --json` prints each event as one JSON object per line, with `seq` and `ts` added. Readers should open the file read-only (`SQLITE_OPEN_READ_ONLY`): a run may still be writing it, and the WAL journal gives each read a fixed snapshot (D-170).

**Calls.** A tool call has a `call_id` (1, 2, …) and goes through these events: `tool.requested`, then either `tool.denied`, `approval.requested` or `approval.refused` (it was not run), or `tool.intent` followed by `tool.completed` or `tool.failed`. A call with an intent and no ending was cut off by a stop: it is **uncertain** until recovery writes `tool.reconciled` (design rules 3 to 5). `norn::log::calls` checks these steps and refuses any other order.

**Hashes.** Fields named `output`, `content_hash`, `old_hash`, `new_hash`, `payload_hash` and `content` hold `sha256:<hex>`: the content is in the execution's blob store (`blobs/`), not in the log.

## Execution

### `execution.created`

The first event of every log. Since 0.1.

| Field | Meaning |
|---|---|
| `execution_id` | The execution's id, also its folder's name when it was made |
| `repo` | The repository the workspace was copied from |
| `policy_name` | The policy's `name` |
| `policy_hash` | `sha256:` of the policy text; the text itself is saved as `policy.toml` beside the log |
| `agent` | `scripted`, `llm` or `mcp` |
| `task` | The task text (for `mcp`: `MCP session (<client name>)`) |

### `workspace.created`

The repository was copied into the workspace, and the base checkpoint taken. Since 0.1.

| Field | Meaning |
|---|---|
| `files_copied` | Number of files copied |
| `denied` | Paths left out because a deny rule matched (D-008) |
| `skipped` | Paths left out by `filesystem.skip`. Since 1.0.0; absent when empty |
| `checkpoint` | The base checkpoint, `cp-0000`: the start of the output patch |

### `agent.message`

One message of the conversation, stored before the call it leads to (ADR 0005). Since 0.1.

| Field | Meaning |
|---|---|
| `message` | For the LLM agent: an OpenAI-style chat message (`role`, `content`, `tool_calls`, `tool_call_id`). For `norn mcp`: once per session, `{"mcp_session": {client, protocol, tools}}`, the tools the client was offered. The scripted agent writes none |

## Tool calls

### `tool.requested`

The agent asked for a call. Since 0.1.

| Field | Meaning |
|---|---|
| `call_id` | The call's number |
| `call` | The call as logged: `{"tool": "fs.write", "path": …, "content_hash": …, "bytes": …}`. Large arguments are replaced by a hash and a size |

### `tool.denied`

The call was refused and not run. Terminal for the call. Since 0.1.

| Field | Meaning |
|---|---|
| `call_id` | |
| `rule` | The rule that decided (`filesystem.deny[0] ".env"`, `no matching rule`), or `invalid request` |
| `reason` | `policy`, or the error in the request |

### `tool.intent`

The call is allowed, and this is what it will do. Written and fsynced **before** the effect starts (design rule 3). Since 0.1.

| Field | Meaning |
|---|---|
| `call_id` | |
| `rule` | The rule that allowed it |
| `plan` | What recovery needs to settle the call if it is cut off: the resolved path, the program and cgroup of a command, the URL, idempotency key and header templates of an `ext.call` (never a secret's value) |

### `tool.completed`

The call ended normally. Terminal. Since 0.1.

| Field | Meaning |
|---|---|
| `call_id` | |
| `summary` | One line for people (`exit 0, 0.2s`, `wrote 37 bytes`) |
| `output` | `sha256:` of the full result in the blob store |
| `checkpoint` | For a call that can change files: the checkpoint taken after the effect (design rule 4); otherwise `null` |

### `tool.failed`

The call ran into an error; nothing was changed. Terminal. Since 0.1.

| Field | Meaning |
|---|---|
| `call_id` | |
| `error` | The error as the agent was told it |

### `policy.violation`

A command changed paths outside the write rules; the runtime put them back from the checkpoint before the call ended (D-009). Since 0.1.

| Field | Meaning |
|---|---|
| `call_id` | |
| `reverted` | The paths put back |

### `exec.network`

The connections a command asked the network proxy for, written when the command has ended, before its checkpoint. A command cut off by a stop has none (M19). Since 1.2.0.

| Field | Meaning |
|---|---|
| `call_id` | |
| `connections` | One object per connection: `host`, `port`, `method` (`CONNECT`, `GET`, …), `allowed`, `rule` (the rule that allowed it, or why not), `bytes_up`, `bytes_down`, and `error` when an allowed connection could not be made |
| `dropped` | Connections past the per-call limit, counted but not listed |

## Approvals

### `approval.requested`

The call needs a person's yes and has none that can be used: it was not run, and the agent was told to wait and ask again. Terminal for this call (M20). Since 1.2.0.

| Field | Meaning |
|---|---|
| `call_id` | |
| `approval` | The approval id: `a` and 10 hex digits of the SHA-256 of the logged call. The same call gets the same id each time it asks |
| `rule` | The rule that asks for approval |
| `content` | For a write or an edit: `sha256:` of the whole new file, so the person sees what they approve |

### `approval.used`

A person's yes for this exact call was found and is used now; the intent follows. Since 1.2.0.

| Field | Meaning |
|---|---|
| `call_id` | The call that uses it (a later call than the one that asked) |
| `approval` | |
| `by` | Who said yes (the user name of whoever ran `norn approve`) |
| `granted_at` | When they said it |

### `approval.refused`

A person said no to this exact call. Terminal, like a denial. Since 1.2.0.

| Field | Meaning |
|---|---|
| `call_id` | |
| `approval` | |
| `by` | Who said no |

## Recovery

### `recovery.started`

`norn recover` (or a resume) found calls with no ending after a stop. May appear more than once if recovery itself was stopped. Since 0.1.

| Field | Meaning |
|---|---|
| `pending` | The call ids without an ending |
| `orphan_checkpoints` | Checkpoint folders no event names, deleted (D-011) |

### `tool.reconciled`

Recovery's verdict on one call that had no ending (D-016). Terminal. Since 0.1.

| Field | Meaning |
|---|---|
| `call_id` | |
| `applied` | Did the call's effect happen and stay? |
| `reason` | How recovery knows |
| `checkpoint` | If applied: the checkpoint taken now for it |
| `restored` | If not applied by rolling back: the checkpoint the workspace was put back to |

### `recovery.completed`

Recovery finished. Since 0.1.

| Field | Meaning |
|---|---|
| `reconciled` | Number of calls settled |
| `duration_ms` | How long it took |

### `execution.resumed`

The agent goes on with an execution that had stopped (D-170). Written after any recovery events of the same start. Since 1.3.0; before it, a resume left no event, and a pause looked like a slow agent.

| Field | Meaning |
|---|---|
| `by` | `norn run --resume`, or `norn mcp` (a new session continuing an open execution) |
| `after_calls` | Number of calls made before the resume |

## The end

### `execution.finished`

The run ended and its patch was written. Since 0.1.

| Field | Meaning |
|---|---|
| `status` | `completed`, or `stopped` (the agent gave up, or a limit was reached) |
| `summary` | Why it ended, in one line |
| `patch` | The path of `result.patch` |

### `result.applied`

`norn apply` copied the result into a repository (D-108). Written after `execution.finished`; an execution may have several. Since 1.0.0.

| Field | Meaning |
|---|---|
| `target` | The repository written to |
| `changes` | One line per change, as `norn apply` printed them |
