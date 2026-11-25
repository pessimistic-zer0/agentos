# M15 experiments: what an MCP client really does (E1, E2, E3)

Run on 2026-11-26 on the development machine with Claude Code 2.1.332 as the client and `probe_server.py` (in this folder) as the server. The probe offers one tool, `text.upper`, and logs every line it receives and sends. Claude Code was started with `claude -p … --mcp-config <file> --strict-mcp-config --tools "" --allowedTools …`. Four short sessions; together they cost under 0.10 USD of Prince's account.

## E1: which protocol, and which details matter

Reading the specification (modelcontextprotocol.io, 2026-11-26): the current revision is **2026-07-28**. It has **no `initialize` handshake**. Every request carries `_meta["io.modelcontextprotocol/protocolVersion"]` and the client's capabilities, and a server MUST answer `server/discover`. The previous revision, **2025-11-25**, still starts with `initialize` and `notifications/initialized`. A server may support both.

What Claude Code sent:

- First `server/discover`, with `protocolVersion` `2026-07-28` in `_meta`, then `tools/list`, then `tools/call`. No `initialize`. It is a "modern" client.
- **`cacheScope` must be `"public"` or `"private"`.** The first probe sent `"session"`; Claude Code rejected the tool list (`Invalid option: expected one of "public"|"private"`), retried it three times, then started the model with **no tools and no error shown to the model**. A small mistake in the server's answer silently removes every tool. The fix was `"private"`.
- **Dots in tool names:** allowed by the specification ("SHOULD" use letters, digits, `_`, `-`, `.`). Claude Code accepted `text.upper`, but showed it to the model and to its permission rules as `mcp__probe__text_upper`; on the wire it sent back `text.upper`. Another client may handle dots differently.
- **A refusal as a tool result** (`isError: true`, text "refused: the policy does not allow this") reached the model as an error with the text intact. That is the right shape for norn's denials.
- `tools/list` was asked for once in a normal session.

## E2: can the client be kept from going around norn?

With `--tools ""` the model reported exactly one tool, the probe's. Asked to read `/etc/hostname` and to run `id` "by any means", it said it had no file tool and no shell and did neither. So with Claude Code, **`--tools ""` plus `--strict-mcp-config` leaves the MCP server as the only way to act**. This was one session and the model's own report, checked against the probe's log (no call arrived). It is a property of the client's setup, not something norn can enforce: a client started without these flags keeps its own Bash and file tools.

## E3: the server dies in the middle of a call

With `PROBE_DIE_ON_CALL=1` the probe exits without answering the first `tools/call`.

- The model got `Connection closed` as the tool's result.
- **On the next call, Claude Code started the server again by itself**, with the same command line (a new process id in the probe's log), and sent the new call to it. The second probe died the same way.

Consequence for norn: a server started as `norn mcp --repo X --policy P` that is killed and restarted by the client must **continue the same execution**, not copy the repository again. An execution id given on the command line cannot work, because the client's configuration is written before the id exists.

## E3b: how the client stops the server at the end of a session

With `PROBE_SLOW_EXIT=1` the probe catches signals and stays alive. Claude Code's debug log and the probe's log agree:

```
22:45:20.799  Sending SIGINT to MCP server process
22:45:20.899  SIGINT failed, sending SIGTERM to MCP server process
22:45:21.300  SIGTERM failed, sending SIGKILL to MCP server process
```

**SIGINT, 100 ms later SIGTERM, 400 ms later SIGKILL. Standard input is not closed first** (the probe never saw end of file). A server gets about half a second; writing a patch can take longer. So the end of a session cannot be the moment the execution is finished: norn has to treat it as what it is, a stop, which D-106 already handles like `kill -9`.

## What this changed

D-140 (both protocol revisions), D-141 (a session's end does not finish the execution; a restart continues it; `norn finish`), D-142 (tool names on the wire).
