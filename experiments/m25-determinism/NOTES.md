# E3 (v4, M25): does the local model give the same call again?

**Question.** Replay (R2) runs the LLM agent again and answers each call it
asks for from the recording. That is only useful if, with the same input, the
model often asks for the same call again; otherwise replay diverges at once.
With llama.cpp at temperature 0 (what norn's LLM agent sends), how often is
the answer the same?

**What was planned.** Record a whole run of the LLM agent through a
pass-through that saves each request, then send every saved request again a
few times and count where the answers first differ.

**What could be run.** The only model on this machine now is
`Qwen3.6-35B-A3B` (IQ4_NL, 19.8 GB), on 15 GB of RAM and a 6 GB GPU
(`llama-server --jinja -c 16384 -ngl 99 --cpu-moe`). It loaded in 30 s but
answered at **1.25 tokens per second**: a test reply of 200 tokens took 2 min
52 s. A whole agent run would take hours, so the planned form was dropped.
The 4B model of the earlier trials is no longer on the machine.

The small form, `same_request.py`: one request (a system message, a task, two
tools, `temperature: 0`, `max_tokens: 80`, thinking switched off) sent three
times with llama.cpp's prompt cache on and three times with it off.

```sh
llama-server -m <model>.gguf --jinja --port 8093 -c 16384 -ngl 99 --cpu-moe
python3 -I same_request.py http://127.0.0.1:8093/v1 3
```

**What happened** (2026-11-28, llama.cpp build 10408):

| Prompt cache | Answers | Same? |
|---|---|---|
| on | `fs_read {"path": "tests/test_math.py"}` three times (108 s, 37 s, 32 s) | 3 of 3 |
| off | the same call three times (56 s, 48 s, 61 s) | 3 of 3 |

**What it taught.**

- With one short request, the answer was the same six times out of six,
  with the cache on or off. It is one request with an obvious first step, so
  it shows that replay *can* match. It does not show how far into a long run
  the answers stay the same. That question stays open, and only real
  replays answer it.
- So replay must not promise how far it will get. The design already
  handles this: it reports the first divergence, whatever the cause (R4). A
  replay of a scripted run is the exact test of the replay itself.
- The model of a replay can differ from the recorded one (`--model`), so the
  report must name both. The log does not record the model today: the LLM
  settings come from the environment at each start. So R1 records the
  model's name together with the token counts.
