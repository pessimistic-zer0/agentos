# E1 (v4, M23): reading the event log while a run writes it

**Question.** Can `norn trace` (a second process) read an execution's SQLite
log read-only while the run is still appending to it, without slowing the run,
failing, or seeing half an event?

**How.** `src/main.rs`, with the same `rusqlite` (0.42, bundled SQLite) and the
same settings as `src/log.rs`: WAL, `synchronous=FULL`, one `INSERT` per event,
a body of about 600 bytes. The reader opens with `SQLITE_OPEN_READ_ONLY` in a
loop and checks that `seq` runs 1..=max with no gap and every body is whole.

```sh
nix develop -c bash -c 'cd experiments/m23-readonly-log && cargo build --release'
B=./target/release/m23-readonly-log
$B write a.db 2000                       # the writer alone
$B write b.db 1; $B read b.db 30 & $B write b.db 10000   # with a reader
$B once c.db                             # one read-only open
```

**What happened** (NixOS, 2026-11-28; run once, so the times are a rough look,
not a record):

| Case | Result |
|---|---|
| Writer alone, 2,000 appends | p50 1.81 ms, p99 3.00 ms per append |
| Writer with a reader looping, 10,000 appends | p50 1.57 ms, p99 3.27 ms. The reader made 24,264 full reads: 0 errors, 0 gaps or half bodies, the count never went back. The WAL file grew past 1,000 pages, so SQLite's automatic checkpoints ran during the reads |
| After a clean close (no `-wal`, `-shm` left) | Read-only open works |
| Writer killed with `kill -9` (`-wal`, `-shm` left) | Read-only open works and sees the 784 committed rows |
| The folder made read-only, `-wal`/`-shm` present | Read-only open works |
| The folder made read-only, a log closed cleanly (no `-wal`, `-shm`) | **Refused**: `attempt to write a readonly database` (`SQLITE_READONLY_DIRECTORY`). Found later, while building M23: the first run of this table said "works", because the read of the row above it had left `-wal` and `-shm` files behind. Repeated alone, it fails |
| No file | "unable to open database file"; nothing is created |

**What it taught.**

- A read-only reader beside the writer is safe and does not slow it down. WAL
  gives each read a fixed snapshot, so a reader never sees half an event.
- `EventLog::open` as it is today is not a reader: it sets the journal mode,
  runs `CREATE TABLE IF NOT EXISTS` and may insert the schema version. So
  `norn trace` needs its own read-only open (T6), which checks the schema
  version and writes nothing. A missing log then fails without making a file.
- A read-only connection still needs the `-shm` file to read a WAL
  database, and creates it if it is missing. In a folder it cannot write,
  that fails. When there is no `-wal` file either, every committed event is
  in the main file and no writer is active, so the reader retries with
  `immutable=1`, which skips the shared memory (D-172).
- A case that passes once may pass for a reason left behind by the case
  before it. Repeat each case alone in a fresh folder.
