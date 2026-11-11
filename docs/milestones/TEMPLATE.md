# M<n> — <Milestone name>

| | |
|---|---|
| Weeks in the plan | |
| Started / finished | YYYY-MM-DD / YYYY-MM-DD |
| Commits | `<first hash>` … `<last hash>` (tag `m<n>`) |
| Status | Done / Done with gaps / Blocked |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it. Fill it in while working.

## 1. Aim

Two to four sentences: what this milestone adds and why the project needs it. Say which of the five parts (gateway, policy, sandbox, event log, checkpoints + recovery) it touches.

## 2. Requirements

Copy the exit criterion from the v1 milestone table, then break it into checks. Every row needs evidence that can be re-run.

| Requirement | Met? | Evidence (command and result) |
|---|---|---|

Anything not met goes here with the reason. Do not mark a requirement met without evidence.

## 3. What you need to know first

The ideas this milestone depends on, each in a short paragraph, in the order they are needed. Link to glossary terms in `docs/decision-log.md`.

## 4. Experiments

The small throwaway programs run before building, from `experiments/`.

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|

## 5. What was built

A map of the files added or changed and what each one is for.

| File | Purpose |
|---|---|

Then a short description of how the pieces talk to each other. Use a small diagram if it helps.

## 6. The code, explained

Go through the code in the order a reader should meet it, not alphabetically. For each piece:

- the file and function name;
- a short excerpt of the important lines (not the whole file);
- what it does, line by line where it is not obvious;
- why it is written this way, and what would go wrong if it were written the obvious way;
- any Rust feature that appears for the first time, explained once.

## 7. Decisions made

Every decision-log entry created during this milestone, with a one-line summary. The full reasoning stays in the log.

| Entry | Decision | Discarded alternatives |
|---|---|---|

## 8. Problems hit

Bugs, wrong turns and surprises, in the order they happened. For each: what was expected, what happened, how it was found, how it was fixed, and the commit. These also go in the "Findings" table of the decision log.

## 9. How to run it

Exact commands from a fresh clone to seeing this milestone work, with the output to expect.

## 10. What this milestone does not do

Known gaps and limits, and which later milestone covers each one, if any.

## 11. Check your understanding

Eight to twelve questions that Prince should be able to answer after studying this milestone, from basic ("what does X do?") to interview level ("why not Y?"). No answers here; they are in the sections above.
