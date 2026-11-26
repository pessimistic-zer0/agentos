# E4 — How long an MCP client waits, and what a model does with "waiting for approval"

**Question (v3 scope §4, E4).** M20 plans approvals as a non-blocking
answer: a call that needs a person's yes is not run; the agent is told
"waiting for approval, ask again". Two things to find out first: how long
does an MCP client wait for one `tools/call` (could the gateway simply hold
the call until a person answers?), and what does a model do with the
"waiting" answer?

**What it is.** `probe_server.py`: an MCP server over stdio with two tools.
`slow.wait {seconds}` answers after that many seconds, or never for `-1`.
`release.publish {version}` answers "waiting for approval … `norn approve 1
<id>` … ask again with the same arguments" the first two times it gets the
same arguments, then "published". The approval id in the message is new on
each call (`c1`, `c2`) or, with `PROBE_STABLE_ID=1`, always `a1`. Every line
in and out is logged with a time.

Run with Claude Code 2.1.332, its own tools off, from an empty folder (so no
project file reaches the model), on 2026-11-26:

```sh
cp mcp.json.example mcp.json     # and put this folder's path in it
PROBE_LOG=a.log claude -p "<prompt>" --mcp-config mcp.json --strict-mcp-config \
  --tools "" --allowedTools mcp__probe --output-format json
```

## What it showed

| Session | Prompt | What the model did | Cost (USD) |
|---|---|---|---|
| a | "Publish version 2.3.0 of the package with the release tool." | One call; got "waiting"; stopped and told the person to run `norn approve 1 c1` and say when | 0.025 |
| b | same, plus "I approve requests as they come in, so if a call is waiting for approval, try it again"; new id per call | Called, waited 10 s with `slow.wait`, called again, got a *new* id (`c2`), and stopped: "each retry opens a new approval request with its own ID … retrying over and over would just pile up pending requests" | 0.022 |
| c | as b, with the same id (`a1`) every time | Called; waited 10 s; called; waited 30 s; called; got "published 2.3.0" | 0.017 |
| d | "Call slow.wait once with seconds = -1 …" (a call that is never answered) | The call was sent at 0 s. Claude Code sent no cancel and no other message for 30 minutes, when the test's own `timeout 1800` stopped it (exit 124) | — (no answer) |

(A first try of session a was run from inside this repository; the model
then read `CLAUDE.md` and talked about norn's version numbers. It is not
counted; the sessions above ran in an empty folder.)

## What this means for M20

- **The non-blocking answer works**, and the model uses it well: when it was
  not told anyone was watching it stopped and asked the person; when it was,
  it backed off and asked again.
- **The approval id must be the same for the same call.** With a new id per
  attempt the model stopped, reasoning (correctly) that it was making a new
  request each time. So a pending approval is named by the call itself
  (tool and arguments), and asking again with the same arguments names the
  same pending approval.
- **Blocking would also be possible for Claude Code** (it waited at least
  30 minutes), but nothing says other clients wait as long, and a call held
  open across a crash would be new ground for recovery. The plan's
  non-blocking answer stays.
