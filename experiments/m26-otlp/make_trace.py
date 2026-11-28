"""E4 (v4, M26): write OTLP/JSON the way norn would, in a few variants.

Usage: python3 -I make_trace.py <variant>   (prints the JSON)
  good          one execution span, two call spans whose parent is the execution
  numbers       times as JSON numbers instead of strings
  upper         ids in upper-case hex
  base64        ids in base64 (the protobuf JSON mapping) instead of hex
  orphan        a call span whose parent is not in the file
"""
import base64, json, sys

V = sys.argv[1]
T = "5b8efff798038103d269b633813fc60c"     # 16 bytes: the trace
E, C1, C2 = "eee19b7ec3c1b174", "a1a1a1a1a1a1a1a1", "b2b2b2b2b2b2b2b2"
S = 1_764_360_000_000_000_000              # 2025-11-28 in Unix nanoseconds

def ident(h):
    if V == "upper":
        return h.upper()
    if V == "base64":
        return base64.b64encode(bytes.fromhex(h)).decode()
    return h

def t(ns):
    return ns if V == "numbers" else str(ns)

def attr(k, v):
    return {"key": k, "value": {"intValue": str(v)} if isinstance(v, int) else {"stringValue": v}}

def span(sid, parent, name, start, end, attrs, error=False):
    s = {"traceId": ident(T), "spanId": ident(sid), "name": name, "kind": 1,
         "startTimeUnixNano": t(S + start), "endTimeUnixNano": t(S + end),
         "attributes": [attr(k, v) for k, v in attrs.items()],
         "status": {"code": 2, "message": "denied"} if error else {"code": 0}}
    if parent:
        s["parentSpanId"] = ident(parent)
    return s

spans = [
    span(E, None, "execution", 0, 9_000_000_000, {"norn.execution_id": "20261128-000000000-abcdef", "norn.agent": "llm"}),
    span(C1, E, "fs.read", 1_000_000_000, 1_002_000_000, {"norn.call_id": 1, "norn.rule": "filesystem.read[0]"}),
    span(C2, "ffffffffffffffff" if V == "orphan" else E, "exec", 5_000_000_000, 5_900_000_000, {"norn.call_id": 2}, error=True),
]
print(json.dumps({"resourceSpans": [{"resource": {"attributes": [attr("service.name", "norn")]},
                                     "scopeSpans": [{"scope": {"name": "norn", "version": "1.3.0"}, "spans": spans}]}]}))
