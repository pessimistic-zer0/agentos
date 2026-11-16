# Trial with a hosted model (`nix` preset): started, then blocked by the key's limits

Date: 2026-11-16. Code: commit `52d27e3`, debug build, NixOS 26.05, kernel 7.1.8.

**This is not the trial that was planned.** The plan (D-114, item C7) is one run of a capable hosted model through each of the six tickets of `~/Temp/AgentOS_temp/inventory-service`. One ticket was run. The other five were not started, because the API key's tier cannot carry them (below). Nothing here is a claim about six tickets.

## Setup

- Provider and model: Groq, `qwen/qwen3.8-27b` (131,072-token context), through its OpenAI-compatible API. The key was given by Prince for this trial (D-058); it was passed as an environment variable at run time and is in no file (checked: `grep` for it in the execution folder finds nothing).
- Project: `inventory-service` at commit `0755692`, untouched; before any change its suite gives 3 failed, 75 passed.
- Policy, tasks: `~/Temp/AgentOS_temp/run3/` (outside this repository): the policy of the earlier trials with one more command rule and `max_tool_calls = 60`; each task is the ticket's text from `TASKS.md` plus one line on how to run the tests.
- One session by hand; a record, not a benchmark.

## What the key allows

Read from the response headers of a one-token request (`curl -D -`), the same for the three chat models tried:

| Limit | Value |
|---|---|
| Tokens per minute | 8,000 |
| Requests per day | 1,000 |

Every request carries the whole conversation so far. So the first limit means two things: a pause of up to a minute between calls once the conversation is a few thousand tokens long, and **no request larger than 8,000 tokens, ever**.

## Ticket INV-101 (one-line bug)

| Call | | Result |
|---|---|---|
| 1 | `fs.list src/inventory` | 12 entries |
| 2 | `fs.read src/inventory/pricing.py` | 3,440 bytes |
| 3 | `fs.edit src/inventory/pricing.py` | replaced at line 39 (`>` became `>=`) |
| | | **stopped by HTTP 429** after about 15 s: agentos retried for 8.5 s and gave up. Fixed by D-124, then resumed |
| 4 | `exec pytest -q` | exit 1 (the two tests of ticket INV-102 still fail) |
| 5 | `exec pytest -q tests/test_pricing.py::test_bulk_tier_applies_at_exact_threshold` | exit 0: the ticket's test passes |
| 6, 7 | `exec git stash`, `exec git diff` | denied: no matching rule |
| 8 to 10 | `fs.read` of `parsing.py`, `models.py`, `tests/test_parsing.py` | the model went on to the other failing tests; pauses of 17 to 54 s between calls, as the server asked |
| | | `STOPPED`: `HTTP 413 Request too large` three times in a row. The conversation (28 messages, about 31,000 characters in the log) no longer fits in 8,000 tokens |

Checked outside AgentOS: `agentos apply` to a fresh clone changes one line in one file, `if quantity >= tier.min_qty:`, which is the fix the solution notes name; the project's suite there gives **2 failed, 76 passed**, as the notes say for this ticket alone. The original project was untouched. `agentos status` shows the execution as `stopped`, with its patch.

So the ticket was solved at call 5. The run did not end as `completed` because the model, seeing two more failing tests, started on another ticket and ran into the size limit.

## What this showed about AgentOS

- **Found and fixed:** the runtime gave a rate-limited server 8.5 seconds; it now waits what the server's `Retry-After` header asks (D-124). After the fix the run paused nine times, 17 to 54 s each, and went on.
- **Worked as designed:** a stop by the server is resumable (`run --resume` carried on after call 3 with the conversation from the log); a request the server will not take ends the run as `stopped` with the patch of what was done (D-111); denied commands are answers, not stops; the key stays out of the execution folder.
- **Seen, not changed:** a `413` can never get better, yet the model is told three times before the run stops, which costs three requests. And nothing in AgentOS shortens a conversation that has grown too long; with a hosted model that has a per-request cap, a long task ends there. Both are noted for the parking lot.

## What is needed to do the planned trial

A key whose limit lets one request hold a whole conversation: for these tickets, well above 8,000 tokens per minute (the two-module ticket took about 31,000 characters before any larger file was read; INV-105 is about a 134 KB file). Either a higher tier for this Groq key, or a key for another OpenAI-compatible provider. Only Prince can provide that.
