//! Usage: `m11-seccomp selftest` or `m11-seccomp <program> [args...]`.
//! Sets no_new_privs, installs the filter, then either tries the blocked
//! system calls itself or execs the program under the filter.
use std::ffi::CString;

const RET_ALLOW: u32 = libc::SECCOMP_RET_ALLOW;
const RET_KILL: u32 = libc::SECCOMP_RET_KILL_PROCESS;
fn ret_errno(e: i32) -> u32 {
    libc::SECCOMP_RET_ERRNO | (e as u32 & libc::SECCOMP_RET_DATA)
}
#[cfg(target_arch = "x86_64")]
const ARCH: u32 = 0xC000_003E; // AUDIT_ARCH_X86_64
#[cfg(target_arch = "aarch64")]
const ARCH: u32 = 0xC000_00B7; // AUDIT_ARCH_AARCH64

// Offsets into struct seccomp_data { int nr; u32 arch; u64 ip; u64 args[6]; }
const OFF_NR: u32 = 0;
const OFF_ARCH: u32 = 4;
const OFF_ARG0_LO: u32 = 16; // little-endian: low half of args[0]

fn stmt(code: u16, k: u32) -> libc::sock_filter {
    libc::sock_filter { code, jt: 0, jf: 0, k }
}
fn jeq(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code: (libc::BPF_JMP | libc::BPF_JEQ | libc::BPF_K) as u16, jt, jf, k }
}
fn jset(k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code: (libc::BPF_JMP | libc::BPF_JSET | libc::BPF_K) as u16, jt, jf, k }
}
const LD_W_ABS: u16 = (libc::BPF_LD | libc::BPF_W | libc::BPF_ABS) as u16;
const RET_K: u16 = (libc::BPF_RET | libc::BPF_K) as u16;

fn denied() -> Vec<i64> {
    use libc::*;
    let mut v = vec![
        SYS_mount, SYS_umount2, SYS_pivot_root, SYS_chroot, SYS_unshare, SYS_setns,
        SYS_bpf, SYS_perf_event_open, SYS_ptrace, SYS_process_vm_readv, SYS_process_vm_writev,
        SYS_keyctl, SYS_add_key, SYS_request_key, SYS_userfaultfd,
        SYS_io_uring_setup, SYS_io_uring_enter, SYS_io_uring_register,
        SYS_kexec_load, SYS_kexec_file_load, SYS_init_module, SYS_finit_module, SYS_delete_module,
        SYS_open_by_handle_at, SYS_name_to_handle_at,
        SYS_fsopen, SYS_fsmount, SYS_fsconfig, SYS_fspick, SYS_move_mount, SYS_open_tree, SYS_mount_setattr,
        SYS_swapon, SYS_swapoff, SYS_reboot, SYS_acct, SYS_settimeofday, SYS_clock_settime,
        SYS_clock_adjtime, SYS_adjtimex, SYS_syslog, SYS_quotactl, SYS_vhangup,
    ];
    #[cfg(target_arch = "x86_64")]
    v.extend([SYS_iopl, SYS_ioperm, SYS_uselib]);
    v
}

/// Namespace flags a `clone` may not ask for.
const NS_FLAGS: u32 = (libc::CLONE_NEWUSER | libc::CLONE_NEWNS | libc::CLONE_NEWPID | libc::CLONE_NEWNET
    | libc::CLONE_NEWUTS | libc::CLONE_NEWIPC | libc::CLONE_NEWCGROUP) as u32;

fn program() -> Vec<libc::sock_filter> {
    let mut p = vec![
        stmt(LD_W_ABS, OFF_ARCH),
        jeq(ARCH, 1, 0),
        stmt(RET_K, RET_KILL),
        stmt(LD_W_ABS, OFF_NR),
    ];
    #[cfg(target_arch = "x86_64")]
    {
        // x32 system calls have bit 30 set; refuse the whole x32 ABI.
        p.push(jset(0x4000_0000, 0, 1));
        p.push(stmt(RET_K, ret_errno(libc::ENOSYS)));
    }
    for nr in denied() {
        p.push(jeq(nr as u32, 0, 1));
        p.push(stmt(RET_K, ret_errno(libc::EPERM)));
    }
    // clone3 passes its flags in memory, which BPF cannot read: say "not
    // implemented", and libc falls back to clone, whose flags we can check.
    p.push(jeq(libc::SYS_clone3 as u32, 0, 1));
    p.push(stmt(RET_K, ret_errno(libc::ENOSYS)));
    p.push(jeq(libc::SYS_clone as u32, 0, 3));
    p.push(stmt(LD_W_ABS, OFF_ARG0_LO));
    p.push(jset(NS_FLAGS, 0, 1));
    p.push(stmt(RET_K, ret_errno(libc::EPERM)));
    p.push(stmt(RET_K, RET_ALLOW));
    p
}

fn install() {
    let prog = program();
    let fprog = libc::sock_fprog { len: prog.len() as u16, filter: prog.as_ptr() as *mut _ };
    // SAFETY: prctl with plain integer arguments; the filter outlives the
    // call (the kernel copies it).
    unsafe {
        assert_eq!(libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0), 0);
        let r = libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &fprog as *const _);
        assert_eq!(r, 0, "PR_SET_SECCOMP: {}", std::io::Error::last_os_error());
    }
    eprintln!("filter: {} instructions", prog.len());
}

fn errno_of(r: i64) -> String {
    if r >= 0 { format!("ok ({r})") } else { std::io::Error::last_os_error().to_string() }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    install();
    if args.first().map(String::as_str) == Some("selftest") {
        // SAFETY: raw system calls with arguments the kernel validates; each is
        // expected to be refused by the filter before it does anything.
        unsafe {
            println!("unshare(NEWUSER)  -> {}", errno_of(libc::syscall(libc::SYS_unshare, libc::CLONE_NEWUSER)));
            println!("clone(NEWUSER)    -> {}", errno_of(libc::syscall(libc::SYS_clone, libc::CLONE_NEWUSER | libc::SIGCHLD, 0, 0, 0, 0)));
            println!("clone3            -> {}", errno_of(libc::syscall(libc::SYS_clone3, 0, 0)));
            println!("mount             -> {}", errno_of(libc::syscall(libc::SYS_mount, 0, 0, 0, 0, 0)));
            println!("bpf               -> {}", errno_of(libc::syscall(libc::SYS_bpf, 0, 0, 0)));
            println!("io_uring_setup    -> {}", errno_of(libc::syscall(libc::SYS_io_uring_setup, 1, 0)));
            println!("keyctl            -> {}", errno_of(libc::syscall(libc::SYS_keyctl, 0, 0)));
            println!("ptrace(TRACEME)   -> {}", errno_of(libc::syscall(libc::SYS_ptrace, 0, 0, 0, 0)));
            println!("getpid (allowed)  -> {}", errno_of(libc::syscall(libc::SYS_getpid)));
            let pid = libc::fork();
            if pid == 0 { libc::_exit(7); }
            let mut st = 0;
            libc::waitpid(pid, &mut st, 0);
            println!("fork (allowed)    -> child exit {}", libc::WEXITSTATUS(st));
        }
        return;
    }
    let c: Vec<CString> = args.iter().map(|a| CString::new(a.as_str()).unwrap()).collect();
    let mut ptrs: Vec<*const libc::c_char> = c.iter().map(|s| s.as_ptr()).collect();
    ptrs.push(std::ptr::null());
    // SAFETY: argv is a null-terminated array of valid C strings that live
    // until execvp replaces the process.
    unsafe { libc::execvp(ptrs[0], ptrs.as_ptr()) };
    panic!("execvp: {}", std::io::Error::last_os_error());
}
