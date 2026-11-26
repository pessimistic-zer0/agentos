#!/usr/bin/env python3
"""E1: a tiny HTTP proxy that logs every request and allows only listed hosts.

Usage: python3 proxy.py PORT LOGFILE [allowed-host ...]
With no hosts every host is allowed (to discover what a tool needs).
CONNECT host:port is tunnelled; a plain `GET http://host/...` is forwarded.
"""
import socket, sys, threading, time

PORT, LOG, ALLOW = int(sys.argv[1]), sys.argv[2], set(sys.argv[3:])
lock = threading.Lock()

def log(line):
    with lock, open(LOG, "a") as f:
        f.write(f"{time.time():.3f} {line}\n")

def allowed(host):
    return not ALLOW or host in ALLOW or any(a.startswith("*.") and host.endswith(a[1:]) for a in ALLOW)

def pump(a, b, counter, key):
    try:
        while True:
            data = a.recv(65536)
            if not data:
                break
            counter[key] += len(data)
            b.sendall(data)
    except OSError:
        pass
    finally:
        try: b.shutdown(socket.SHUT_WR)
        except OSError: pass

def handle(c):
    head = b""
    while b"\r\n\r\n" not in head:
        d = c.recv(4096)
        if not d: return c.close()
        head += d
    line = head.split(b"\r\n", 1)[0].decode("latin1")
    method, target, _ = line.split(" ", 2)
    if method == "CONNECT":
        host, port = target.rsplit(":", 1)
        rest = head.split(b"\r\n\r\n", 1)[1]
    else:
        hp = target.split("/", 3)[2]
        host, port = (hp.rsplit(":", 1) + ["80"])[:2] if ":" in hp else (hp, "80")
        rest = head
    if not allowed(host):
        log(f"DENY {method} {host}:{port}")
        c.sendall(b"HTTP/1.1 403 Forbidden\r\nContent-Length: 0\r\nConnection: close\r\n\r\n")
        return c.close()
    try:
        s = socket.create_connection((host, int(port)), timeout=30)
        s.settimeout(None)
    except OSError as e:
        log(f"FAIL {method} {host}:{port} {e}")
        c.sendall(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\n\r\n")
        return c.close()
    log(f"OPEN {method} {host}:{port}")
    if method == "CONNECT":
        c.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
    if rest:
        s.sendall(rest)
    n = {"up": len(rest), "down": 0}
    t = threading.Thread(target=pump, args=(c, s, n, "up")); t.start()
    pump(s, c, n, "down"); t.join()
    log(f"ALLOW {method} {host}:{port} up={n['up']} down={n['down']}")
    s.close(); c.close()

srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
srv.bind(("127.0.0.1", PORT)); srv.listen(64)
while True:
    conn, _ = srv.accept()
    threading.Thread(target=handle, args=(conn,), daemon=True).start()
