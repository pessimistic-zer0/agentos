# E2 (v4, M24): what the kept trial logs can answer today

**Question.** Which TraceWeaver questions can the logs of the earlier trials
answer without any new field, and which need the `cause` link (C1) or token
counts (R1)?

**How.** The trials' state folders are kept outside this repository, in
`~/Temp/AgentOS_temp/` (32 executions: scripted, LLM through llama.cpp, LLM
through the relay of M13, and Claude Code through MCP). `probe.py` reads a log
read-only and prints where the time went, what links each call to the message
before it, whether token counts appear, and whether each changing call's
checkpoint is still on disk. Then two runs of the same ticket were lined up by
their calls (Python's `difflib`, longest common subsequence).

```sh
cd ~/Temp/AgentOS_temp
python3 -I <repo>/experiments/m24-trial-logs/probe.py run/state-m9-llm/executions/20261115-071323300-f58e78 \
  run4/INV-104/state/executions/20261116-204215472-43eeb7 run5/INV-104/state/executions/20261125-232824181-db3045 \
  run/state-m9/executions/20261115-071302549-186918
```

**What happened** (2026-11-28):

| Run | Calls | Time in tools / between calls | Calls with an assistant message just before | Linked by an id | Token counts | Checkpoints on disk |
|---|---|---|---|---|---|---|
| M9, local model (llama.cpp) | 7 | 0.8 s / 75.4 s | 7, 6 with text | 0 | none | 4/4 |
| M13, relay, INV-104 | 19 | 2.8 s / 39,396.5 s | 19, 0 with text | 0 | none | 10/10 |
| M18, Claude Code through MCP, INV-104 | 22 | 2.8 s / 33.8 s | 0 (one `agent.message`: the session's tool list) | 0 | none | 11/11 |
| M9, scripted | 18 | 2.3 s / 0.1 s | 0 | 0 | none | 11/11 |

Lining up the two INV-104 runs (relay vs Claude Code): 19 and 22 calls, 7 the
same and in the same order; the rest show as inserted or replaced calls.

**What it taught.**

1. **"Why did the agent make this call?"** For the LLM agent, the assistant
   message that holds the call is always the event just before
   `tool.requested` (`drive` stores messages first, ADR 0005, and the stored
   message keeps exactly one tool call). The model's own id for the call
   (`tool_calls[0].id`) is in that message and in the tool's answer
   (`tool_call_id`), but **not** in `tool.requested`. So today the link is
   only by order, which spec §9.13 rules out. C1 is needed as planned. Old
   logs show "cause not recorded".
2. **Through MCP nothing explains a call.** The client's reasoning never
   reaches norn. C1 can record only the client's JSON-RPC request id, which
   names the request, not a reason. The trace must say so plainly.
3. **The relay messages have no text**: the model's reasoning is not in the
   log, only its call. Explaining a call can only be as good as what the
   model wrote.
4. **Time.** In every LLM or MCP run the tools take a few seconds, and the
   agent takes the rest: a timeline must show the agent's time between calls,
   not only each call's own time. The relay run shows **39,396 s** between
   calls: a 10-minute wait for the model server, then a stop, then
   `norn run --resume` **10.7 hours later**. Nothing in the log marks the stop
   or the resume (`agent.message` 93 at 20:55, the next at 07:39 the next
   day). A gap looks like a slow model. Leads to D-170: a resume is recorded.
5. **No token counts anywhere** (R1 is needed). The LLM agent drops the
   reply's `usage` when it keeps `choices[0].message`.
6. **Per-call diffs work on every kept run**: each `tool.completed` of a
   changing call names a checkpoint, and every named checkpoint is on disk
   (`norn rm` alone removes them). C4 needs no new field.
7. **Run diff by longest common subsequence of `(tool, arguments)`** gives a
   readable result even for two very different agents.
