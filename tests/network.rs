//! The network allow-list for commands (M19; D-153 to D-156).
//!
//! Without `[network]` a command has no network at all, as since M2. With
//! hosts listed, the sandbox's only way out is the runtime's proxy on
//! `127.0.0.1:3128`, which lets through the listed hosts and refuses the
//! rest, and every connection is in the log.
//!
//! These tests use servers on the host's `127.0.0.1`, named by number in
//! the policy (a name that resolves to this machine is refused, see
//! `sandbox::proxy`). No test reaches the internet.
//!
//! Needs a delegated cgroup: systemd-run --user --scope -p Delegate=yes cargo test --test network

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use common::*;
use serde_json::Value;

/// The preset policy, allowing `python3` and listing `hosts` under `[network]`.
fn policy(hosts: &[String]) -> String {
    let mut out = String::new();
    for line in std::fs::read_to_string(preset_policy()).unwrap().lines() {
        if line == "allow = [" {
            out.push_str("allow = [\n  [\"python3\", \"client.py\"],\n");
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    assert!(out.contains("client.py"), "the preset has no `allow = [` line");
    if !hosts.is_empty() {
        let list: Vec<String> = hosts.iter().map(|h| format!("{h:?}")).collect();
        out.push_str(&format!("\n[network]\nallow = [{}]\n", list.join(", ")));
    }
    out
}

/// A web server on the host that answers every request with `body`.
fn web_server(body: &'static str) -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let _ = c.read(&mut buf);
            let _ = write!(c, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    port
}

/// A small repo whose `client.py` prints one line per check.
fn repo(t: &TestEnv, client: &str) -> PathBuf {
    let repo = t.root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join("client.py"), client).unwrap();
    repo
}

const CLIENT: &str = r#"
import os, socket, sys, urllib.request
print("proxy-env", os.environ.get("HTTPS_PROXY"), os.environ.get("http_proxy"), os.environ.get("NO_PROXY"))
def try_(name, f):
    try:
        print(name, "ok", f())
    except Exception as e:
        print(name, "error", type(e).__name__, str(e)[:120])
def direct(host, port):
    s = socket.create_connection((host, port), timeout=3); s.close(); return "connected"
# NO_PROXY names 127.0.0.1 (so that a project's own test servers are not
# sent to the proxy), and urllib honours it even for an explicit proxy. The
# test servers here are on the host, reached only through the proxy.
os.environ.pop("NO_PROXY", None); os.environ.pop("no_proxy", None)
for a in sys.argv[1:]:
    host, port = a.rsplit(":", 1)
    try_("direct " + a, lambda: direct(host, int(port)))
    if os.environ.get("HTTP_PROXY"):
        opener = urllib.request.build_opener(urllib.request.ProxyHandler({"http": os.environ["HTTP_PROXY"]}))
        try_("proxied " + a, lambda: opener.open("http://%s/" % a, timeout=5).read().decode())
"#;

fn run(t: &TestEnv, repo: &Path, args: &[String]) -> (String, Vec<Value>, Value) {
    let argv: Vec<String> = ["python3", "client.py"].iter().map(|s| s.to_string()).chain(args.iter().cloned()).collect();
    let script = t.write_script("s.toml", &format!("task = \"net\"\n[[step]]\ntool = \"exec\"\nargv = {argv:?}\n"));
    let id = t.run_script_on(script.to_str().unwrap(), repo);
    let events = t.events(&id);
    let out = t.output_of(&id, &events, 1).unwrap_or_else(|| panic!("the exec did not complete: {events:#?}"));
    (id, events, out)
}

fn line<'a>(stdout: &'a str, prefix: &str) -> &'a str {
    stdout.lines().find(|l| l.starts_with(prefix)).unwrap_or_else(|| panic!("no line `{prefix}` in:\n{stdout}"))
}

#[test]
fn without_a_network_section_a_command_reaches_nothing() {
    let t = TestEnv::new("net-off");
    let t = t.with_policy_text(&policy(&[]));
    let port = web_server("hello");
    let repo = repo(&t, CLIENT);
    let (_, events, out) = run(&t, &repo, &[format!("127.0.0.1:{port}"), "1.1.1.1:80".into()]);
    let stdout = out["stdout"].as_str().unwrap();
    assert_eq!(line(stdout, "proxy-env"), "proxy-env None None None", "no proxy variables without [network]");
    // 127.0.0.1 inside the sandbox is the sandbox's own loopback: the host's
    // server is not there.
    assert!(line(stdout, &format!("direct 127.0.0.1:{port}")).contains("error"), "{stdout}");
    assert!(line(stdout, "direct 1.1.1.1:80").contains("error"), "{stdout}");
    assert!(out.get("network").is_none(), "no network record without [network]: {out}");
    assert!(!events.iter().any(|e| e["type"] == "exec.network"));
}

#[test]
fn a_listed_host_is_reached_through_the_proxy_and_logged() {
    let t = TestEnv::new("net-allow");
    let allowed = web_server("hello from the allowed host");
    let other = web_server("you should not see this");
    let t = t.with_policy_text(&policy(&[format!("127.0.0.1:{allowed}")]));
    let repo = repo(&t, CLIENT);
    let (id, events, out) = run(&t, &repo, &[format!("127.0.0.1:{allowed}"), format!("127.0.0.1:{other}")]);
    let stdout = out["stdout"].as_str().unwrap();
    let stderr = out["stderr"].as_str().unwrap();
    assert_eq!(line(stdout, "proxy-env"), "proxy-env http://127.0.0.1:3128 http://127.0.0.1:3128 localhost,127.0.0.1,::1");
    assert_eq!(line(stdout, &format!("proxied 127.0.0.1:{allowed}")), format!("proxied 127.0.0.1:{allowed} ok hello from the allowed host"));
    let refused = line(stdout, &format!("proxied 127.0.0.1:{other}"));
    assert!(refused.contains("error") && refused.contains("403"), "{refused}");
    assert!(!stdout.contains("you should not see this"));
    // Direct connections still go nowhere: the proxy is the only way out.
    assert!(line(stdout, &format!("direct 127.0.0.1:{allowed}")).contains("error"), "{stdout}");
    // The agent is told why, and what is allowed.
    assert!(stderr.contains(&format!("[norn] network: 127.0.0.1:{other} was not reached: no matching rule")), "{stderr}");
    assert!(stderr.contains(&format!("[norn] network.allow lists: 127.0.0.1:{allowed}")), "{stderr}");

    // The log has both connections, with the rule and the bytes.
    let net: Vec<&Value> = events.iter().filter(|e| e["type"] == "exec.network").collect();
    assert_eq!(net.len(), 1, "{events:#?}");
    assert_eq!(net[0]["call_id"], 1);
    let conns = net[0]["connections"].as_array().unwrap();
    assert_eq!(conns.len(), 2, "{conns:#?}");
    let ok = conns.iter().find(|c| c["port"] == allowed).unwrap();
    assert_eq!(ok["allowed"], true);
    assert_eq!(ok["method"], "GET");
    assert_eq!(ok["rule"], format!("network.allow[0] \"127.0.0.1:{allowed}\""));
    assert!(ok["bytes_down"].as_u64().unwrap() > 27, "{ok}");
    let no = conns.iter().find(|c| c["port"] == other).unwrap();
    assert_eq!((no["allowed"].clone(), no["bytes_up"].clone()), (Value::Bool(false), Value::from(0)));
    // The network event comes before the call's ending.
    let seq = |ty: &str| events.iter().find(|e| e["type"] == ty && e["call_id"] == 1).unwrap()["seq"].as_u64().unwrap();
    assert!(seq("tool.intent") < seq("exec.network") && seq("exec.network") < seq("tool.completed"));

    let trace = t.norn(&["trace", &id]);
    assert!(trace.contains(&format!("network: GET 127.0.0.1:{allowed} ")), "{trace}");
    assert!(trace.contains(&format!("network: GET 127.0.0.1:{other} REFUSED (no matching rule)")), "{trace}");
}

#[test]
fn a_tunnel_to_a_listed_host_carries_bytes_both_ways() {
    // An echo server: what goes in through the CONNECT tunnel comes back.
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = l.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for mut c in l.incoming().flatten() {
            let mut buf = [0u8; 4096];
            while let Ok(n) = c.read(&mut buf) {
                if n == 0 || c.write_all(&buf[..n]).is_err() {
                    break;
                }
            }
        }
    });
    let t = TestEnv::new("net-tunnel");
    let t = t.with_policy_text(&policy(&[format!("127.0.0.1:{port}")]));
    let client = format!(
        r#"
import socket
s = socket.create_connection(("127.0.0.1", 3128), timeout=5)
s.sendall(b"CONNECT 127.0.0.1:{port} HTTP/1.1\r\nHost: x\r\n\r\n")
head = b""
while not head.endswith(b"\r\n\r\n"):
    head += s.recv(1)
print("head", head.split(b"\r\n")[0].decode())
s.sendall(b"x" * 100000)
got = 0
while got < 100000:
    got += len(s.recv(65536))
print("echoed", got)
"#
    );
    let repo = repo(&t, &client);
    let (_, events, out) = run(&t, &repo, &[]);
    let stdout = out["stdout"].as_str().unwrap();
    assert!(stdout.contains("head HTTP/1.1 200 Connection established"), "{stdout}{}", out["stderr"]);
    assert!(stdout.contains("echoed 100000"), "{stdout}");
    let net = events.iter().find(|e| e["type"] == "exec.network").unwrap();
    let c = &net["connections"][0];
    assert_eq!(c["method"], "CONNECT");
    assert_eq!((c["bytes_up"].as_u64(), c["bytes_down"].as_u64()), (Some(100000), Some(100000)), "{c}");
}

#[test]
fn a_bad_network_entry_stops_the_run_before_it_starts() {
    let t = TestEnv::new("net-bad");
    let t = t.with_policy_text(&policy(&["https://pypi.org".into()]));
    let repo = repo(&t, CLIENT);
    let script = t.write_script("s.toml", "task = \"net\"\n[[step]]\ntool = \"exec\"\nargv = [\"python3\", \"client.py\"]\n");
    let out = t.output(&["run", "--quiet", "--repo", repo.to_str().unwrap(), "--policy", t.policy.to_str().unwrap(), "--script", script.to_str().unwrap()]);
    assert!(!out.status.success());
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("network.allow entry `https://pypi.org`"), "{err}");
}
