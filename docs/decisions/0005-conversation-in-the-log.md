# ADR 0005 — The agent's conversation lives in the event log

**Status:** accepted · **Log entries:** D-017, D-056, D-059

## Question
The model's memory of the task exists only in the runtime's RAM. How does it survive a crash?

## Decision
Every message to and from the model is appended to the log as an `agent.message` event, before the call it leads to. On restart the conversation is rebuilt from those events, and the cut-off call's result is recovery's verdict (APPLIED, or NOT APPLIED with the reason).

## Why
One source of truth. A separate "agent state" file could disagree with the log after a crash; a log cannot disagree with itself.

## Consequences
- A request to the model that was cut off is simply sent again. The answer may differ. Accepted: a model is not deterministic anyway, and nothing had been done on the lost answer.
- The scripted agent has no conversation; its position is rebuilt by replaying the recorded results.
- Whatever the agent reads ends up in the log. Denied files never enter the workspace, so they never enter the log.

## Evidence
`tests/llm_agent.rs` (mock endpoint) and a real-model run killed at P4 and resumed (`docs/results/llm-demo-qwen3-vl-4b.md`).
