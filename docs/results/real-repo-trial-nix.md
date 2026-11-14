# First trial on a project that is not the demo (`nix` preset)

Date: 2026-11-14. Code: commit `9c7d434` (debug build), NixOS 26.05, kernel 7.1.8, btrfs.

Everything before this ran on `examples/demo-repo` (4 files). This is the first run on something shaped like a real project. It was done by hand; there is no script that repeats it, so the numbers here are a record of one session, not a benchmark.

## The project

`~/Temp/AgentOS_temp/inventory-service`, written for this trial by a separate AI agent (Claude Opus, as a subagent) that was told to build an ordinary project and not to look at AgentOS. That agent stopped at its usage limit just before its final report; the repository was complete and was checked by hand.

- Python, `src/` layout, 11 modules, 78 tests, a command line tool.
- Real packages from PyPI in a virtualenv inside the repo (`.venv`, 35 MB, 2,700 files): attrs, click, PyYAML, python-dateutil, pytest.
- A `.env` with fake secrets, a `.env.example`, a YAML config, a 134 KB CSV fixture, a 251-byte gzip fixture, an executable script, a git history.
- Six tickets in `TASKS.md`; the expected solutions were kept outside the repo.
- Before any change: 3 tests fail on purpose, 75 pass.

## The policy a user would write

A copy of `policies/nix.toml` with: `write = ["src/**", "tests/**", "config/**", "scripts/**"]`, the virtualenv's folder added to `sandbox.ro_mounts`, and `agentos` started with the virtualenv activated.

Second version, after the first run: `.venv/**` added to `deny` (so it is not copied; it is still visible through the mount), `["python3", "-m", "inventory"]` added to `exec.allow`, and `PYTHONPATH = "/work/src"` in `[sandbox.env]`.

## What was run and what happened

| # | Run | Result |
|---|---|---|
| 1 | `pytest -q`, virtualenv mounted but **not activated** | exit 4 from the system's pytest in 0.2 s. No hint that the virtualenv was not in use |
| 2 | The same, activated | `3 failed, 75 passed`, the same as outside the sandbox. 2,779 files copied; 3.1 s for three calls; 140 MB for the execution (2 checkpoints) |
| 3 | 18 calls written by Claude acting as the agent (second policy): read the tickets, fix INV-101, INV-102 and INV-104, add the package for INV-103 with a test, start INV-106 | 73 files copied; 1.5 s; 8.3 MB. `pytest -q` in the sandbox: **79 passed**. The quote command showed EUR prices and 19 % tax for the Berlin order |
| 4 | The patch of run 3 applied to a fresh clone with `git apply`, tests run there | 79 passed. **`src/inventory/alerts/__init__.py` was missing** (bug B-012) |
| 5 | LLM agent (Qwen3-VL-4B, local `llama-server`, 8,192-token context), ticket INV-101 | Solved in 5 calls and 33 s. Call 3 wrote only the changed function (298 bytes); the runtime's dropped-lines warning made the model send the whole file again in call 4. The patch has the right one-character fix, and also drops the file's last newline |
| 6 | The same model, ticket INV-102 (two modules) | The model ran the tests once, wrote a sentence about what it would need to change, and stopped. The run is shown as `COMPLETED` with an empty patch |

## What got in the way (run 3, call by call)

| Call | Result | What it means for a user |
|---|---|---|
| `fs.read .env.example` | DENY by `.env.*` | The example file is the one an agent *should* read. The shipped deny glob is too wide for it |
| `fs.read` of the 134 KB fixture | FAILED: over `max_output_kb` | No way to read part of a file. Ticket INV-105 is about that file. Its content was still reachable through the project's own import command, which printed the bad rows |
| `fs.read` of the gzip fixture | FAILED: not UTF-8 | Binary files cannot be read, and `fs.write` takes text, so they cannot be written either |
| `fs.write pyproject.toml` | DENY: no matching rule | Correct for this policy. The user has to think of every file outside `src/` that a task may touch |
| `exec python3 scripts/regen_snapshot.py` | DENY | Correct. To allow it the policy needs the exact rule `["python3", "scripts/regen_snapshot.py"]`; allowing `["python3"]` would allow everything. So ticket INV-106 (a binary fixture and an executable script) cannot be done under this policy |
| `exec pytest -q` | exit 0, `2 change(s) outside write globs reverted` | The two were pytest's own `.pytest_cache` files. A harmless cache makes every test run report a policy violation |
| `exec git status` | DENY | Correct; and `.git` is not in the workspace anyway |
| New script in `scripts/` | written, mode 644 | `fs.write` cannot make a file executable |

## What it showed

**Works:** packages from a virtualenv (D-091), policy variables (D-092), folder-making writes (D-084), the patch as the result, denial as an answer instead of a stop. A real ticket was solved by a 4-billion-parameter local model in 33 seconds.

**Bug found and fixed:** B-012, empty new files missing from the patch.

**Not fixed; in the parking lot (spec §22):** no hint when a mounted virtualenv is not the one in use; `.env.*` denies `.env.example`; no partial read of large files; no binary reads or writes; caches such as `.pytest_cache` reported as violations; a model that stops with words is recorded as `COMPLETED`.

**Worth knowing:** putting `.venv/**` in `deny` while mounting the same folder read-only cut the copy from 2,779 files to 73 and the execution's size from 140 MB to 8.3 MB.
