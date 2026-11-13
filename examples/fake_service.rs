//! Runs the fake external service of the v1 "Bonus" as a program, for trying
//! `ext.call` by hand. The service itself is `tests/common/fake_service.rs`,
//! the same code the tests use.
//!
//!   cargo run --example fake_service -- [port]        (default port 8099)
//!
//! Faults, one list per payload, used up one per request:
//!
//!   FAKE_SERVICE_FAULTS='order-1=drop_after,drop_before;order-2=delay:300'
//!
//! `drop_before` loses the request (nothing applied, no answer), `drop_after`
//! loses the answer (applied, no answer), `delay:N` answers after N ms.
//! `FAKE_SERVICE_HOLD_MS` (default 5000) is how long a dropped request keeps
//! the connection open. See what was applied with:
//!
//!   curl http://127.0.0.1:8099/state

#[path = "../tests/common/fake_service.rs"]
mod fake_service;

use std::time::Duration;

use anyhow::{bail, Context, Result};
use fake_service::{FakeService, Fault};

fn parse_fault(s: &str) -> Result<Fault> {
    Ok(match s {
        "drop_before" => Fault::DropBefore,
        "drop_after" => Fault::DropAfter,
        other => match other.strip_prefix("delay:") {
            Some(ms) => Fault::DelayMs(ms.parse().with_context(|| format!("bad delay in `{other}`"))?),
            None => bail!("unknown fault `{other}` (drop_before, drop_after, delay:<ms>)"),
        },
    })
}

fn main() -> Result<()> {
    let port: u16 = match std::env::args().nth(1) {
        Some(p) => p.parse().context("the port must be a number")?,
        None => 8099,
    };
    let hold = std::env::var("FAKE_SERVICE_HOLD_MS").ok().and_then(|v| v.parse().ok()).unwrap_or(5000);
    let service = FakeService::start(port, Duration::from_millis(hold)).with_context(|| format!("listening on 127.0.0.1:{port}"))?;
    if let Ok(spec) = std::env::var("FAKE_SERVICE_FAULTS") {
        for part in spec.split(';').filter(|p| !p.is_empty()) {
            let (payload, list) = part.split_once('=').with_context(|| format!("`{part}` is not payload=fault,fault"))?;
            let faults = list.split(',').map(parse_fault).collect::<Result<Vec<_>>>()?;
            service.inject(payload, &faults);
            println!("faults for payload {payload:?}: {faults:?}");
        }
    }
    println!("fake service listening; put this in the policy:\n\n[[external.service]]\nname = \"ledger\"\nurl = \"{}\"\n", service.url());
    let mut printed = 0;
    loop {
        std::thread::sleep(Duration::from_millis(200));
        let seen = service.seen();
        for s in &seen[printed..] {
            println!("{:<22} key {}  payload {:?}", s.outcome, s.key, s.payload);
        }
        printed = seen.len();
    }
}
