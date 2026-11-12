# Real-LLM demo run with a crash in the middle

Model: Qwen3-VL-4B-Instruct (GGUF, Q6_K_XL) served by a local llama.cpp `llama-server --jinja` on an RTX 4050. Date: 2026-11-12. Preset: `nix`.
The runtime was killed (`AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=4`) after call #4 took effect and before its checkpoint, then continued with `agentos run --resume`.

```text
execution 20261112-070906-21df29
  repo    /home/zer0/Desktop/agentos/examples/demo-repo
  policy  default-nix
  agent   llm
  task    The test suite of this Python project has a failing test. Find the bu…
  workspace: 4 files copied, base checkpoint cp-0000
  left out by deny rules: .env

#1   exec     pytest -q                                allow  exit 1, 0.2s  [cp-0001]
#2   fs.read  src/stats.py                             allow  554 bytes
#3   fs.write src/stats.py (194 bytes)                 allow  wrote 194 bytes  [cp-0002]
#4   fs.write src/stats.py (545 bytes)                 allow  RECOVERED, APPLIED: file has the new content; checkpoint cp-0003 taken  [cp-0003]
#5   exec     pytest -q                                allow  exit 0, 0.2s  [cp-0004]

finished: completed (All tests now pass. The bug in the `mean` function has been fixed by dividing by `len(values)` instead of `len(values) - 1`, which correctly calculates the arithmetic mean.)
patch: <state>/executions/20261112-070906-21df29/result.patch
```
