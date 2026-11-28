"""E3, small form: the same tool-calling request, at temperature 0, sent
several times, with and without llama.cpp's prompt cache. Are the answers
identical?

Usage: python3 -I same_request.py <server base url> <times>
"""
import json, sys, time, urllib.request

URL, N = sys.argv[1].rstrip("/") + "/chat/completions", int(sys.argv[2])
TOOLS = [
    {"type": "function", "function": {"name": "fs_read", "description": "Read a file of the repository.",
        "parameters": {"type": "object", "properties": {"path": {"type": "string"}}, "required": ["path"]}}},
    {"type": "function", "function": {"name": "exec", "description": "Run a command (argv list) in the repository.",
        "parameters": {"type": "object", "properties": {"argv": {"type": "array", "items": {"type": "string"}}}, "required": ["argv"]}}},
]
BASE = {
    "messages": [
        {"role": "system", "content": "You are a coding agent. Act only through the tools. Call exactly one tool per reply."},
        {"role": "user", "content": "The test tests/test_math.py::test_add fails. Find out why."},
    ],
    "tools": TOOLS, "temperature": 0, "max_tokens": 80,
    "chat_template_kwargs": {"enable_thinking": False},
}
for cache in (True, False):
    seen = []
    for i in range(N):
        body = dict(BASE, cache_prompt=cache)
        t = time.time()
        with urllib.request.urlopen(urllib.request.Request(URL, data=json.dumps(body).encode(), headers={"Content-Type": "application/json"}), timeout=1800) as r:
            m = json.load(r)["choices"][0]["message"]
        out = json.dumps({"content": m.get("content"), "tool_calls": [c["function"] for c in m.get("tool_calls") or []]}, sort_keys=True)
        seen.append(out)
        print(f"cache_prompt={cache} #{i + 1} {time.time() - t:.0f} s  {out[:150]}", flush=True)
    print(f"cache_prompt={cache}: {len(set(seen))} different answer(s) in {N}", flush=True)
