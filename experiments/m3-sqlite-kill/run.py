#!/usr/bin/env python3
"""Kill a process with SIGKILL while it inserts rows into SQLite (WAL, synchronous=FULL).

The writer inserts rows 1, 2, 3, ... Each transaction inserts THREE rows
(seq 3k+1 .. 3k+3) with a short pause between them, so most kills land in the
middle of a transaction. After each kill, reopen and check:
  - the database opens and passes integrity_check,
  - the rows are exactly 1..n with no gap,
  - n is a multiple of 3 (no half transaction is visible),
  - n is at least the last number the writer said it had committed.
"""
import os, random, signal, sqlite3, subprocess, sys, tempfile, time

WRITER = r'''
import sqlite3, sys, time
db = sqlite3.connect(sys.argv[1], isolation_level=None)
db.execute("PRAGMA journal_mode=WAL"); db.execute("PRAGMA synchronous=FULL")
db.execute("CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY, body TEXT)")
n = db.execute("SELECT COALESCE(MAX(seq),0) FROM events").fetchone()[0]
while True:
    db.execute("BEGIN")
    for _ in range(3):
        n += 1
        db.execute("INSERT INTO events VALUES (?, ?)", (n, "x" * 200))
        time.sleep(0.0005)
    db.execute("COMMIT")
    print(n, flush=True)          # only printed after COMMIT returned
'''

runs = int(sys.argv[1]) if len(sys.argv) > 1 else 200
bad = 0; total_rows = 0
with tempfile.TemporaryDirectory(dir=os.environ.get("EXP_DIR")) as d:
    path = os.path.join(d, "log.db")
    for _ in range(runs):
        p = subprocess.Popen([sys.executable, "-c", WRITER, path], stdout=subprocess.PIPE, text=True)
        time.sleep(random.uniform(0.03, 0.12))
        p.send_signal(signal.SIGKILL)
        out, _ = p.communicate()
        acked = int(out.split()[-1]) if out.split() else 0
        db = sqlite3.connect(path)
        ok = db.execute("PRAGMA integrity_check").fetchone()[0] == "ok"
        seqs = [r[0] for r in db.execute("SELECT seq FROM events ORDER BY seq")]
        db.close()
        n = len(seqs)
        if not (ok and seqs == list(range(1, n + 1)) and n % 3 == 0 and n >= acked):
            bad += 1
            print("VIOLATION", ok, n, acked)
        total_rows = n
print(f"{runs} kills, {total_rows} rows at the end, {bad} violations "
      f"(corrupt db, gap, half transaction visible, or acknowledged commit lost)")
