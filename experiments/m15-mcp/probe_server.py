#!/usr/bin/env python3
"""E1/E3 probe: the smallest MCP server over stdio that offers one tool.

It logs every line it receives and every line it sends to the file named by
PROBE_LOG, and answers both protocol eras: `initialize` (2025-11-25) and
`server/discover` (2026-07-28). The tool name has a dot in it, to see whether
the client accepts dots. With PROBE_DIE_ON_CALL=1 it exits without answering
the first tools/call (experiment E3).
"""
import json, os, sys, time

LOG = open(os.environ.get("PROBE_LOG", "probe.log"), "a", buffering=1)
TOOL = {
    "name": "text.upper",
    "description": "Returns the given text in upper case.",
    "inputSchema": {"type": "object", "properties": {"text": {"type": "string"}}, "required": ["text"], "additionalProperties": False},
}

def log(direction, obj):
    LOG.write(f"{time.time():.3f} {direction} {obj}\n")

def send(msg):
    line = json.dumps(msg)
    log("OUT", line)
    sys.stdout.write(line + "\n")
    sys.stdout.flush()

log("START", f"argv={sys.argv} pid={os.getpid()}")
if os.environ.get("PROBE_SLOW_EXIT") == "1":
    # E3b: how long does the client wait after its first signal, and what comes next?
    import signal
    def noted(sig, _frame):
        log("SIGNAL", signal.Signals(sig).name)
    for s_ in (signal.SIGINT, signal.SIGTERM, signal.SIGHUP):
        signal.signal(s_, noted)
for raw in sys.stdin:
    log("IN ", raw.rstrip("\n"))
    try:
        msg = json.loads(raw)
    except ValueError:
        continue
    method, mid = msg.get("method"), msg.get("id")
    if mid is None:
        continue  # a notification: nothing to answer
    params = msg.get("params") or {}
    if method == "initialize":
        asked = params.get("protocolVersion")
        version = asked if asked in ("2025-11-25", "2025-06-18", "2025-03-26") else "2025-11-25"
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "protocolVersion": version, "capabilities": {"tools": {}},
            "serverInfo": {"name": "probe", "version": "0.0.1"}}})
    elif method == "server/discover":
        send({"jsonrpc": "2.0", "id": mid, "result": {
            "resultType": "complete", "supportedVersions": ["2026-07-28"], "capabilities": {"tools": {}},
            "_meta": {"io.modelcontextprotocol/serverInfo": {"name": "probe", "version": "0.0.1"}},
            "ttlMs": 0, "cacheScope": "private"}})
    elif method == "tools/list":
        send({"jsonrpc": "2.0", "id": mid, "result": {"tools": [TOOL], "resultType": "complete", "ttlMs": 0, "cacheScope": "private"}})
    elif method == "tools/call":
        if os.environ.get("PROBE_DIE_ON_CALL") == "1":
            log("DIE", "exiting without an answer")
            os._exit(1)
        args = params.get("arguments") or {}
        if params.get("name") != "text.upper":
            send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32602, "message": f"Unknown tool: {params.get('name')}"}})
        elif args.get("text") == "fail":
            send({"jsonrpc": "2.0", "id": mid, "result": {"content": [{"type": "text", "text": "refused: the policy does not allow this"}], "isError": True, "resultType": "complete"}})
        else:
            send({"jsonrpc": "2.0", "id": mid, "result": {"content": [{"type": "text", "text": str(args.get("text", "")).upper()}], "isError": False, "resultType": "complete"}})
    elif method == "ping":
        send({"jsonrpc": "2.0", "id": mid, "result": {}})
    else:
        send({"jsonrpc": "2.0", "id": mid, "error": {"code": -32601, "message": f"Method not found: {method}"}})
log("EOF", "stdin closed")
if os.environ.get("PROBE_SLOW_EXIT") == "1":
    time.sleep(30)
    log("EXIT", "after 30 s")
