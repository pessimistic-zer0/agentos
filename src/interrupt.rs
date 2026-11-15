//! What happens when the user stops a run (Ctrl-C, a closed terminal,
//! `kill`): say that the execution can be continued, and how. Then stop at
//! once, exactly as a `kill -9` would (D-106).
//!
//! Nothing is cleaned up here on purpose. Stopping in the middle of a call
//! is the case the event log and recovery exist for, and the fault-injection
//! suite tests it 4,800 times. A handler that tried to "finish properly"
//! would be a second, untested way of stopping.

use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

// The text to print, prepared ahead of time: a signal handler may not
// allocate or format (it can interrupt code that holds the allocator's
// lock). The bytes are leaked once per message and never freed.
static TEXT: AtomicPtr<u8> = AtomicPtr::new(std::ptr::null_mut());
static LEN: AtomicUsize = AtomicUsize::new(0);

fn set(text: String) {
    let bytes: &'static mut [u8] = Box::leak(text.into_bytes().into_boxed_slice());
    // Length first to zero, then pointer, then length: a handler running in
    // between sees either the old pair or a shorter text, never a long
    // length with a short buffer.
    LEN.store(0, Ordering::SeqCst);
    TEXT.store(bytes.as_mut_ptr(), Ordering::SeqCst);
    LEN.store(bytes.len(), Ordering::SeqCst);
}

/// A run of this execution is under way.
pub fn running(id: &str) {
    set(format!(
        "\nagentos: stopped. Execution {id} is unfinished; nothing is lost.\n  \
         Go on with it:                 agentos run --resume {id}\n  \
         Only settle the cut-off call:  agentos recover {id}\n  \
         (Add the same --state-dir if you used one.)\n"
    ));
}

/// Recovery of this execution is under way.
pub fn recovering(id: &str) {
    set(format!("\nagentos: stopped during recovery of {id}. Recovery is safe to repeat:  agentos recover {id}\n"));
}

/// The execution is finished; a signal now needs no explanation.
pub fn finished() {
    LEN.store(0, Ordering::SeqCst);
}

extern "C" fn on_signal(sig: libc::c_int) {
    let (ptr, len) = (TEXT.load(Ordering::SeqCst), LEN.load(Ordering::SeqCst));
    // SAFETY: `write` and `_exit` are async-signal-safe. `ptr` is null or
    // points at a leaked buffer of at least `len` bytes (see `set`).
    unsafe {
        if !ptr.is_null() && len > 0 {
            libc::write(2, ptr.cast(), len);
        }
        libc::_exit(128 + sig)
    }
}

/// Catch SIGINT (Ctrl-C), SIGTERM and SIGHUP (the terminal closed).
/// A signal that is ignored when agentos starts stays ignored: that is how
/// `nohup` and a shell's background jobs ask not to be stopped.
pub fn install() {
    for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
        // SAFETY: `sigaction` with a zeroed struct we own; the handler above
        // only makes async-signal-safe calls.
        unsafe {
            let mut old: libc::sigaction = std::mem::zeroed();
            if libc::sigaction(sig, std::ptr::null(), &mut old) != 0 || old.sa_sigaction == libc::SIG_IGN {
                continue;
            }
            let mut new: libc::sigaction = std::mem::zeroed();
            new.sa_sigaction = on_signal as extern "C" fn(libc::c_int) as usize;
            libc::sigemptyset(&mut new.sa_mask);
            libc::sigaction(sig, &new, std::ptr::null_mut());
        }
    }
}
