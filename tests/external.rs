//! The v1 "Bonus": one fake external service, called through the gateway
//! with an idempotency key.
//!
//! Part 1 (here, no sandbox needed): the fake service keeps its promise, and
//! the client in `src/tools/ext.rs` reads each kind of answer correctly.
//! Part 2 (further down): `ext.call` through the real `agentos` binary.
//! The kill and timeout matrix is in `tests/recovery/main.rs`.

mod common;

use std::time::{Duration, Instant};

use agentos::tools::ext::{self, Answer};
use common::fake_service::{FakeService, Fault};

const TIMEOUT: Duration = Duration::from_millis(300);
const HOLD: Duration = Duration::from_millis(500);

fn body_json(a: &Answer) -> serde_json::Value {
    match a {
        Answer::Applied { body, .. } => serde_json::from_str(body).unwrap(),
        other => panic!("expected an applied answer, got {other:?}"),
    }
}

#[test]
fn the_service_applies_each_key_once_and_replays_its_answer() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let first = ext::send(&svc.url(), "k1", "order-1", TIMEOUT, 4096);
    assert_eq!(body_json(&first), serde_json::json!({"seq": 1, "key": "k1", "replayed": false}));
    let again = ext::send(&svc.url(), "k1", "order-1", TIMEOUT, 4096);
    assert_eq!(body_json(&again), serde_json::json!({"seq": 1, "key": "k1", "replayed": true}));
    let other = ext::send(&svc.url(), "k2", "order-1", TIMEOUT, 4096);
    assert_eq!(body_json(&other)["seq"], 2, "a new key is a new effect, even with the same payload");
    assert_eq!(svc.applied().len(), 2);
    assert_eq!(svc.seen().iter().map(|s| s.outcome).collect::<Vec<_>>(), vec!["applied", "replayed", "applied"]);
}

#[test]
fn refusals_are_definite_and_apply_nothing() {
    let svc = FakeService::start(0, HOLD).unwrap();
    assert!(matches!(ext::send(&svc.url(), "k1", "reject this", TIMEOUT, 4096), Answer::Refused { status: 422, .. }));
    // Same key, different payload: the service will not guess which one was meant.
    assert!(matches!(ext::send(&svc.url(), "k2", "a", TIMEOUT, 4096), Answer::Applied { .. }));
    assert!(matches!(ext::send(&svc.url(), "k2", "b", TIMEOUT, 4096), Answer::Refused { status: 409, .. }));
    // A path the service does not have.
    let wrong = svc.url().replace("/apply", "/nope");
    assert!(matches!(ext::send(&wrong, "k3", "x", TIMEOUT, 4096), Answer::Refused { status: 404, .. }));
    assert_eq!(svc.applied().len(), 1);
}

#[test]
fn a_lost_request_and_a_lost_answer_look_the_same_from_outside() {
    let svc = FakeService::start(0, HOLD).unwrap();
    svc.inject("lost-request", &[Fault::DropBefore]);
    svc.inject("lost-answer", &[Fault::DropAfter]);

    let started = Instant::now();
    let a = ext::send(&svc.url(), "ka", "lost-request", TIMEOUT, 4096);
    assert!(matches!(a, Answer::Unknown { .. }), "{a:?}");
    assert!(started.elapsed() >= TIMEOUT, "the client waited for its timeout");
    let b = ext::send(&svc.url(), "kb", "lost-answer", TIMEOUT, 4096);
    assert!(matches!(b, Answer::Unknown { .. }), "{b:?}");

    // Same view from the client; opposite truth at the service.
    let applied: Vec<String> = svc.applied().into_iter().map(|a| a.payload).collect();
    assert_eq!(applied, vec!["lost-answer".to_string()]);

    // Sending again with the same key settles both, and nothing is doubled.
    assert_eq!(body_json(&ext::send(&svc.url(), "ka", "lost-request", TIMEOUT, 4096))["replayed"], false);
    assert_eq!(body_json(&ext::send(&svc.url(), "kb", "lost-answer", TIMEOUT, 4096))["replayed"], true);
    assert_eq!(svc.applied().len(), 2);
}

#[test]
fn send_until_answered_retries_with_the_same_key_and_gives_up_as_unknown() {
    let svc = FakeService::start(0, HOLD).unwrap();
    svc.inject("flaky", &[Fault::DropAfter, Fault::DropBefore]);
    let (answer, sent) = ext::send_until_answered(&svc.url(), "k1", "flaky", TIMEOUT, 3, 4096);
    assert_eq!(sent, 3);
    assert_eq!(body_json(&answer)["replayed"], true, "attempt 1 applied it; attempt 3 got the stored answer");
    assert_eq!(svc.applied().len(), 1);
    assert!(svc.seen().iter().all(|s| s.key == "k1"));

    svc.inject("down", &[Fault::DropBefore; 3]);
    let (answer, sent) = ext::send_until_answered(&svc.url(), "k2", "down", TIMEOUT, 3, 4096);
    assert_eq!(sent, 3);
    assert!(matches!(answer, Answer::Unknown { .. }));
    assert_eq!(svc.applied().len(), 1, "nothing new was applied");
}

#[test]
fn nobody_listening_is_unknown_not_failed() {
    // Bind a port, then close it again, so nothing listens there.
    let port = std::net::TcpListener::bind("127.0.0.1:0").unwrap().local_addr().unwrap().port();
    let a = ext::send(&format!("http://127.0.0.1:{port}/apply"), "k", "x", TIMEOUT, 4096);
    assert!(matches!(a, Answer::Unknown { .. }), "{a:?}");
}

#[test]
fn a_redirect_is_not_followed() {
    use std::io::{Read, Write};
    // A one-shot server that answers every request with a redirect to the
    // fake service. If the client followed it, the service would see a request.
    let svc = FakeService::start(0, HOLD).unwrap();
    let target = svc.url();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for conn in listener.incoming().take(1) {
            let mut conn = conn.unwrap();
            let mut buf = [0u8; 2048];
            let _ = conn.read(&mut buf);
            let _ = write!(conn, "HTTP/1.1 307 Temporary Redirect\r\nLocation: {target}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        }
    });
    let a = ext::send(&format!("http://127.0.0.1:{port}/apply"), "k", "x", TIMEOUT, 4096);
    assert!(matches!(a, Answer::Unknown { .. }), "{a:?}");
    std::thread::sleep(Duration::from_millis(100));
    assert!(svc.seen().is_empty(), "the redirect was followed");
}

#[test]
fn long_answers_are_cut_at_the_limit() {
    let svc = FakeService::start(0, HOLD).unwrap();
    let a = ext::send(&svc.url(), "a-rather-long-idempotency-key-0123456789", "x", TIMEOUT, 16);
    match a {
        Answer::Applied { body, .. } => assert_eq!(body.len(), 16),
        other => panic!("{other:?}"),
    }
}
