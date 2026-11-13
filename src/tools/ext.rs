//! `ext.call`: ask a service outside the runtime to do something (the v1
//! "Bonus"). This file only sends one HTTP request and says what the answer
//! means. The rules around it (policy, intent, retries after a crash) are in
//! `gateway.rs` and `recovery.rs`.
//!
//! The one thing that matters here is being honest about what an answer
//! proves (D-077):
//!
//! | What came back | Meaning |
//! |---|---|
//! | HTTP 2xx | the service did it (now, or earlier under the same key) |
//! | HTTP 4xx | the service refused; it did nothing |
//! | anything else: a timeout, a dropped connection, HTTP 5xx, a redirect | **unknown**: it may or may not have been done |
//!
//! "Unknown" is never turned into "failed". A request that timed out may
//! still have been carried out.

use std::time::Duration;

/// What one request, or a series of them with the same key, established.
#[derive(Debug, Clone, PartialEq)]
pub enum Answer {
    Applied { status: u16, body: String },
    Refused { status: u16, body: String },
    Unknown { why: String },
}

/// The idempotency key of one tool call: the execution id and the call id.
///
/// It is worked out from things that are already in the log, not drawn at
/// random, so the runtime that sends the request and a later recovery that
/// sends it again cannot disagree about it. Execution ids are unique per
/// state folder and call ids are never reused inside an execution, so one
/// key means one intended effect (D-077).
pub fn idempotency_key(execution_id: &str, call_id: u64) -> String {
    format!("agentos-{execution_id}-c{call_id}")
}

/// Send `payload` to `url` once, with `key` as the `Idempotency-Key` header.
pub fn send(url: &str, key: &str, payload: &str, timeout: Duration, max_body: usize) -> Answer {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(timeout))
        // A status code is an answer to classify, not an error.
        .http_status_as_error(false)
        // The policy only allows 127.0.0.1 (D-074). A redirect could point
        // anywhere, so none is followed: it comes back as an error, which
        // counts as "unknown". The same goes for a proxy named in the
        // environment (`http_proxy`), which ureq would otherwise use.
        .max_redirects(0)
        .proxy(None)
        .build()
        .into();
    let sent = agent.post(url).header("Idempotency-Key", key).header("Content-Type", "text/plain; charset=utf-8").send(payload);
    let mut resp = match sent {
        Ok(resp) => resp,
        Err(e) => return Answer::Unknown { why: e.to_string() },
    };
    let status = resp.status().as_u16();
    // The status line already says what happened. If the body cannot be
    // read, the answer still stands, with an empty body.
    let mut body = resp.body_mut().read_to_string().unwrap_or_default();
    if body.len() > max_body {
        let mut cut = max_body;
        while !body.is_char_boundary(cut) {
            cut -= 1;
        }
        body.truncate(cut);
    }
    match status {
        200..=299 => Answer::Applied { status, body },
        400..=499 => Answer::Refused { status, body },
        other => Answer::Unknown { why: format!("the service answered HTTP {other}") },
    }
}

/// Send until there is a definite answer, at most `attempts` times, always
/// with the same key. Returns the answer and how many requests were sent.
///
/// Sending again is safe only because the key is the same: the service
/// promises to apply a key at most once and to repeat its first answer.
pub fn send_until_answered(url: &str, key: &str, payload: &str, timeout: Duration, attempts: u32, max_body: usize) -> (Answer, u32) {
    let mut last = Answer::Unknown { why: "no request was sent".into() };
    for n in 1..=attempts.max(1) {
        last = send(url, key, payload, timeout, max_body);
        if !matches!(last, Answer::Unknown { .. }) {
            return (last, n);
        }
    }
    (last, attempts.max(1))
}
