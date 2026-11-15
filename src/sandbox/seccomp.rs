//! seccomp: a filter in the kernel that runs on every system call the
//! sandboxed command makes, and refuses some of them (D-116).
//!
//! It is a **deny list**: everything is allowed except about 45 calls that a
//! test command has no use for and that would let it reach more of the kernel
//! than the rest of the sandbox already allows: new namespaces (nested user
//! namespaces are the case `docs/threat-model.md` §4 named), mounts, `bpf`,
//! `ptrace`, `io_uring`, key rings, kernel modules, setting the clock.
//! Refused calls fail with `EPERM` ("not permitted"), an error programs
//! already handle; nothing is killed. An allow list (Docker's way) would
//! have to know every call that cargo, node, a linker and Python make.
//!
//! The filter is classic BPF, written out by hand: a short program the kernel
//! runs with the call's number and arguments (`struct seccomp_data`) and
//! that returns "allow" or "fail with this error". No library is used; each
//! instruction is below. Experiment: `experiments/m11-seccomp/`.
//!
//! It is installed by the sandbox's init process after Landlock, and is
//! inherited by the command and everything it starts. It needs
//! `no_new_privs`, which `ns::drop_privileges` has already set.

use anyhow::{bail, Result};

#[cfg(target_arch = "x86_64")]
const AUDIT_ARCH: u32 = 0xC000_003E; // AUDIT_ARCH_X86_64
#[cfg(target_arch = "aarch64")]
const AUDIT_ARCH: u32 = 0xC000_00B7; // AUDIT_ARCH_AARCH64
#[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
compile_error!("the seccomp filter knows x86_64 and aarch64 only");

// Offsets into `struct seccomp_data { int nr; u32 arch; u64 ip; u64 args[6]; }`.
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;
/// The low 32 bits of `args[0]` (both supported architectures are
/// little-endian). `clone`'s flags fit in them.
const OFF_ARG0_LO: u32 = 16;

const LD_W_ABS: u16 = (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16;
const RET_K: u16 = (libc::BPF_RET | libc::BPF_K) as u16;
const JEQ_K: u16 = (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16;
const JSET_K: u16 = (libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K) as u16;

/// Namespace flags a `clone` may not ask for.
const NS_FLAGS: u32 = (libc::CLONE_NEWUSER
    | libc::CLONE_NEWNS
    | libc::CLONE_NEWPID
    | libc::CLONE_NEWNET
    | libc::CLONE_NEWUTS
    | libc::CLONE_NEWIPC
    | libc::CLONE_NEWCGROUP) as u32;

/// The refused system calls. Each fails with `EPERM`.
pub fn denied() -> Vec<libc::c_long> {
    use libc::*;
    #[allow(unused_mut)]
    let mut v = vec![
        // Namespaces and mounts: the sandbox has built its own; the command
        // may not build more.
        SYS_unshare, SYS_setns, SYS_mount, SYS_umount2, SYS_pivot_root, SYS_chroot,
        SYS_fsopen, SYS_fsmount, SYS_fsconfig, SYS_fspick, SYS_move_mount, SYS_open_tree, SYS_mount_setattr,
        // Kernel interfaces with a long record of security bugs.
        SYS_bpf, SYS_perf_event_open, SYS_userfaultfd, SYS_io_uring_setup, SYS_io_uring_enter, SYS_io_uring_register,
        SYS_keyctl, SYS_add_key, SYS_request_key,
        // Looking into or controlling other processes.
        SYS_ptrace, SYS_process_vm_readv, SYS_process_vm_writev,
        // Opening files by kernel handle, which can step around the mount view.
        SYS_open_by_handle_at, SYS_name_to_handle_at,
        // Whole-machine operations: none can succeed without host privileges,
        // and none has a place in a test run.
        SYS_kexec_load, SYS_kexec_file_load, SYS_init_module, SYS_finit_module, SYS_delete_module,
        SYS_swapon, SYS_swapoff, SYS_reboot, SYS_acct, SYS_settimeofday, SYS_clock_settime, SYS_clock_adjtime,
        SYS_adjtimex, SYS_syslog, SYS_quotactl, SYS_vhangup,
    ];
    #[cfg(target_arch = "x86_64")]
    v.extend([SYS_iopl, SYS_ioperm, SYS_uselib]);
    v
}

fn ret(k: u32) -> libc::sock_filter {
    libc::sock_filter { code: RET_K, jt: 0, jf: 0, k }
}
fn load(offset: u32) -> libc::sock_filter {
    libc::sock_filter { code: LD_W_ABS, jt: 0, jf: 0, k: offset }
}
/// If the loaded word equals `k`, skip `jt` instructions, else skip `jf`.
fn jeq(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code: JEQ_K, jt, jf, k }
}
/// If the loaded word has any bit of `k` set, skip `jt`, else skip `jf`.
fn jset(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code: JSET_K, jt, jf, k }
}
fn errno(e: i32) -> u32 {
    libc::SECCOMP_RET_ERRNO | (e as u32 & libc::SECCOMP_RET_DATA)
}

/// The filter program, instruction by instruction.
pub fn program() -> Vec<libc::sock_filter> {
    let mut p = vec![
        // A call made through another architecture's numbers (a 32-bit
        // program on a 64-bit kernel) would bypass every number below: kill.
        load(OFF_ARCH),
        jeq(AUDIT_ARCH, 1, 0),
        ret(libc::SECCOMP_RET_KILL_PROCESS),
        load(OFF_NR),
    ];
    // x32 calls are x86_64 numbers with bit 30 set; refuse the whole ABI.
    #[cfg(target_arch = "x86_64")]
    p.extend([jset(0x4000_0000, 0, 1), ret(errno(libc::ENOSYS))]);
    for nr in denied() {
        p.extend([jeq(nr as u32, 0, 1), ret(errno(libc::EPERM))]);
    }
    // clone3 passes its flags in memory, which the filter cannot read. Saying
    // "not implemented" makes glibc and Rust's std fall back to clone, whose
    // flags are a register the filter can read. (EPERM would not.)
    p.extend([jeq(libc::SYS_clone3 as u32, 0, 1), ret(errno(libc::ENOSYS))]);
    p.extend([
        jeq(libc::SYS_clone as u32, 0, 3),
        load(OFF_ARG0_LO),
        jset(NS_FLAGS, 0, 1),
        ret(errno(libc::EPERM)),
        ret(libc::SECCOMP_RET_ALLOW),
    ]);
    p
}

/// Install the filter on this process; it is inherited across `fork` and
/// `execve` and cannot be removed.
pub fn install() -> Result<()> {
    let prog = program();
    let fprog = libc::sock_fprog { len: prog.len() as u16, filter: prog.as_ptr() as *mut libc::sock_filter };
    // SAFETY: PR_SET_SECCOMP reads `fprog` and the `prog` array it points to
    // during the call (the kernel copies the program); both outlive the call.
    // It requires no_new_privs to be set, which drop_privileges did.
    let r = unsafe { libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &fprog as *const libc::sock_fprog) };
    if r != 0 {
        bail!("installing the seccomp filter: {}", std::io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_jump_lands_inside_the_program() {
        let p = program();
        for (i, ins) in p.iter().enumerate() {
            if ins.code == JEQ_K || ins.code == JSET_K {
                assert!(i + 1 + (ins.jt.max(ins.jf) as usize) < p.len(), "jump at {i} leaves the program");
            }
        }
        assert_eq!(p.last().unwrap().code, RET_K, "the program must end in a return");
        assert!(p.len() < 4096, "BPF_MAXINSNS");
    }

    #[test]
    fn the_runtime_needs_none_of_the_denied_calls_for_a_command() {
        // execve, the calls that start processes, and wait are allowed;
        // listing one of them would break every command.
        for nr in [libc::SYS_execve, libc::SYS_clone, libc::SYS_wait4, libc::SYS_exit_group] {
            assert!(!denied().contains(&nr));
        }
    }
}
