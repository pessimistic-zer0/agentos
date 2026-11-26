#!/usr/bin/env python3
"""E2, the part outside the sandbox: a CONNECT proxy (allows every host) that
takes its client connections in one of two ways.

  outside.py unix   SOCK   listen on a Unix socket; a forwarder inside the
                           sandbox connects to it once per client connection
  outside.py fdpass CTRL   listen on a Unix socket CTRL; the sandbox sends us
                           ONE file descriptor: its own listening TCP socket on
                           127.0.0.1:3128. We accept() on it directly.
"""
import array, os, socket, sys, threading

def pump(a, b):
    try:
        while (d := a.recv(65536)):
            b.sendall(d)
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
    method, target, _ = head.split(b"\r\n", 1)[0].decode().split(" ", 2)
    if method == "CONNECT":
        host, port = target.rsplit(":", 1); rest = head.split(b"\r\n\r\n", 1)[1]
    else:
        hp = target.split("/", 3)[2]; host, port = hp.rsplit(":", 1) if ":" in hp else (hp, "80"); rest = head
    s = socket.create_connection((host, int(port)))
    if method == "CONNECT":
        c.sendall(b"HTTP/1.1 200 Connection established\r\n\r\n")
    if rest: s.sendall(rest)
    t = threading.Thread(target=pump, args=(c, s)); t.start(); pump(s, c); t.join()
    s.close(); c.close()

def serve(listener):
    while True:
        c, _ = listener.accept()
        threading.Thread(target=handle, args=(c,), daemon=True).start()

mode, path = sys.argv[1], sys.argv[2]
if os.path.exists(path): os.unlink(path)
u = socket.socket(socket.AF_UNIX); u.bind(path); u.listen(128)
if mode == "unix":
    serve(u)
else:
    ctrl, _ = u.accept()
    msg, fds, _, _ = socket.recv_fds(ctrl, 16, 1)
    lst = socket.socket(fileno=fds[0])
    print(f"outside: got fd; family={lst.family.name} bound to {lst.getsockname()}", flush=True)
    serve(lst)
