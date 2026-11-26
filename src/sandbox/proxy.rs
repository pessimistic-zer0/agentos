//! The network proxy for commands (M19; D-153, D-154, D-155).
//!
//! A sandboxed command has an empty network namespace: no route anywhere.
//! When the policy lists hosts (`[network] allow`), the sandbox's init
//! process makes one listening socket on `127.0.0.1:3128` inside that
//! namespace and hands it to the runtime (`ns.rs`). This file is the
//! runtime's side: it accepts the command's connections on that socket and,
//! for each one:
//!
//! 1. reads the request head: `CONNECT host:port` (a tunnel for HTTPS; the
//!    proxy never sees the content) or a plain-HTTP request with a full URL
//!    (`GET http://host/path`);
//! 2. checks `host:port` against the policy's host rules; no match means
//!    refused (`403`), as everywhere else (design rule 2);
//! 3. resolves the name itself (the sandbox has no DNS) and refuses
//!    addresses on this machine or its link (loopback, link-local, ...)
//!    unless the rule named that address by number, so that an allowed name
//!    cannot be pointed at the runtime's own machine;
//! 4. connects, and copies bytes both ways, counting them.
//!
//! Every connection, allowed or not, becomes a `Connection` record. A
//! connection's byte counts are only known when it ends, and tools keep
//! connections open until they exit (E1), so the records are collected when
//! the command has ended and the proxy has closed whatever was still open.

use std::io::{Read, Write};
use std::net::{IpAddr, Shutdown, SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::policy::{HostPattern, HostRule};

/// The longest request head the proxy reads before giving up on a client.
const MAX_HEAD: usize = 16 * 1024;
/// How long a client has to send its request head, and the proxy to connect.
const HEAD_TIMEOUT: Duration = Duration::from_secs(10);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
/// Connections open at the same time; more are closed at once.
const MAX_ACTIVE: usize = 128;
/// Records kept per command; past this only a count is kept, so a command
/// cannot fill the log by trying many hosts.
pub const MAX_RECORDS: usize = 256;

/// One connection a command asked the proxy for.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Connection {
    /// As the command asked for it (lower case).
    pub host: String,
    pub port: u16,
    /// `CONNECT` for HTTPS tunnels, or the plain-HTTP method (`GET`, ...).
    pub method: String,
    /// Did the policy allow it?
    pub allowed: bool,
    /// The rule that allowed it (`network.allow[0] "pypi.org"`), or why it
    /// was refused.
    pub rule: String,
    /// Bytes from the command to the server, and back.
    pub bytes_up: u64,
    pub bytes_down: u64,
    /// Allowed, but it could not be made (no such host, refused, timeout).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// What the proxy saw during one command.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub connections: Vec<Connection>,
    /// Connections past `MAX_RECORDS`, counted but not recorded.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub dropped: u64,
}

fn is_zero(n: &u64) -> bool {
    *n == 0
}

struct Shared {
    rules: Vec<(String, HostRule)>,
    stop: AtomicBool,
    /// The listening socket, once received, so `finish` can wake `accept`.
    listener: Mutex<Option<i32>>,
    /// Both sockets of every open connection, so `finish` can end them.
    active: Mutex<Vec<(u64, TcpStream)>>,
    records: Mutex<Report>,
    next: AtomicU64,
}

/// A running proxy for one command.
pub struct Proxy {
    shared: Arc<Shared>,
    thread: JoinHandle<()>,
}

impl Proxy {
    /// Start waiting for the sandbox's listening socket on `chan` (the
    /// runtime's end of the socket pair), then serve it. If the sandbox
    /// fails before sending it, `chan` reaches its end and the proxy simply
    /// has nothing to do.
    pub fn start(chan: OwnedFd, rules: Vec<(String, HostRule)>) -> Proxy {
        let shared = Arc::new(Shared {
            rules,
            stop: AtomicBool::new(false),
            listener: Mutex::new(None),
            active: Mutex::new(Vec::new()),
            records: Mutex::new(Report::default()),
            next: AtomicU64::new(0),
        });
        let s = shared.clone();
        let thread = std::thread::spawn(move || {
            let Some(listener) = receive_listener(&chan) else { return };
            drop(chan);
            *s.listener.lock().unwrap() = Some(listener.as_raw_fd());
            if s.stop.load(Ordering::SeqCst) {
                return;
            }
            serve(&listener, &s);
            *s.listener.lock().unwrap() = None;
        });
        Proxy { shared, thread }
    }

    /// The command has ended: stop accepting, end every open connection,
    /// and return what happened.
    pub fn finish(self) -> Report {
        self.shared.stop.store(true, Ordering::SeqCst);
        if let Some(fd) = *self.shared.listener.lock().unwrap() {
            // SAFETY: shutdown on a descriptor the serving thread still owns
            // (it clears this slot only after `serve` returns, under the same
            // lock). On Linux it wakes a thread blocked in accept().
            unsafe { libc::shutdown(fd, libc::SHUT_RDWR) };
        }
        for (_, s) in self.shared.active.lock().unwrap().iter() {
            let _ = s.shutdown(Shutdown::Both);
        }
        let _ = self.thread.join();
        std::mem::take(&mut *self.shared.records.lock().unwrap())
    }
}

/// Receive the one descriptor the sandbox sends (D-155).
fn receive_listener(chan: &OwnedFd) -> Option<TcpListener> {
    use nix::sys::socket::{recvmsg, ControlMessageOwned, MsgFlags};
    let mut byte = [0u8; 1];
    let mut iov = [std::io::IoSliceMut::new(&mut byte)];
    let mut space = nix::cmsg_space!([std::os::fd::RawFd; 1]);
    let msg = loop {
        match recvmsg::<()>(chan.as_raw_fd(), &mut iov, Some(&mut space), MsgFlags::MSG_CMSG_CLOEXEC) {
            Err(nix::errno::Errno::EINTR) => continue,
            Err(_) => return None,
            Ok(m) => break m,
        }
    };
    for c in msg.cmsgs().ok()? {
        if let ControlMessageOwned::ScmRights(fds) = c {
            let mut it = fds.into_iter();
            let first = it.next()?;
            for extra in it {
                // SAFETY: descriptors just received are ours to close.
                unsafe { libc::close(extra) };
            }
            // SAFETY: the kernel just gave us this descriptor; nothing else owns it.
            return Some(unsafe { TcpListener::from_raw_fd(first) });
        }
    }
    None
}

fn serve(listener: &TcpListener, s: &Arc<Shared>) {
    let mut workers = Vec::new();
    loop {
        let client = match listener.accept() {
            Ok((c, _)) => c,
            Err(_) if s.stop.load(Ordering::SeqCst) => break,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => {
                // Out of descriptors or similar: wait a little, never spin.
                std::thread::sleep(Duration::from_millis(10));
                continue;
            }
        };
        if s.stop.load(Ordering::SeqCst) {
            break;
        }
        let id = s.next.fetch_add(1, Ordering::SeqCst);
        {
            let mut active = s.active.lock().unwrap();
            if active.len() >= MAX_ACTIVE {
                continue; // dropping `client` closes it
            }
            if let Ok(c) = client.try_clone() {
                active.push((id, c));
            }
        }
        let s2 = s.clone();
        workers.push(std::thread::spawn(move || {
            handle(id, client, &s2);
            s2.active.lock().unwrap().retain(|(i, _)| *i != id);
        }));
        workers.retain(|w| !w.is_finished());
    }
    for w in workers {
        let _ = w.join();
    }
}

/// What the request head asked for.
struct Request {
    method: String,
    host: String,
    port: u16,
    /// For plain HTTP: the head to send to the server, with the full URL
    /// turned into a path. Empty for `CONNECT`.
    forward: Vec<u8>,
    /// Bytes the client sent after the head (the start of a TLS hello, a
    /// request body).
    rest: Vec<u8>,
}

fn record(s: &Shared, c: Connection) {
    let mut r = s.records.lock().unwrap();
    if r.connections.len() < MAX_RECORDS {
        r.connections.push(c);
    } else {
        r.dropped += 1;
    }
}

fn answer(client: &mut TcpStream, status: &str, text: &str) {
    let body = format!("norn: {text}\n");
    let _ = write!(client, "HTTP/1.1 {status}\r\nContent-Type: text/plain\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
}

fn handle(id: u64, mut client: TcpStream, s: &Shared) {
    let _ = client.set_read_timeout(Some(HEAD_TIMEOUT));
    let req = match read_request(&mut client) {
        Ok(r) => r,
        Err(why) => {
            answer(&mut client, "400 Bad Request", why);
            return;
        }
    };
    let mut rec = Connection {
        host: req.host.clone(),
        port: req.port,
        method: req.method.clone(),
        allowed: false,
        rule: "no matching rule".into(),
        bytes_up: 0,
        bytes_down: 0,
        error: None,
    };
    let Some((rule_text, rule)) = s.rules.iter().find(|(_, r)| r.matches(&req.host, req.port)) else {
        answer(&mut client, "403 Forbidden", &format!("{}:{} is not in network.allow", req.host, req.port));
        record(s, rec);
        return;
    };
    rec.rule = rule_text.clone();
    // An address rule connects to that address only; a name is resolved
    // here, and addresses on this machine or its link are refused.
    let by_number = matches!(rule.host, HostPattern::Ip(_));
    let addrs: Vec<SocketAddr> = match (req.host.as_str(), req.port).to_socket_addrs() {
        Ok(a) => a.collect(),
        Err(e) => {
            rec.allowed = true;
            rec.error = Some(format!("cannot resolve: {e}"));
            answer(&mut client, "502 Bad Gateway", &format!("cannot resolve {}: {e}", req.host));
            record(s, rec);
            return;
        }
    };
    let usable: Vec<SocketAddr> = addrs.iter().copied().filter(|a| by_number || !is_local(a.ip())).collect();
    if usable.is_empty() {
        rec.rule = format!("{} resolves only to local addresses ({})", req.host, addrs.iter().map(|a| a.ip().to_string()).collect::<Vec<_>>().join(", "));
        answer(&mut client, "403 Forbidden", &rec.rule);
        record(s, rec);
        return;
    }
    rec.allowed = true;
    let mut last_err = String::new();
    let server = usable.iter().find_map(|a| match TcpStream::connect_timeout(a, CONNECT_TIMEOUT) {
        Ok(t) => Some(t),
        Err(e) => {
            last_err = e.to_string();
            None
        }
    });
    let Some(mut server) = server else {
        rec.error = Some(format!("cannot connect: {last_err}"));
        answer(&mut client, "502 Bad Gateway", &format!("cannot connect to {}:{}: {last_err}", req.host, req.port));
        record(s, rec);
        return;
    };
    if let Ok(c) = server.try_clone() {
        s.active.lock().unwrap().push((id, c));
    }
    if s.stop.load(Ordering::SeqCst) {
        let _ = server.shutdown(Shutdown::Both);
    }
    let _ = client.set_read_timeout(None);
    let mut up = 0u64;
    if req.method == "CONNECT" {
        let _ = client.write_all(b"HTTP/1.1 200 Connection established\r\n\r\n");
    } else if server.write_all(&req.forward).is_ok() {
        up += req.forward.len() as u64;
    }
    if !req.rest.is_empty() && server.write_all(&req.rest).is_ok() {
        up += req.rest.len() as u64;
    }
    let (mut c2, mut s2) = match (client.try_clone(), server.try_clone()) {
        (Ok(c), Ok(sv)) => (c, sv),
        _ => {
            record(s, rec);
            return;
        }
    };
    let upward = std::thread::spawn(move || pump(&mut c2, &mut s2));
    rec.bytes_down = pump(&mut server, &mut client);
    rec.bytes_up = up + upward.join().unwrap_or(0);
    record(s, rec);
}

/// Copy until the source ends, then tell the other side no more is coming.
fn pump(from: &mut TcpStream, to: &mut TcpStream) -> u64 {
    let mut buf = [0u8; 32 * 1024];
    let mut n = 0u64;
    loop {
        match from.read(&mut buf) {
            Ok(0) | Err(_) => break,
            Ok(k) => {
                if to.write_all(&buf[..k]).is_err() {
                    break;
                }
                n += k as u64;
            }
        }
    }
    let _ = to.shutdown(Shutdown::Write);
    let _ = from.shutdown(Shutdown::Read);
    n
}

/// Addresses that mean "this machine" or "this link". A name that resolves
/// to one of them is refused: through an allowed name, a command could
/// otherwise reach services on the runtime's own machine (or a cloud
/// machine's metadata service at 169.254.169.254).
pub fn is_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_unspecified() || v4.is_link_local() || v4.is_multicast() || v4.is_broadcast(),
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_local(IpAddr::V4(v4));
            }
            v6.is_loopback() || v6.is_unspecified() || v6.is_multicast() || (v6.segments()[0] & 0xffc0) == 0xfe80
        }
    }
}

fn read_request(client: &mut TcpStream) -> Result<Request, &'static str> {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
            break i + 4;
        }
        if buf.len() > MAX_HEAD {
            return Err("request head too long");
        }
        match client.read(&mut chunk) {
            Ok(0) | Err(_) => return Err("no complete request"),
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
    };
    let head = std::str::from_utf8(&buf[..end]).map_err(|_| "the request head is not text")?;
    let rest = buf[end..].to_vec();
    let (line, headers) = head.split_once("\r\n").unwrap_or((head, ""));
    let mut parts = line.split(' ');
    let (Some(method), Some(target), Some(version), None) = (parts.next(), parts.next(), parts.next(), parts.next()) else {
        return Err("bad request line");
    };
    if !method.bytes().all(|b| b.is_ascii_uppercase()) || method.is_empty() || !version.starts_with("HTTP/1.") {
        return Err("bad request line");
    }
    if method == "CONNECT" {
        let (host, port) = split_host_port(target, None)?;
        return Ok(Request { method: method.into(), host, port, forward: Vec::new(), rest });
    }
    // Plain HTTP through a proxy carries the full URL. Anything else (a
    // bare path, `https://` without a tunnel) is not a proxy request.
    let after = target.strip_prefix("http://").ok_or("only CONNECT and http:// URLs are accepted")?;
    let cut = after.find(['/', '?']).unwrap_or(after.len());
    let (authority, path) = after.split_at(cut);
    if authority.contains('@') {
        return Err("user names in URLs are not accepted");
    }
    let (host, port) = split_host_port(authority, Some(80))?;
    let path = if path.is_empty() { "/" } else { path };
    let forward = format!("{method} {path} {version}\r\n{headers}").into_bytes();
    Ok(Request { method: method.into(), host, port, forward, rest })
}

/// `host:port` (or `host` with a default port), lower case, trailing dot
/// removed. IPv6 literals are not accepted (no rule can name one).
fn split_host_port(s: &str, default: Option<u16>) -> Result<(String, u16), &'static str> {
    if s.starts_with('[') {
        return Err("IPv6 addresses are not accepted");
    }
    let (host, port) = match s.rsplit_once(':') {
        Some((h, p)) => (h, p.parse::<u16>().map_err(|_| "bad port")?),
        None => (s, default.ok_or("CONNECT needs host:port")?),
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    if host.is_empty() || port == 0 || !host.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.') {
        return Err("bad host");
    }
    Ok((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::net::UnixStream;

    /// A proxy fed a listening socket made here, as the sandbox would.
    fn start(rules: &[&str]) -> (Proxy, u16) {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let rules = rules.iter().enumerate().map(|(i, r)| (format!("network.allow[{i}] {r:?}"), HostRule::parse(r).unwrap())).collect();
        let proxy = Proxy::start(OwnedFd::from(ours), rules);
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        let fds = [l.as_raw_fd()];
        nix::sys::socket::sendmsg::<()>(theirs.as_raw_fd(), &[std::io::IoSlice::new(b"L")], &[nix::sys::socket::ControlMessage::ScmRights(&fds)], nix::sys::socket::MsgFlags::empty(), None).unwrap();
        (proxy, port)
    }

    /// A server on 127.0.0.1 that answers every connection with `hello`.
    fn server() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for mut c in l.incoming().flatten() {
                let mut b = [0u8; 1024];
                let _ = c.read(&mut b);
                let _ = c.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\nConnection: close\r\n\r\nhello");
            }
        });
        port
    }

    fn ask(port: u16, text: &str) -> String {
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(text.as_bytes()).unwrap();
        let mut out = String::new();
        let _ = c.read_to_string(&mut out);
        out
    }

    #[test]
    fn an_allowed_address_is_tunnelled_and_counted() {
        let target = server();
        let (proxy, port) = start(&[&format!("127.0.0.1:{target}")]);
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\nHost: x\r\n\r\n").as_bytes()).unwrap();
        let mut head = [0u8; 39];
        c.read_exact(&mut head).unwrap();
        assert_eq!(&head, b"HTTP/1.1 200 Connection established\r\n\r\n");
        c.write_all(b"GET / HTTP/1.1\r\n\r\n").unwrap();
        let mut out = String::new();
        c.read_to_string(&mut out).unwrap();
        assert!(out.ends_with("hello"), "{out}");
        drop(c);
        let report = proxy.finish();
        assert_eq!(report.connections.len(), 1);
        let r = &report.connections[0];
        assert!(r.allowed && r.error.is_none(), "{r:?}");
        assert_eq!((r.method.as_str(), r.port), ("CONNECT", target));
        assert_eq!(r.bytes_up, 18);
        assert_eq!(r.bytes_down, out.len() as u64);
    }

    #[test]
    fn plain_http_is_forwarded_with_a_path() {
        let target = server();
        let (proxy, port) = start(&[&format!("127.0.0.1:{target}")]);
        let out = ask(port, &format!("GET http://127.0.0.1:{target}/x HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"));
        assert!(out.ends_with("hello"), "{out}");
        let report = proxy.finish();
        assert_eq!(report.connections[0].method, "GET");
        assert!(report.connections[0].allowed);
    }

    #[test]
    fn hosts_not_listed_are_refused_and_recorded() {
        let target = server();
        let (proxy, port) = start(&["pypi.org"]);
        let out = ask(port, &format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\n\r\n"));
        assert!(out.starts_with("HTTP/1.1 403"), "{out}");
        assert!(out.contains("not in network.allow"), "{out}");
        let out = ask(port, "CONNECT pypi.org:22 HTTP/1.1\r\n\r\n");
        assert!(out.starts_with("HTTP/1.1 403"), "another port of an allowed host: {out}");
        let report = proxy.finish();
        assert_eq!(report.connections.len(), 2);
        assert!(report.connections.iter().all(|c| !c.allowed && c.rule == "no matching rule" && c.bytes_up == 0));
    }

    #[test]
    fn an_allowed_name_that_resolves_to_this_machine_is_refused() {
        let target = server();
        let (proxy, port) = start(&[&format!("localhost:{target}")]);
        let out = ask(port, &format!("CONNECT localhost:{target} HTTP/1.1\r\n\r\n"));
        assert!(out.starts_with("HTTP/1.1 403"), "{out}");
        assert!(out.contains("local addresses"), "{out}");
        let report = proxy.finish();
        assert!(!report.connections[0].allowed);
    }

    #[test]
    fn malformed_requests_get_400_and_no_record() {
        let (proxy, port) = start(&["pypi.org"]);
        for bad in ["GET / HTTP/1.1\r\n\r\n", "GET https://pypi.org/ HTTP/1.1\r\n\r\n", "CONNECT pypi.org HTTP/1.1\r\n\r\n", "CONNECT [::1]:443 HTTP/1.1\r\n\r\n", "GET http://u@pypi.org/ HTTP/1.1\r\n\r\n", "hello\r\n\r\n"] {
            let out = ask(port, bad);
            assert!(out.starts_with("HTTP/1.1 400"), "{bad:?}: {out}");
        }
        assert!(proxy.finish().connections.is_empty());
    }

    #[test]
    fn finish_ends_connections_that_are_still_open() {
        // A server that accepts and then says nothing, ever.
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let target = l.local_addr().unwrap().port();
        let hold = std::thread::spawn(move || l.accept().map(|(c, _)| c));
        let (proxy, port) = start(&[&format!("127.0.0.1:{target}")]);
        let mut c = TcpStream::connect(("127.0.0.1", port)).unwrap();
        c.write_all(format!("CONNECT 127.0.0.1:{target} HTTP/1.1\r\n\r\n").as_bytes()).unwrap();
        let mut head = [0u8; 39];
        c.read_exact(&mut head).unwrap();
        // A second client that never finishes its request head.
        let _idle = TcpStream::connect(("127.0.0.1", port)).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        let t = std::time::Instant::now();
        let report = proxy.finish();
        assert!(t.elapsed() < Duration::from_secs(5), "finish waited {:?}", t.elapsed());
        assert_eq!(report.connections.len(), 1);
        drop(hold);
    }

    #[test]
    fn local_addresses() {
        for ip in ["127.0.0.1", "127.1.2.3", "0.0.0.0", "169.254.169.254", "224.0.0.1", "255.255.255.255", "::1", "::", "fe80::1", "::ffff:127.0.0.1"] {
            assert!(is_local(ip.parse().unwrap()), "{ip}");
        }
        for ip in ["93.184.216.34", "10.0.0.1", "192.168.1.1", "2606:4700::1111"] {
            assert!(!is_local(ip.parse().unwrap()), "{ip}");
        }
    }

    #[test]
    fn no_socket_means_nothing_to_do() {
        let (ours, theirs) = UnixStream::pair().unwrap();
        let proxy = Proxy::start(OwnedFd::from(ours), Vec::new());
        drop(theirs);
        assert_eq!(proxy.finish(), Report::default());
    }
}
