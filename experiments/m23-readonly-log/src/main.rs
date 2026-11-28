//! E1 (v4, M23): can a second process read the event log read-only while a
//! run writes it, without slowing the writer or seeing half an event?
//!
//! `write <db> <n>`: append n events the way norn does (WAL, synchronous=FULL,
//!   one INSERT per event, a ~600-byte JSON body), print append latencies.
//! `read <db> <secs>`: open read-only in a loop, read every row, check that
//!   seq is 1..=max with no gaps and every body ends with '}'.
//! `once <db>`: one read-only open and read; print the row count or the error.

use std::time::{Duration, Instant};

use rusqlite::{Connection, OpenFlags};

fn open_writer(db: &str) -> Connection {
    let c = Connection::open(db).unwrap();
    c.pragma_update(None, "journal_mode", "WAL").unwrap();
    c.pragma_update(None, "synchronous", "FULL").unwrap();
    c.execute_batch("CREATE TABLE IF NOT EXISTS events (seq INTEGER PRIMARY KEY, ts TEXT NOT NULL, type TEXT NOT NULL, call_id INTEGER, body TEXT NOT NULL);").unwrap();
    c
}

fn open_reader(db: &str) -> rusqlite::Result<Connection> {
    Connection::open_with_flags(db, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX)
}

fn read_all(c: &Connection) -> rusqlite::Result<(u64, bool)> {
    let mut st = c.prepare("SELECT seq, body FROM events ORDER BY seq")?;
    let rows = st.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
    let mut n = 0u64;
    let mut ok = true;
    for r in rows {
        let (seq, body) = r?;
        n += 1;
        if seq as u64 != n || !body.ends_with('}') {
            ok = false;
        }
    }
    Ok((n, ok))
}

fn pct(v: &mut [f64], p: f64) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v[((v.len() as f64 - 1.0) * p) as usize]
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a[1].as_str() {
        "write" => {
            let c = open_writer(&a[2]);
            let n: u64 = a[3].parse().unwrap();
            let base: i64 = c.query_row("SELECT COALESCE(MAX(seq),0) FROM events", [], |r| r.get(0)).unwrap();
            let body = format!("{{\"type\":\"tool.completed\",\"summary\":\"{}\"}}", "x".repeat(560));
            let mut lat = Vec::new();
            for i in 1..=n {
                let t = Instant::now();
                c.execute("INSERT INTO events (seq, ts, type, call_id, body) VALUES (?1, 't', 'tool.completed', 1, ?2)", rusqlite::params![base + i as i64, body]).unwrap();
                lat.push(t.elapsed().as_secs_f64() * 1000.0);
            }
            println!("appended {n}: p50 {:.2} ms, p99 {:.2} ms", pct(&mut lat.clone(), 0.5), pct(&mut lat, 0.99));
        }
        "read" => {
            let secs: u64 = a[3].parse().unwrap();
            let end = Instant::now() + Duration::from_secs(secs);
            let (mut reads, mut bad, mut errs, mut last, mut errcount) = (0u64, 0u64, Vec::new(), 0u64, 0u64);
            while Instant::now() < end {
                match open_reader(&a[2]).and_then(|c| read_all(&c)) {
                    Ok((n, ok)) => {
                        reads += 1;
                        if !ok || n < last {
                            bad += 1;
                        }
                        last = n;
                    }
                    Err(e) => {
                        errcount += 1;
                        if errs.len() < 3 {
                            errs.push(e.to_string());
                        }
                    }
                }
            }
            println!("reads {reads}, inconsistent {bad}, last count {last}, errors {} {errs:?}", errcount);
        }
        "once" => match open_reader(&a[2]).and_then(|c| read_all(&c)) {
            Ok((n, ok)) => println!("ok: {n} rows, consistent {ok}"),
            Err(e) => println!("error: {e}"),
        },
        _ => panic!("write|read|once"),
    }
}
