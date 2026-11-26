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
///
/// The prefix stays `agentos-` after the rename to norn (D-134): an
/// execution left uncertain by an older version is reconciled by sending its
/// call again, and a different key would make the service apply it twice.
pub fn idempotency_key(execution_id: &str, call_id: u64) -> String {
    format!("agentos-{execution_id}-c{call_id}")
}

/// Where and how one request is sent. `headers` hold real secret values
/// (M21): a `Target` is built just before sending and never stored.
#[derive(Clone)]
pub struct Target {
    pub url: String,
    pub headers: Vec<(String, String)>,
    /// PEM of the one certificate authority to trust (`ca_file`, D-157);
    /// `None`: the compiled-in list.
    pub ca_pem: Option<Vec<u8>>,
}

impl Target {
    pub fn plain(url: &str) -> Target {
        Target { url: url.to_string(), headers: Vec::new(), ca_pem: None }
    }

    /// The target for `url`, with the header templates filled in from
    /// `secrets` and the CA file read.
    pub fn build(url: &str, headers: &std::collections::BTreeMap<String, String>, ca_file: Option<&str>, secrets: &crate::secrets::Secrets) -> anyhow::Result<Target> {
        use anyhow::Context;
        let headers = headers.iter().map(|(k, v)| Ok((k.clone(), secrets.expand(v)?))).collect::<anyhow::Result<Vec<_>>>()?;
        let ca_pem = match ca_file {
            Some(f) => Some(std::fs::read(f).with_context(|| format!("reading ca_file {f}"))?),
            None => None,
        };
        Ok(Target { url: url.to_string(), headers, ca_pem })
    }
}

/// Send `payload` to the target once, with `key` as the `Idempotency-Key` header.
pub fn send(target: &Target, key: &str, payload: &str, timeout: Duration, max_body: usize) -> Answer {
    let mut tls = ureq::tls::TlsConfig::builder();
    if let Some(pem) = &target.ca_pem {
        // Trust this authority only, for this service (D-157, E5).
        match ureq::tls::Certificate::from_pem(pem) {
            Ok(cert) => tls = tls.root_certs(ureq::tls::RootCerts::new_with_certs(&[cert])),
            Err(e) => return Answer::Unknown { why: format!("ca_file is not a PEM certificate: {e}") },
        }
    }
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls.build())
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
    let mut req = agent.post(&target.url).header("Idempotency-Key", key).header("Content-Type", "text/plain; charset=utf-8");
    for (k, v) in &target.headers {
        req = req.header(k, v);
    }
    let sent = req.send(payload);
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
pub fn send_until_answered(target: &Target, key: &str, payload: &str, timeout: Duration, attempts: u32, max_body: usize) -> (Answer, u32) {
    let mut last = Answer::Unknown { why: "no request was sent".into() };
    for n in 1..=attempts.max(1) {
        last = send(target, key, payload, timeout, max_body);
        if !matches!(last, Answer::Unknown { .. }) {
            return (last, n);
        }
    }
    (last, attempts.max(1))
}
