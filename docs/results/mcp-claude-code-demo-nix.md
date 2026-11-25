# Claude Code fixes the demo through `norn mcp` (M15, `nix` preset)

*2026-11-26, development machine (NixOS 26.05, kernel 7.1.8), Claude Code 2.1.332 on Prince's account (D-139), norn at the M15 working tree (version 1.0.0 plus `norn mcp`).*

One session, recorded as it happened. It is a record that the pieces work together with a real client, not a measurement.

## Setup

A copy of `examples/demo-repo` and a fresh state folder in the scratch directory. Configuration:

```json
{"mcpServers": {"norn": {"command": "<repo>/target/debug/norn",
  "args": ["--state-dir", "<scratch>/state", "mcp", "--repo", "<scratch>/repo", "--policy", "<repo>/policies/nix.toml"]}}}
```

```sh
claude -p 'The repository has a failing test. Find the cause and fix it, then run the tests to confirm. Use only the norn tools.' \
  --mcp-config mcp.json --strict-mcp-config --tools "" --allowedTools mcp__norn --output-format json
```

## First attempt: Claude Code started outside the dev shell

Every tool answered `FAILED: norn could not start: allowed program `pytest` was not found on PATH`. The model reported that, suggested installing pytest or changing the policy, and changed nothing. No execution was created; the repository copy was unchanged (`diff -r`). 4 turns, 0.052 USD.

This is the client's environment, not a fault: Claude Code starts the server with its own `PATH`, and `pytest` is only on the dev shell's. The README now says so.

## Second attempt: inside the dev shell

7 turns, 10.3 s, 0.030 USD. The model's answer: the cause was `mean()` dividing by `len(values) - 1`; it changed the line; "all 4 tests now pass"; "To bring it into your actual repository, run `norn finish` and then `norn apply`."

`norn trace`:

```
  agent   mcp
  task    MCP session (claude-code)
  workspace: 4 files copied, base checkpoint cp-0000
  left out by deny rules: .env

#1   fs.list  .                                        allow  4 entries
#2   exec     pytest -q                                allow  exit 1, 0.2s  [cp-0001]
#3   fs.list  src                                      allow  1 entries
#4   fs.read  src/stats.py                             allow  554 bytes
#5   fs.edit  src/stats.py (42 bytes -> 36 bytes)      allow  replaced at line 8; file is 548 bytes  [cp-0002]
#6   exec     pytest -q                                allow  exit 0, 0.2s  [cp-0003]
```

After the session: `norn status` showed `open (`norn mcp` continues it, `norn finish` ends it) 6 calls`; `diff -r` between the demo and the copy found no difference (the run changed nothing outside the workspace).

Then, as the person:

```
$ norn finish 20261125-225616659-3eb04a
20261125-225616659-3eb04a  COMPLETED  finished with `norn finish`
$ norn apply 20261125-225616659-3eb04a
  M src/stats.py
1 change(s) made in …/repo. Look at them there before you commit.
$ python -m pytest -q        # in the repository copy
4 passed
```

The patch held the one-line change (`-    return sum(values) / (len(values) - 1)` / `+    return sum(values) / len(values)`).
