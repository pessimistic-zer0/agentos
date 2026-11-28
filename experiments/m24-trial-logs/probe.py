"""E2 (v4, M24): what can the kept trial logs answer today, without new fields?

Usage: python3 -I probe.py <execution folder>...
Reads each log read-only and prints, per run:
  - where the time went: model time (previous ending -> next request) vs tool time
  - for each call, the agent message just before it, and whether an id links them
  - whether token counts are anywhere in the log
  - whether the checkpoint before and after each changing call is on disk
"""
import json, os, sqlite3, sys
from datetime import datetime

def ts(s):
    return datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()

for d in sys.argv[1:]:
    c = sqlite3.connect(f"file:{d}/log.db?mode=ro", uri=True)
    rows = [(s, t, ty, cid, json.loads(b)) for s, t, ty, cid, b in c.execute("select seq, ts, type, call_id, body from events order by seq")]
    agent = next(b["agent"] for *_, b in rows if b["type"] == "execution.created")
    req = {cid: (s, ts(t)) for s, t, ty, cid, b in rows if ty == "tool.requested"}
    end = {cid: ts(t) for s, t, ty, cid, b in rows if ty in ("tool.completed", "tool.failed", "tool.denied", "approval.requested")}
    tool_time = sum(end[k] - req[k][1] for k in req if k in end)
    model_time, prev = 0.0, None
    for k in sorted(req):
        if prev is not None and prev in end:
            model_time += req[k][1] - end[prev]
        prev = k
    # The message right before each request, and whether it names the call by an id.
    linked_by_order = linked_by_id = with_text = 0
    for k, (s, _) in req.items():
        before = [b for s2, _, ty, _, b in rows if s2 < s and ty == "agent.message"]
        if not before:
            continue
        m = before[-1]["message"]
        if isinstance(m, dict) and m.get("role") == "assistant":
            linked_by_order += 1
            with_text += bool((m.get("content") or "").strip())
            # The call itself records no tool_call id, so nothing to match on.
            call = next(b["call"] for s2, _, ty, cid, b in rows if ty == "tool.requested" and cid == k)
            linked_by_id += any(key in call for key in ("tool_call_id", "id", "cause"))
    tokens = sum('"usage"' in json.dumps(b) or "prompt_tokens" in json.dumps(b) for *_, b in rows)
    cps = [b.get("checkpoint") for *_, ty, _, b in rows if ty == "tool.completed" and b.get("checkpoint")]
    on_disk = sum(os.path.isdir(f"{d}/checkpoints/{cp}") for cp in cps)
    print(f"{os.path.basename(d)} agent={agent} calls={len(req)}")
    print(f"  time: tools {tool_time:.1f} s, agent between calls {model_time:.1f} s")
    print(f"  calls preceded by an assistant message: {linked_by_order}/{len(req)}; with text: {with_text}; linked by a recorded id: {linked_by_id}")
    print(f"  events mentioning token counts: {tokens}")
    print(f"  changing calls with their checkpoint on disk: {on_disk}/{len(cps)}")
