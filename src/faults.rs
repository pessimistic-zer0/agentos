//! Fault injection: make the runtime die at a chosen point, so the recovery
//! tests can check what is left behind.
//!
//! Controlled by environment variables, read once:
//!
//! | Variable | Meaning |
//! |---|---|
//! | `NORN_CRASH_AT` | Crash point name: `P1` … `P8` (see the v1 table) |
//! | `NORN_CRASH_CALL` | Only crash while handling this call id (default: any) |
//! | `NORN_CRASH_HIT` | Crash on the n-th time the point is reached (default 1) |
//! | `NORN_CRASH_DELAY_US` | For mid-effect points: kill after a random delay up to this many microseconds |
//!
//! "Crash" means the process sends itself SIGKILL: no destructors run, no
//! buffers are flushed, exactly like `kill -9` from outside (D-052).
//! With no variable set, every function here does nothing.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;

struct Config {
    at: String,
    call: Option<u64>,
    hit: u64,
    delay_us: u64,
}

static CONFIG: OnceLock<Option<Config>> = OnceLock::new();
static HITS: AtomicU64 = AtomicU64::new(0);

fn config() -> Option<&'static Config> {
    CONFIG
        .get_or_init(|| {
            let at = crate::env_var("CRASH_AT")?;
            let num = |k: &str| crate::env_var(k).and_then(|v| v.parse::<u64>().ok());
            Some(Config {
                at,
                call: num("CRASH_CALL"),
                hit: num("CRASH_HIT").unwrap_or(1),
                delay_us: num("CRASH_DELAY_US").unwrap_or(2000),
            })
        })
        .as_ref()
}

fn matches(name: &str, call_id: Option<u64>) -> Option<&'static Config> {
    let c = config()?;
    if c.at != name {
        return None;
    }
    if let (Some(want), Some(got)) = (c.call, call_id) {
        if want != got {
            return None;
        }
    }
    let n = HITS.fetch_add(1, Ordering::SeqCst) + 1;
    (n == c.hit).then_some(c)
}

fn die() -> ! {
    let _ = nix::sys::signal::kill(nix::unistd::getpid(), nix::sys::signal::Signal::SIGKILL);
    // SIGKILL cannot be caught; this line is only reached if sending failed.
    std::process::abort()
}

/// Crash here, now, if this is the configured point.
pub fn point(name: &str, call_id: Option<u64>) {
    if matches(name, call_id).is_some() {
        die();
    }
}

/// For points *during* an effect: start a timer that kills the process after
/// a random delay, so the kill lands somewhere inside whatever runs next.
pub fn arm(name: &str, call_id: Option<u64>) {
    if let Some(c) = matches(name, call_id) {
        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.subsec_nanos()).unwrap_or(0) as u64;
        // Cheap scramble of the clock's nanoseconds; quality does not matter here.
        let r = nanos.wrapping_mul(0x9E37_79B9_7F4A_7C15) >> 20;
        let delay = std::time::Duration::from_micros(r % c.delay_us.max(1));
        std::thread::spawn(move || {
            std::thread::sleep(delay);
            die();
        });
    }
}
