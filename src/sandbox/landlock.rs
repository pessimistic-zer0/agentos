//! Landlock: the sandboxed command gives up access to every path except the
//! ones listed here, for itself and every process it starts. This runs after
//! the new root is in place, so the paths are the sandbox's own (`/work`,
//! `/tmp`, the read-only system folders). It is defence in depth: if the mount
//! setup had a bug that left a host folder visible, Landlock would still
//! refuse it.

use std::path::Path;

use anyhow::{bail, Result};
use landlock::{
    path_beneath_rules, Access, AccessFs, CompatLevel, Compatible, Ruleset, RulesetAttr, RulesetCreatedAttr,
    RulesetStatus, ABI,
};

/// Device files every sandbox gets (read and write, nothing else).
pub const DEVICES: [&str; 5] = ["/dev/null", "/dev/zero", "/dev/full", "/dev/random", "/dev/urandom"];

/// The ABI this code is written against. Newer kernels support more; older
/// ones get "best effort" (each right the kernel does not know is skipped).
const TARGET: ABI = ABI::V5;

/// Restrict this process (and its future children) to:
/// - read + execute below each folder in `ro`,
/// - everything below `/work` and `/tmp`,
/// - read + write on the device files above,
/// - read below `/proc` (the sandbox's own `/proc`, which shows only its own
///   processes, because it is mounted inside the new PID namespace).
pub fn restrict(ro: &[&Path]) -> Result<RulesetStatus> {
    let abi = TARGET;
    let ro: Vec<&Path> = ro.iter().copied().filter(|p| p.exists()).collect();
    let status = Ruleset::default()
        .set_compatibility(CompatLevel::BestEffort)
        .handle_access(AccessFs::from_all(abi))?
        .create()?
        .add_rules(path_beneath_rules(&ro, AccessFs::from_read(abi)))?
        .add_rules(path_beneath_rules(["/work", "/tmp"], AccessFs::from_all(abi)))?
        .add_rules(path_beneath_rules(DEVICES, AccessFs::ReadFile | AccessFs::WriteFile))?
        .add_rules(path_beneath_rules(["/proc"], AccessFs::ReadFile | AccessFs::ReadDir))?
        .restrict_self()?;
    if status.ruleset == RulesetStatus::NotEnforced {
        // The startup check makes this unlikely; refuse rather than run unconfined.
        bail!("Landlock is not enforced by this kernel");
    }
    Ok(status.ruleset)
}

/// The Landlock ABI version the running kernel supports, or 0 if none.
/// `landlock_create_ruleset(NULL, 0, LANDLOCK_CREATE_RULESET_VERSION)` is the
/// documented way to ask (Documentation/userspace-api/landlock.rst).
pub fn kernel_abi() -> i64 {
    const LANDLOCK_CREATE_RULESET_VERSION: libc::c_uint = 1;
    // SAFETY: with a null attribute pointer and size 0, this syscall only
    // reads the flag and returns an integer; it touches no memory of ours.
    let v = unsafe {
        libc::syscall(libc::SYS_landlock_create_ruleset, std::ptr::null::<u8>(), 0usize, LANDLOCK_CREATE_RULESET_VERSION)
    };
    if v < 0 { 0 } else { v }
}
