# M13 — Evidence with a capable hosted model

| | |
|---|---|
| Weeks in the plan | v1.0 (D-114); no week plan |
| Started / finished | 2026-11-16 / not finished |
| Commits | `52d27e3` … |
| Status | **Blocked**: the API key's tier allows 8,000 tokens per minute, which is also the largest single request; the six-ticket run cannot be made with it. One ticket was run |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it. Fill it in while working.

## 1. Aim

Everything real so far was done by a script, by a 4-billion-parameter local model, or by Claude acting as the agent. This milestone asks what happens when a capable hosted model works through the six tickets of the inventory project inside AgentOS: which tickets it solves, what the policy stops, and what the runtime gets wrong when the model is on the other side of the internet. It touches no part of the runtime by plan; what it finds may.

## 2. Requirements

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| One run of a capable hosted model through each of the six tickets, each result checked on a fresh clone against the solution notes | **No.** One of six | `docs/results/hosted-model-trial-nix.md`: INV-101 solved (one-line fix, 2 failed / 76 passed on a clone, as the notes say); the other five not started |
| A credential given by Prince for this project, used and stored nowhere (D-058) | Yes | The key was passed in the environment; `grep` for it in the execution folder finds nothing |
| What the trial finds in the runtime is fixed or parked | Yes, so far | D-124 (wait as long as the server asks), with tests; two observations parked |

**Why it is not met.** The key's tier limits a minute to 8,000 tokens (measured from the response headers). Each request carries the whole conversation, so a conversation over 8,000 tokens cannot be sent at all: the server answers `413 Request too large`. The first ticket's run reached that at call 10. This needs a key with a higher limit, which only Prince can provide. The milestone stays open; no result is filled in for the five tickets that were not run.

## 3. What you need to know first

**Tokens.** A model reads and writes text in pieces called tokens, roughly three to four characters each. Hosted APIs count them for limits and for billing.

**The conversation is sent every time.** The model keeps nothing between requests. Each request contains the system prompt, the tool descriptions and every message and tool result so far. A run's tenth request is much larger than its first.

**Rate limit, 429 and 413.** A provider caps tokens per minute. Over the cap, it answers `429 Too Many Requests` and, in the `Retry-After` header, how many seconds to wait. A single request larger than the whole cap gets `413 Request too large`: waiting does not help.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| (by `curl`, not kept as a program) | A one-token request to each of three chat models, reading the rate-limit headers | `x-ratelimit-limit-tokens: 8000`, `x-ratelimit-limit-requests: 1000` for all three | The limit belongs to the key's tier, not to the model; changing the model does not help |

## 5. What was built

| File | Purpose |
|---|---|
| `src/agent/llm.rs` | A busy server's `Retry-After` is used as the pause, up to 90 s; a longer one ends the run at once, resumably (D-124) |
| `tests/llm_agent.rs` | The mock can send `Retry-After`; two tests |
| `docs/results/hosted-model-trial-nix.md` | The record of what was run |

## 6. The code, explained

**`LlmAgent::complete`.** It makes up to four attempts. `complete_once` now also returns the wait the server named, read from the `retry-after` header as a number of seconds. Between attempts: if the server named a wait of at most 90 s, sleep that long plus half a second (and say so on the screen from 5 s up); if it named a longer one, stop now with the usual "unfinished, resume later" error; if it named none, use the old fixed pauses (0.5 s, 2 s, 6 s), which suit a local server that is still loading its model.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-124 | Wait what `Retry-After` asks, capped at 90 s | Longer fixed pauses; parsing the error text; waiting without a cap; pacing requests on the client |

## 8. Problems hit

1. **Stopped after three calls by a 429.** The runtime's pauses added up to 8.5 s; the server wanted up to a minute. Fixed (D-124); the run was resumed from the log.
2. **Stopped at call 10 by a 413.** The conversation outgrew the key's per-minute cap. Not fixable in the runtime without a new mechanism (shortening a conversation), which is outside the v1.0 scope and parked. This is what blocks the milestone.
3. **The first version of the new test would have passed with the old code.** It checked only the total time. The mock now refuses any request that comes back sooner than it asked, and the test requires exactly one refusal.

## 9. How to run it

```sh
export AGENTOS_LLM_BASE_URL=https://api.groq.com/openai/v1
export AGENTOS_LLM_MODEL=qwen/qwen3.8-27b
export AGENTOS_LLM_API_KEY=...        # never in a file in this repository
agentos run --repo <project> --policy <policy> --agent llm --task "$(cat task.txt)"
```

## 10. What this milestone does not do

- It has not run five of the six tickets. No claim is made about them.
- One run of one model is a record, not a measurement.

## 11. Check your understanding

(Filled in when the milestone is finished.)
