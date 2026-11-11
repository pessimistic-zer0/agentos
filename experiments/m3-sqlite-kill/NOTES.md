# m3-sqlite-kill

**Tried:** a writer inserts rows into SQLite (WAL mode, `synchronous=FULL`),
three rows per transaction with a pause between them, printing the row count
after each COMMIT returns. It is killed with SIGKILL after a random 30–120 ms,
200 times, on the same database file. After each kill the database is
reopened and checked. Run:
`nix develop -c python3 experiments/m3-sqlite-kill/run.py 200`.

**Showed (2026-11-11):**

```text
200 kills, 7476 rows at the end, 0 violations (corrupt db, gap, half transaction visible, or acknowledged commit lost)
```

**Taught:**

- After a kill in the middle of a transaction, none of that transaction's
  rows are visible: the row count was always a multiple of 3.
- Rows are a gap-free run 1..n every time, `integrity_check` passes, and no
  commit the writer had reported was ever missing.
- So "append one event per transaction" gives the event log invariant 5
  (gap-free prefix, no torn records) without any code of ours. This is the
  evidence D-005 was waiting for.
- Same limit as above: this is a process kill, not a power cut.
