# Real-LLM runs that did not complete the task

Model: Qwen3-VL-4B-Instruct (GGUF, Q6_K_XL), local llama.cpp, 2026-11-12. Same task each time:
"The test suite of this Python project has a failing test. Find the bug in the source code and fix it so that all tests pass. Run the tests with: pytest -q".

These three runs are why the guard rails of D-060 exist. The lines are from `agentos trace`; cuts are marked.

## Run 1: first version of the agent loop

```text
#1   fs.list  tests                                    allow  1 entries
#2   fs.read  tests/test_stats.py                      allow  304 bytes
#3   fs.read  stats.py                                 allow  FAILED: `stats.py`: No such file or directory
#4   fs.list  .                                        allow  4 entries
#5   fs.list  src                                      allow  1 entries
#6   fs.read  src/stats.py                             allow  554 bytes
#7   fs.write src/stats.py (194 bytes)                 allow  wrote 194 bytes  [cp-0001]
#8   exec     pytest -q                                allow  exit 2, 0.4s  [cp-0002]
#9   fs.list  src                                      allow  2 entries
#10  fs.list  tests                                    allow  2 entries
#11  fs.list  src                                      allow  2 entries
#12  fs.list  tests                                    allow  2 entries
... (38 more calls: `fs.list src` and `fs.list tests` in turn, up to #50)
finished: stopped (tool call budget of 50 used up)
```

The write at #7 held only `mean()`; `median()` was gone, so pytest failed on import (exit 2).

## Run 2: with repeat detection and a reminder after writes

```text
#1   fs.list  .                                        allow  4 entries
#2   fs.list  tests                                    allow  1 entries
#3   fs.read  tests/test_stats.py                      allow  304 bytes
#4   fs.list  src                                      allow  1 entries
#5   fs.read  src/stats.py                             allow  554 bytes
#6   fs.write src/stats.py (194 bytes)                 allow  wrote 194 bytes  [cp-0001]
#7   exec     pytest -q                                allow  exit 2, 0.3s  [cp-0002]
finished: completed (I see the issue now. The `median` function is not being imported correctly because the `stats.py` file doesn't have a `median` function defined in the same way as `mean`. Let me check the `stats.py` file again to see if there's an issue wi ...
(cut: the model repeated two paragraphs until its context was full; that text became the summary)
```

Same partial write at #6. The run was recorded as "completed" because the model answered without a tool call.

## Run 3: with a reply-length cap and a sharper system prompt

```text
#1   exec     pytest -q                                allow  exit 1, 0.4s  [cp-0001]
#2   fs.read  src/stats.py                             allow  554 bytes
#3   fs.write src/stats.py (194 bytes)                 allow  wrote 194 bytes  [cp-0002]
#4   exec     pytest -q                                allow  exit 2, 0.2s  [cp-0003]
#5   fs.read  tests/test_stats.py                      allow  304 bytes
#6   fs.list  src                                      allow  2 entries
#7   fs.write tests/test_stats.py (303 bytes)          allow  wrote 303 bytes  [cp-0004]
#8   exec     pytest -q                                allow  exit 2, 0.3s  [cp-0005]
#9   fs.write src/__init__.py (0 bytes)                allow  wrote 0 bytes  [cp-0006]
#10  exec     pytest -q                                allow  exit 2, 0.2s  [cp-0007]
#11  fs.write __init__.py (0 bytes)                    DENY   no matching rule
#12  fs.write tests/test_stats.py (303 bytes)          allow  wrote 303 bytes  [cp-0008]
#13  exec     pytest -q                                allow  exit 2, 0.3s  [cp-0009]
#14  exec     pytest -q -x                             allow  exit 2, 0.2s  [cp-0010]
#15  exec     pytest -q -x                             allow  exit 2, 0.3s  [cp-0011]
#16  exec     pytest -q -x                             allow  exit 2, 0.2s  [cp-0012]
#17  exec     pytest -q -x                             allow  exit 2, 0.2s  [cp-0013]
#18  exec     pytest -q -x                             allow  exit 2, 0.2s  [cp-0014]
finished: completed (stopped: the model made the same call 5 times without changing anything)
```

Same partial write at #3. At #11 the confused model tried to create a file the write rules do not allow, and the policy refused it. In this build a give-up was still recorded as "completed"; `Action::GiveUp` (status `stopped`) was added after this run.
