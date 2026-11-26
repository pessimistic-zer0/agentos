//! The fake external service of the v1 "Bonus": a tiny HTTP server that
//! "applies" requests and remembers them, with faults that can be switched on.
//!
//! It stands for anything outside Norn that cannot be rolled back (a
//! payment, an email, a pull request). It is deliberately small: standard
//! library only, one thread per connection, state in memory.
//!
//! What it promises, like real services that take an idempotency key:
//! a request with a key it has already applied is **not applied again**; it
//! gets the stored answer back, marked `"replayed": true`.
//!
//! | Request | Answer |
//! |---|---|
//! | `POST /apply` with header `Idempotency-Key: K` and a text body | `200 {"seq": N, "key": "K", "replayed": false}` the first time, `"replayed": true` after that |
//! | same, body starts with `reject` | `422`: refused, nothing applied |
//! | same key, different body | `409`: refused, nothing applied |
//! | no `Idempotency-Key` header | `400` |
//! | `GET /state` | `200` with everything applied so far, as JSON |
//! | body starts with `echo` | applied as usual, and the answer also lists every request header it got (a careless service; for the secret tests, M21) |
//!
//! `start_tls` serves the same over HTTPS with the test certificate in
//! `tests/fixtures/tls/` (M21). `require_auth` makes it answer `401` to a
//! request without that exact `Authorization` header.
//!
//! Faults are queued per payload; each `POST /apply` for that payload uses
//! up the next one:
//!
//! | Fault | What the caller sees | What really happened |
//! |---|---|---|
//! | `DelayMs(n)` | a slow answer | applied after `n` ms |
//! | `DropBefore` | no answer (timeout) | **not** applied: the request was "lost" |
//! | `DropAfter` | no answer (timeout) | **applied**: the answer was "lost" |
//!
//! The last two look exactly the same from outside. That is the whole
//! problem the idempotency key solves.
//!
//! Used in-process by the tests (`FakeService::start`), and as a program by
//! `examples/fake_service.rs`.
#![allow(dead_code)] // each user of this file needs a different part of it

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Fault {
    DelayMs(u64),
    DropBefore,
    DropAfter,
}

/// One effect the service carried out. The tests read these as ground truth.
#[derive(Debug, Clone, PartialEq)]
pub struct Applied {
    pub seq: u64,
    pub key: String,
    pub payload: String,
}

/// One `POST /apply` the service received, whatever became of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Seen {
    pub key: String,
    pub payload: String,
    /// `applied`, `replayed`, `dropped-before`, `applied-then-dropped`,
    /// `replayed-then-dropped`, `rejected`, `conflict`, `unauthorized`.
    pub outcome: &'static str,
    /// The `Authorization` header, as received.
    pub authorization: Option<String>,
}

#[derive(Default)]
struct State {
    applied: Vec<Applied>,
    seen: Vec<Seen>,
    faults: HashMap<String, VecDeque<Fault>>,
    require_auth: Option<String>,
}

pub struct FakeService {
    port: u16,
    state: Arc<Mutex<State>>,
    stop: Arc<AtomicBool>,
    https: bool,
}

impl FakeService {
    /// Start on a free port of 127.0.0.1 (`port` 0) or on the given one.
    /// `hold` is how long a dropped request keeps its connection open
    /// before closing it without an answer; make it longer than the
    /// caller's timeout.
    pub fn start(port: u16, hold: Duration) -> std::io::Result<FakeService> {
        FakeService::start_with(port, hold, None)
    }

    /// The same service over HTTPS, with the test server certificate
    /// (signed by `tests/fixtures/tls/ca.pem`, for `127.0.0.1` and
    /// `localhost`).
    pub fn start_tls(port: u16, hold: Duration) -> std::io::Result<FakeService> {
        use rustls::pki_types::pem::PemObject;
        use rustls::pki_types::{CertificateDer, PrivateKeyDer};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/tls");
        let certs: Vec<CertificateDer<'static>> =
            CertificateDer::pem_file_iter(dir.join("server.pem")).map_err(std::io::Error::other)?.collect::<Result<_, _>>().map_err(std::io::Error::other)?;
        let key = PrivateKeyDer::from_pem_file(dir.join("server-key.pem")).map_err(std::io::Error::other)?;
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(rustls::crypto::ring::default_provider()))
            .with_safe_default_protocol_versions()
            .map_err(std::io::Error::other)?
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .map_err(std::io::Error::other)?;
        FakeService::start_with(port, hold, Some(Arc::new(config)))
    }

    fn start_with(port: u16, hold: Duration, tls: Option<Arc<rustls::ServerConfig>>) -> std::io::Result<FakeService> {
        let listener = TcpListener::bind(("127.0.0.1", port))?;
        let port = listener.local_addr()?.port();
        let https = tls.is_some();
        let state = Arc::new(Mutex::new(State::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (st, sp) = (state.clone(), stop.clone());
        std::thread::spawn(move || {
            for conn in listener.incoming() {
                if sp.load(Ordering::SeqCst) {
                    break;
                }
                if let Ok(conn) = conn {
                    let st = st.clone();
                    let tls = tls.clone();
                    std::thread::spawn(move || {
                        let _ = conn.set_read_timeout(Some(Duration::from_secs(5)));
                        match tls {
                            None => {
                                let _ = handle(conn, &st, hold);
                            }
                            Some(cfg) => {
                                let Ok(server) = rustls::ServerConnection::new(cfg) else { return };
                                let _ = handle(rustls::StreamOwned::new(server, conn), &st, hold);
                            }
                        }
                    });
                }
            }
        });
        Ok(FakeService { port, state, stop, https })
    }

    /// Answer `401` unless the request has exactly this `Authorization` header.
    pub fn require_auth(&self, value: &str) {
        self.state.lock().unwrap().require_auth = Some(value.to_string());
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// The URL to put in the policy.
    pub fn url(&self) -> String {
        format!("{}://127.0.0.1:{}/apply", if self.https { "https" } else { "http" }, self.port)
    }

    /// Queue faults for the next `POST /apply` requests carrying `payload`.
    pub fn inject(&self, payload: &str, faults: &[Fault]) {
        self.state.lock().unwrap().faults.entry(payload.to_string()).or_default().extend(faults.iter().copied());
    }

    /// Faults still queued (not yet used up by a request).
    pub fn faults_left(&self) -> usize {
        self.state.lock().unwrap().faults.values().map(|q| q.len()).sum()
    }

    pub fn applied(&self) -> Vec<Applied> {
        self.state.lock().unwrap().applied.clone()
    }

    pub fn seen(&self) -> Vec<Seen> {
        self.state.lock().unwrap().seen.clone()
    }
}

impl Drop for FakeService {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        // Wake the accept loop so it sees the flag.
        let _ = TcpStream::connect(("127.0.0.1", self.port));
    }
}

struct Request {
    method: String,
    path: String,
    key: Option<String>,
    body: String,
    headers: Vec<(String, String)>,
}

fn read_request(conn: &mut impl Read) -> std::io::Result<Request> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        let n = conn.read(&mut chunk)?;
        if n == 0 || buf.len() > 64 * 1024 {
            return Err(std::io::Error::other("bad request"));
        }
        buf.extend_from_slice(&chunk[..n]);
    };
    let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
    let mut lines = head.split("\r\n");
    let mut first = lines.next().unwrap_or("").split(' ');
    let method = first.next().unwrap_or("").to_string();
    let path = first.next().unwrap_or("").to_string();
    let (mut key, mut len) = (None, 0usize);
    let mut headers = Vec::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            headers.push((name.to_string(), value.trim().to_string()));
            match name.to_ascii_lowercase().as_str() {
                "idempotency-key" => key = Some(value.trim().to_string()),
                "content-length" => len = value.trim().parse().unwrap_or(0),
                _ => {}
            }
        }
    }
    if len > 1024 * 1024 {
        return Err(std::io::Error::other("body too large"));
    }
    while buf.len() < head_end + len {
        let n = conn.read(&mut chunk)?;
        if n == 0 {
            return Err(std::io::Error::other("body cut short"));
        }
        buf.extend_from_slice(&chunk[..n]);
    }
    let body = String::from_utf8_lossy(&buf[head_end..head_end + len]).into_owned();
    Ok(Request { method, path, key, body, headers })
}

fn respond(conn: &mut impl Write, status: u16, body: &str) -> std::io::Result<()> {
    let reason = match status {
        200 => "OK",
        400 => "Bad Request",
        401 => "Unauthorized",
        404 => "Not Found",
        409 => "Conflict",
        _ => "Unprocessable Entity",
    };
    write!(
        conn,
        "HTTP/1.1 {status} {reason}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )?;
    conn.flush()
}

/// Apply `payload` under `key` unless that key was applied before. This is
/// the idempotency rule, and it is one step under one lock: two requests
/// with the same key can never both apply.
fn apply(state: &Mutex<State>, key: &str, payload: &str) -> Result<(u64, bool), ()> {
    let mut st = state.lock().unwrap();
    if let Some(a) = st.applied.iter().find(|a| a.key == key) {
        return if a.payload == payload { Ok((a.seq, true)) } else { Err(()) };
    }
    let seq = st.applied.len() as u64 + 1;
    st.applied.push(Applied { seq, key: key.to_string(), payload: payload.to_string() });
    Ok((seq, false))
}

fn handle<C: Read + Write>(mut conn: C, state: &Mutex<State>, hold: Duration) -> std::io::Result<()> {
    let req = read_request(&mut conn)?;
    if req.method == "GET" && req.path == "/state" {
        let st = state.lock().unwrap();
        let items: Vec<serde_json::Value> =
            st.applied.iter().map(|a| serde_json::json!({"seq": a.seq, "key": a.key, "payload": a.payload})).collect();
        let body = serde_json::json!({"applied": items, "requests_seen": st.seen.len()}).to_string();
        drop(st);
        return respond(&mut conn, 200, &body);
    }
    if req.method != "POST" || req.path != "/apply" {
        return respond(&mut conn, 404, r#"{"error":"unknown path"}"#);
    }
    let Some(key) = req.key else {
        return respond(&mut conn, 400, r#"{"error":"Idempotency-Key header is required"}"#);
    };
    let authorization = req.headers.iter().find(|(n, _)| n.eq_ignore_ascii_case("authorization")).map(|(_, v)| v.clone());
    let note = |outcome: &'static str| {
        state.lock().unwrap().seen.push(Seen { key: key.clone(), payload: req.body.clone(), outcome, authorization: authorization.clone() });
    };
    let required = state.lock().unwrap().require_auth.clone();
    if required.is_some() && required != authorization {
        note("unauthorized");
        return respond(&mut conn, 401, r#"{"error":"missing or wrong Authorization header"}"#);
    }
    if req.body.starts_with("reject") {
        note("rejected");
        return respond(&mut conn, 422, r#"{"error":"rejected by the service; nothing was applied"}"#);
    }
    let fault = state.lock().unwrap().faults.get_mut(&req.body).and_then(|q| q.pop_front());
    match fault {
        Some(Fault::DropBefore) => {
            note("dropped-before");
            std::thread::sleep(hold);
            return Ok(()); // connection closed, nothing applied, no answer
        }
        Some(Fault::DelayMs(ms)) => std::thread::sleep(Duration::from_millis(ms)),
        _ => {}
    }
    let Ok((seq, replayed)) = apply(state, &key, &req.body) else {
        note("conflict");
        return respond(&mut conn, 409, r#"{"error":"this key was used with a different payload"}"#);
    };
    if fault == Some(Fault::DropAfter) {
        note(if replayed { "replayed-then-dropped" } else { "applied-then-dropped" });
        std::thread::sleep(hold);
        return Ok(()); // applied, but the caller never hears about it
    }
    note(if replayed { "replayed" } else { "applied" });
    let mut body = serde_json::json!({"seq": seq, "key": key, "replayed": replayed});
    if req.body.starts_with("echo") {
        let headers: serde_json::Map<String, serde_json::Value> = req.headers.iter().map(|(n, v)| (n.clone(), serde_json::Value::from(v.clone()))).collect();
        body["headers_received"] = serde_json::Value::Object(headers);
    }
    let body = body.to_string();
    // The caller may be gone (killed mid-request); that is not our problem.
    let _ = respond(&mut conn, 200, &body);
    Ok(())
}
