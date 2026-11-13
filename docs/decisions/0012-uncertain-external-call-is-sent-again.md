# ADR 0012 — An uncertain external call is sent again with the same key

**Status:** accepted (bonus, after v0.1) · **Log entries:** D-077, D-078, D-079

## Question
The runtime died, or got no answer, while a request to an outside service was in flight. The service may have done the work or not, and a checkpoint cannot undo it. How is the call settled?

## Decision
Recovery sends the *same* request again with the *same* idempotency key, both read from the intent. A 2xx answer settles the call as APPLIED; a 4xx answer as NOT APPLIED. No answer settles nothing: recovery stops with an error and the execution stays interrupted until recovery is run again and gets an answer.

## Why
ADR 0003 rolls an uncertain `exec` back, and said to look again when an effect leaves the workspace. Rolling back is impossible here, and "ask the service whether it did it" is not safe either: the first request may still be on its way, so "no" can become "yes" a moment later. Sending again with the same key has no such gap. The service applies a key at most once, so afterwards the effect has happened exactly once, whichever of the two requests did it.

Leaving the call uncertain when the service is silent is the price of never guessing (design rule 5). A guess of "not applied" lets the agent ask again under a new key and do the thing twice; a guess of "applied" can lose it.

## How
The key is `agentos-<execution id>-c<call id>`, computed, not random, and written to the intent before the request. The payload is in the blob store under the hash the log names. In a live run the gateway makes up to `external.attempts` requests itself; if none is answered it stops the run with the intent open, which is the same state a crash leaves.

## What this relies on
The service must honour idempotency keys. AgentOS cannot check that from outside. With a service that ignores the key, a retry is a second effect.

## Evidence
`tests/external.rs` (outage, lost answer, kill after the intent) and the external cells of the fault-injection harness (`docs/results/`).

## Look again if
A real service is added that has no idempotency keys, or whose effects need a human's approval before a retry.
