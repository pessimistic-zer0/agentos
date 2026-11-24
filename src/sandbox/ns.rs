//! The sandbox helper: the code that runs between "the runtime starts a
//! command" and "the command runs", building the sandbox around it.
//!
//! The runtime starts a fresh copy of its own binary as
//! `norn __sandbox-init <spec as JSON>`. A fresh process has exactly one
//! thread, which matters: `unshare(CLONE_NEWUSER)` refuses to run in a
//! process with several threads, and forking a multi-threaded process is
//! risky. (Go container tools use the same re-exec trick for the same reason.)
//!
//! Three processes are involved:
//!
//! ```text
//! H  helper, started by the runtime, in the runtime's namespaces
//!    joins the exec cgroup; unshare(user, mount, pid, net); writes uid/gid maps
//!    └─ C  PID 1 of the new PID namespace
//!          builds the new root, pivot_root, loopback up,
//!          drops capabilities, no_new_privs, Landlock, seccomp
//!          └─ G  execve(the allowed program)
//! ```
//!
//! C stays alive as a tiny init: it reaps orphaned processes and reports G's
//! exit status. When C exits, the kernel kills everything left in the PID
//! namespace. The order of steps follows bubblewrap (`bubblewrap.c`, `main`),
//! because most namespace bugs are ordering bugs.
//!
//! Status reporting: file descriptor 3 is a pipe back to the runtime. Setup
//! errors are written there as `error: ...`. G's copy of fd 3 is marked
//! close-on-exec, so a successful `execve` closes it silently; a failed one
//! writes `exec-failed: ...`. When G ends, C writes `exit N` or `signal N`.

use std::ffi::CString;
use std::fs;
use std::io::Write;
use std::os::fd::{FromRawFd, OwnedFd};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use nix::mount::{mount, umount2, MntFlags, MsFlags};
use nix::sched::{unshare, CloneFlags};
use nix::sys::signal::Signal;
use nix::sys::statvfs::{statvfs, FsFlags};
use nix::sys::wait::{waitpid, WaitStatus};
use nix::unistd::{chdir, execve, fork, getppid, pivot_root, ForkResult, Pid};
use serde::{Deserialize, Serialize};

use super::landlock;

/// Everything the helper needs, passed as JSON on its command line.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxSpec {
    /// The workspace on the host; mounted read-write at `/work`.
    pub workspace: PathBuf,
    /// Host folders mounted read-only at the same path.
    pub ro_mounts: Vec<PathBuf>,
    /// The program, by its real host path (it lies under one of `ro_mounts`).
    pub program: PathBuf,
    pub argv: Vec<String>,
    pub env: Vec<(String, String)>,
    /// The exec's cgroup folder; the helper moves itself in first.
    pub cgroup: PathBuf,
    /// The runtime's pid, to detect that it died before `PR_SET_PDEATHSIG`.
    pub parent_pid: i32,
    pub uid: u32,
    pub gid: u32,
    /// No file the command writes may grow past this (`RLIMIT_FSIZE`; D-117).
    pub max_file_bytes: u64,
    /// `filesystem.ro_dirs`: (the folder in the original repository, its
    /// path relative to the workspace). Mounted read-only at
    /// `/work/<path>` (D-122).
    #[serde(default)]
    pub ro_dirs: Vec<(PathBuf, String)>,
}

/// The fd the status pipe is placed on in the helper.
pub const STATUS_FD: i32 = 3;
/// Exit code of the helper when setup fails.
pub const SETUP_FAILED: i32 = 125;

fn status_file() -> fs::File {
    // SAFETY: the runtime placed the write end of the status pipe on fd 3
    // before starting the helper (see sandbox::run). We take ownership of it
    // exactly once, here.
    fs::File::from(unsafe { OwnedFd::from_raw_fd(STATUS_FD) })
}

/// The steps of `probe_in_child`, in order; the child's exit code is the
/// number of the step that failed (1-based), 0 if none did.
pub const PROBE_STEPS: [&str; 6] = [
    "creating a user namespace",
    "writing /proc/self/setgroups",
    "writing /proc/self/uid_map",
    "writing /proc/self/gid_map",
    "changing a mount inside the user namespace",
    "bringing up the loopback interface inside the user namespace",
];

/// Do the first steps of building a sandbox and leave through `_exit`:
/// code 0 if all worked, otherwise the number of the failed step, with the
/// error number written to fd 2 as decimal digits.
///
/// It goes as far as steps that need a privilege *inside* the new user
/// namespace. That matters on Ubuntu: there the kernel lets a program
/// without an AppArmor profile create the namespace and then refuses what
/// it tries to do in it (D-093).
///
/// This runs in a forked child of the runtime, between `fork` and what
/// would be `exec` (`Command::pre_exec`), so it works whatever program the
/// runtime is part of: `norn`, a benchmark, a test (bug B-013). In that
/// place only async-signal-safe calls are allowed, because another thread
/// of the parent may have held a lock (for example the allocator's) at the
/// moment of the fork. So: raw system calls only, no allocation, no
/// `format!`; the map text is prepared by the caller before the fork.
///
/// # Safety
/// Must only be called in a freshly forked child. It never returns.
pub unsafe fn probe_in_child(uid_map: &[u8], gid_map: &[u8]) -> ! {
    unsafe fn fail(step: i32) -> ! {
        // The error number as digits, without allocating.
        let mut e = *libc::__errno_location();
        let mut buf = [0u8; 12];
        let mut at = buf.len();
        loop {
            at -= 1;
            buf[at] = b'0' + (e % 10) as u8;
            e /= 10;
            if e == 0 {
                break;
            }
        }
        libc::write(2, buf[at..].as_ptr().cast(), buf.len() - at);
        libc::_exit(step)
    }
    unsafe fn write_file(path: &std::ffi::CStr, text: &[u8]) -> bool {
        let fd = libc::open(path.as_ptr(), libc::O_WRONLY | libc::O_CLOEXEC);
        if fd < 0 {
            return false;
        }
        let ok = libc::write(fd, text.as_ptr().cast(), text.len()) == text.len() as isize;
        let e = *libc::__errno_location();
        libc::close(fd);
        *libc::__errno_location() = e;
        ok
    }
    if libc::unshare(libc::CLONE_NEWUSER | libc::CLONE_NEWNS | libc::CLONE_NEWNET) != 0 {
        fail(1)
    }
    if !write_file(c"/proc/self/setgroups", b"deny") {
        fail(2)
    }
    if !write_file(c"/proc/self/uid_map", uid_map) {
        fail(3)
    }
    if !write_file(c"/proc/self/gid_map", gid_map) {
        fail(4)
    }
    if libc::mount(std::ptr::null(), c"/".as_ptr(), std::ptr::null(), libc::MS_REC | libc::MS_PRIVATE, std::ptr::null()) != 0 {
        fail(5)
    }
    // The same two ioctl calls as `loopback_up`, without its error text.
    let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
    if fd < 0 {
        fail(6)
    }
    let mut ifr: libc::ifreq = std::mem::zeroed();
    ifr.ifr_name[0] = b'l' as libc::c_char;
    ifr.ifr_name[1] = b'o' as libc::c_char;
    if libc::ioctl(fd, libc::SIOCGIFFLAGS, &mut ifr) != 0 {
        fail(6)
    }
    ifr.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
    if libc::ioctl(fd, libc::SIOCSIFFLAGS, &ifr) != 0 {
        fail(6)
    }
    libc::_exit(0)
}

/// Entry point of `norn __sandbox-init <json>`. Never returns.
pub fn helper_main(spec_json: &str) -> ! {
    let mut status = status_file();
    let code = match helper(spec_json, &mut status) {
        Ok(code) => code,
        Err(e) => {
            let _ = writeln!(status, "error: {e:#}");
            SETUP_FAILED
        }
    };
    std::process::exit(code)
}

/// Process H.
fn helper(spec_json: &str, status: &mut fs::File) -> Result<i32> {
    let spec: SandboxSpec = serde_json::from_str(spec_json).context("bad sandbox spec")?;
    // Mark fd 3 close-on-exec now. H and C never exec, so they keep it; G's
    // execve closes it, which is how the runtime learns the exec succeeded.
    nix::fcntl::fcntl(&*status, nix::fcntl::FcntlArg::F_SETFD(nix::fcntl::FdFlag::FD_CLOEXEC))?;

    // Die with the runtime. If the runtime is killed (kill -9), the kernel
    // sends us SIGKILL. Check the parent afterwards: if it died before the
    // prctl, we were already re-parented and would never get the signal.
    nix::sys::prctl::set_pdeathsig(Signal::SIGKILL)?;
    if getppid().as_raw() != spec.parent_pid {
        bail!("runtime exited before the sandbox started");
    }

    // 1. Join the exec's cgroup before anything else, so every process the
    //    command ever starts is counted and can be killed as a group.
    fs::write(spec.cgroup.join("cgroup.procs"), "0").with_context(|| format!("joining cgroup {}", spec.cgroup.display()))?;

    // 2. New user, mount, PID and network namespaces. After this call we have
    //    every capability, but only over things these namespaces own.
    unshare(CloneFlags::CLONE_NEWUSER | CloneFlags::CLONE_NEWNS | CloneFlags::CLONE_NEWPID | CloneFlags::CLONE_NEWNET)
        .context("unshare(user, mount, pid, net) failed; are unprivileged user namespaces enabled?")?;

    // 3. Map our own uid and gid to themselves inside. An unprivileged
    //    process may write a one-line map of its own ids. "deny" on setgroups
    //    must come first, or the kernel refuses the gid map.
    fs::write("/proc/self/setgroups", "deny").context("writing /proc/self/setgroups")?;
    fs::write("/proc/self/uid_map", format!("{0} {0} 1", spec.uid)).context("writing uid_map")?;
    fs::write("/proc/self/gid_map", format!("{0} {0} 1", spec.gid)).context("writing gid_map")?;

    // 4. unshare(CLONE_NEWPID) does not move us; our next child is PID 1.
    // SAFETY: H has a single thread (fresh process, no threads started), so
    // the child may run any code after fork.
    match unsafe { fork() }.context("fork of sandbox init")? {
        ForkResult::Child => {
            let code = match init(&spec, status) {
                Ok(code) => code,
                Err(e) => {
                    let _ = writeln!(status, "error: {e:#}");
                    SETUP_FAILED
                }
            };
            std::process::exit(code)
        }
        ForkResult::Parent { child } => Ok(match waitpid(child, None)? {
            WaitStatus::Exited(_, code) => code,
            WaitStatus::Signaled(_, sig, _) => 128 + sig as i32,
            _ => SETUP_FAILED,
        }),
    }
}

/// Process C: PID 1 inside the sandbox.
fn init(spec: &SandboxSpec, status: &mut fs::File) -> Result<i32> {
    // If H dies, C dies; when C (PID 1) dies, the kernel kills the rest.
    nix::sys::prctl::set_pdeathsig(Signal::SIGKILL)?;
    build_root(spec)?;
    loopback_up().context("bringing up loopback")?;
    chdir("/work")?;
    drop_privileges()?;
    let ro: Vec<&Path> = spec.ro_mounts.iter().map(|p| p.as_path()).collect();
    landlock::restrict(&ro).context("applying Landlock")?;
    // One file can never be larger than the whole workspace may be. The
    // kernel enforces it on every write: EFBIG, or SIGXFSZ for programs that
    // do not ignore that signal. Inherited by the command (D-117).
    let cap = libc::rlimit { rlim_cur: spec.max_file_bytes, rlim_max: spec.max_file_bytes };
    // SAFETY: setrlimit reads one rlimit struct that lives on this stack.
    if unsafe { libc::setrlimit(libc::RLIMIT_FSIZE, &cap) } != 0 {
        bail!("setting RLIMIT_FSIZE: {}", std::io::Error::last_os_error());
    }
    // Last: from here on C makes only the calls an init needs (fork, wait,
    // write), none of them refused.
    super::seccomp::install()?;

    let prog = CString::new(spec.program.as_os_str().as_encoded_bytes())?;
    let argv: Vec<CString> = spec.argv.iter().map(|a| CString::new(a.as_str())).collect::<Result<_, _>>()?;
    let env: Vec<CString> =
        spec.env.iter().map(|(k, v)| CString::new(format!("{k}={v}"))).collect::<Result<_, _>>()?;

    // SAFETY: C is single-threaded (forked from single-threaded H).
    match unsafe { fork() }.context("fork of the command")? {
        ForkResult::Child => {
            // Process G. Close every inherited fd above the status pipe, so
            // nothing the runtime had open leaks in. SAFETY: close_range only
            // closes descriptors; fds 0-3 stay.
            unsafe { libc::syscall(libc::SYS_close_range, (STATUS_FD + 1) as libc::c_uint, libc::c_uint::MAX, 0) };
            let err = execve(&prog, &argv, &env).unwrap_err();
            let _ = writeln!(status, "exec-failed: {}: {}", spec.program.display(), err.desc());
            std::process::exit(127)
        }
        ForkResult::Parent { child } => reap_until(child, status),
    }
}

/// Act as init: reap every child (orphans re-parent to PID 1) until the
/// command itself ends, then report its status.
fn reap_until(cmd: Pid, status: &mut fs::File) -> Result<i32> {
    loop {
        match waitpid(None, None) {
            Ok(WaitStatus::Exited(pid, code)) if pid == cmd => {
                let _ = writeln!(status, "exit {code}");
                return Ok(code);
            }
            Ok(WaitStatus::Signaled(pid, sig, _)) if pid == cmd => {
                let _ = writeln!(status, "signal {}", sig as i32);
                return Ok(128 + sig as i32);
            }
            Ok(_) => continue,
            Err(nix::errno::Errno::EINTR) => continue,
            Err(e) => bail!("waitpid: {e}"),
        }
    }
}

/// Build the sandbox's root folder and switch to it. After this, the only
/// host paths visible are the workspace (at /work), the read-only system
/// folders, five device files and a fresh /proc.
fn build_root(spec: &SandboxSpec) -> Result<()> {
    let none: Option<&str> = None;
    // Stop mount events travelling between the sandbox and the host.
    mount(none, "/", none, MsFlags::MS_REC | MsFlags::MS_PRIVATE, none).context("making / private")?;
    // A tmpfs over /tmp is the staging area. Only this namespace sees it.
    mount(Some("tmpfs"), "/tmp", Some("tmpfs"), MsFlags::MS_NOSUID | MsFlags::MS_NODEV, Some("mode=0755"))
        .context("mounting staging tmpfs")?;
    chdir("/tmp")?;
    fs::create_dir("newroot")?;
    mount(Some("newroot"), "newroot", none, MsFlags::MS_BIND | MsFlags::MS_REC, none).context("binding newroot")?;
    fs::create_dir("oldroot")?;
    // First pivot: the tmpfs becomes /, the host's root moves to /oldroot.
    // Host paths are now reached as /oldroot/<path>, including paths under
    // the host's /tmp (our tmpfs moved away from there), so a workspace under
    // /tmp still works. This is bubblewrap's trick.
    pivot_root("/tmp", "/tmp/oldroot").context("first pivot_root")?;
    chdir("/")?;
    let host = |p: &Path| Path::new("/oldroot").join(p.strip_prefix("/").unwrap_or(p));
    let new = |p: &str| PathBuf::from("/newroot").join(p.trim_start_matches('/'));

    // /work: the workspace, read-write, but no setuid files and no devices.
    fs::create_dir_all(new("work"))?;
    bind(&host(&spec.workspace), &new("work"), false)?;
    remount(&new("work"), MsFlags::MS_NOSUID | MsFlags::MS_NODEV)?;

    // Folders of the original repository, read-only, at their place inside
    // /work (D-122). The mount point is an empty folder the runtime made in
    // the workspace. Code from an earlier command could have replaced it or
    // a folder above it with a symlink, and a mount follows symlinks, so
    // every step of the way is checked to be a real folder first. Nothing
    // can change it between the check and the mount: no sandboxed process
    // exists yet.
    for (src, rel) in &spec.ro_dirs {
        let mut at = new("work");
        for comp in rel.split('/') {
            at.push(comp);
            let meta = fs::symlink_metadata(&at).with_context(|| format!("the mount point for read-only folder `{rel}` is missing in the workspace"))?;
            if !meta.is_dir() {
                bail!("the mount point for read-only folder `{rel}` is not a plain folder in the workspace any more");
            }
        }
        let src = host(src);
        if !fs::symlink_metadata(&src).map(|m| m.is_dir()).unwrap_or(false) {
            bail!("read-only folder `{rel}` is no longer a folder in the repository");
        }
        bind(&src, &at, false)?;
        remount(&at, MsFlags::MS_RDONLY | MsFlags::MS_NOSUID | MsFlags::MS_NODEV)?;
    }

    // Read-only system folders from the policy (D-012).
    for m in &spec.ro_mounts {
        let src = host(m);
        let dst = new(m.to_str().context("non-UTF-8 mount path")?);
        let meta = match fs::symlink_metadata(&src) {
            Ok(meta) => meta,
            Err(_) => continue, // e.g. no /lib64 on this distro: nothing to mount
        };
        fs::create_dir_all(dst.parent().expect("mount path has a parent"))?;
        if meta.file_type().is_symlink() {
            // /bin -> usr/bin on many distros: recreate the link, mount nothing.
            symlink(fs::read_link(&src)?, &dst)?;
        } else {
            fs::create_dir_all(&dst)?;
            bind(&src, &dst, true)?;
            remount(&dst, MsFlags::MS_RDONLY | MsFlags::MS_NOSUID | MsFlags::MS_NODEV)?;
        }
    }

    // /dev: a small tmpfs with five device files bound in from the host. A
    // process in a user namespace cannot create device files (mknod), so
    // binding the host's is the only way.
    fs::create_dir(new("dev"))?;
    mount(Some("tmpfs"), &new("dev"), Some("tmpfs"), MsFlags::MS_NOSUID | MsFlags::MS_NOEXEC, Some("mode=0755"))?;
    for d in landlock::DEVICES {
        let dst = new(d);
        fs::File::create(&dst)?; // a mount point must exist
        bind(&host(Path::new(d)), &dst, false)?;
    }
    for (link, target) in [("fd", "/proc/self/fd"), ("stdin", "/proc/self/fd/0"), ("stdout", "/proc/self/fd/1"), ("stderr", "/proc/self/fd/2")] {
        symlink(target, new("dev").join(link))?;
    }

    // A fresh /proc for the new PID namespace: it lists only sandbox
    // processes. Mounted while /oldroot/proc is still visible, because the
    // kernel only lets a user namespace mount proc if a full proc is already
    // visible to it.
    fs::create_dir(new("proc"))?;
    mount(Some("proc"), &new("proc"), Some("proc"), MsFlags::MS_NOSUID | MsFlags::MS_NODEV | MsFlags::MS_NOEXEC, none)
        .context("mounting /proc")?;

    // /tmp: private scratch space, counted against the cgroup's memory limit.
    fs::create_dir(new("tmp"))?;
    mount(Some("tmpfs"), &new("tmp"), Some("tmpfs"), MsFlags::MS_NOSUID | MsFlags::MS_NODEV, Some("mode=1777"))?;

    // Drop the host's root.
    mount(none, "/oldroot", none, MsFlags::MS_REC | MsFlags::MS_PRIVATE, none)?;
    umount2("/oldroot", MntFlags::MNT_DETACH).context("detaching old root")?;
    // Second pivot: /newroot becomes /. pivot_root(".", ".") stacks the old
    // root on top of the new one; unmounting "." removes it. (runc and LXC
    // do the same.)
    chdir("/newroot")?;
    pivot_root(".", ".").context("second pivot_root")?;
    umount2(".", MntFlags::MNT_DETACH)?;
    chdir("/")?;
    // The root folder itself is read-only: nothing can be created next to /work.
    mount(none, "/", none, MsFlags::MS_REMOUNT | MsFlags::MS_BIND | MsFlags::MS_RDONLY | MsFlags::MS_NOSUID | MsFlags::MS_NODEV, none)
        .context("making / read-only")?;
    Ok(())
}

fn bind(src: &Path, dst: &Path, recursive: bool) -> Result<()> {
    let mut flags = MsFlags::MS_BIND;
    if recursive {
        flags |= MsFlags::MS_REC;
    }
    mount(Some(src), dst, None::<&str>, flags, None::<&str>).with_context(|| format!("bind {} -> {}", src.display(), dst.display()))
}

/// Change a bind mount's flags. Inside a user namespace the kernel refuses to
/// *clear* flags that the host set on the original mount (they are "locked"),
/// so the current flags are read back and kept. `MS_REC` is not used: a
/// sub-mount below a read-only folder keeps its own flags, and Landlock still
/// limits it to read-only access.
fn remount(dst: &Path, add: MsFlags) -> Result<()> {
    let cur = statvfs(dst)?.flags();
    let mut flags = MsFlags::MS_REMOUNT | MsFlags::MS_BIND | add;
    for (st, ms) in [
        (FsFlags::ST_RDONLY, MsFlags::MS_RDONLY),
        (FsFlags::ST_NOSUID, MsFlags::MS_NOSUID),
        (FsFlags::ST_NODEV, MsFlags::MS_NODEV),
        (FsFlags::ST_NOEXEC, MsFlags::MS_NOEXEC),
        (FsFlags::ST_NOATIME, MsFlags::MS_NOATIME),
        (FsFlags::ST_NODIRATIME, MsFlags::MS_NODIRATIME),
        (FsFlags::ST_RELATIME, MsFlags::MS_RELATIME),
    ] {
        if cur.contains(st) {
            flags |= ms;
        }
    }
    mount(None::<&str>, dst, None::<&str>, flags, None::<&str>).with_context(|| format!("remount {}", dst.display()))
}

/// A new network namespace has only `lo`, and it is down. Bring it up so
/// programs that talk to themselves over 127.0.0.1 work; there is still no
/// route anywhere else.
fn loopback_up() -> Result<()> {
    // SAFETY: plain socket/ioctl calls on a struct we own and zeroed; the
    // name is copied in within bounds; the fd is closed on every path.
    unsafe {
        let fd = libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0);
        if fd < 0 {
            bail!("socket: {}", std::io::Error::last_os_error());
        }
        let mut ifr: libc::ifreq = std::mem::zeroed();
        for (i, b) in b"lo".iter().enumerate() {
            ifr.ifr_name[i] = *b as libc::c_char;
        }
        let ok = libc::ioctl(fd, libc::SIOCGIFFLAGS, &mut ifr) == 0 && {
            ifr.ifr_ifru.ifru_flags |= libc::IFF_UP as libc::c_short;
            libc::ioctl(fd, libc::SIOCSIFFLAGS, &ifr) == 0
        };
        let err = std::io::Error::last_os_error();
        libc::close(fd);
        if !ok {
            bail!("ioctl on lo: {err}");
        }
    }
    Ok(())
}

/// Give up every capability for good, and forbid gaining new rights through
/// setuid programs (`no_new_privs`).
fn drop_privileges() -> Result<()> {
    // SAFETY: prctl and capset with constant arguments and a header/data pair
    // we own; none of these calls touch Rust-managed memory except to read it.
    unsafe {
        // Bounding set: caps that could ever be gained again. Needs
        // CAP_SETPCAP, which we still have inside the user namespace.
        let last = fs::read_to_string("/proc/sys/kernel/cap_last_cap")
            .ok()
            .and_then(|s| s.trim().parse::<libc::c_ulong>().ok())
            .unwrap_or(40);
        for cap in 0..=last {
            if libc::prctl(libc::PR_CAPBSET_DROP, cap, 0, 0, 0) != 0 {
                bail!("PR_CAPBSET_DROP {cap}: {}", std::io::Error::last_os_error());
            }
        }
        if libc::prctl(libc::PR_CAP_AMBIENT, libc::PR_CAP_AMBIENT_CLEAR_ALL, 0, 0, 0) != 0 {
            bail!("clearing ambient caps: {}", std::io::Error::last_os_error());
        }
    }
    nix::sys::prctl::set_no_new_privs().context("PR_SET_NO_NEW_PRIVS")?;
    // Effective, permitted and inheritable sets to zero (capset, version 3).
    #[repr(C)]
    struct Header {
        version: u32,
        pid: i32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Data {
        effective: u32,
        permitted: u32,
        inheritable: u32,
    }
    let header = Header { version: 0x2008_0522, pid: 0 }; // _LINUX_CAPABILITY_VERSION_3
    let data = [Data { effective: 0, permitted: 0, inheritable: 0 }; 2];
    // SAFETY: capset reads one header and two data structs, laid out as the
    // kernel's __user_cap_header_struct / __user_cap_data_struct (repr(C)).
    let r = unsafe { libc::syscall(libc::SYS_capset, &header as *const Header, data.as_ptr()) };
    if r != 0 {
        bail!("capset: {}", std::io::Error::last_os_error());
    }
    Ok(())
}
