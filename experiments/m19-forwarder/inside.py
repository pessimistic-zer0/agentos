#!/usr/bin/env python3
"""E2, the part inside the sandbox (an empty network namespace, lo up).

  inside.py unix   SOCK   forwarder: listen on 127.0.0.1:3128, and for each
                          connection open the Unix socket SOCK and copy bytes
  inside.py fdpass CTRL   make the listening socket on 127.0.0.1:3128, send it
                          out through the Unix socket CTRL, close our copy
Then run the command given after `--`.
"""
import os, socket, subprocess, sys, threading

def pump(a, b):
    try:
        while (d := a.recv(65536)):
            b.sendall(d)
    except OSError:
        pass
    finally:
        try: b.shutdown(socket.SHUT_WR)
        except OSError: pass

def forward(c, path):
    u = socket.socket(socket.AF_UNIX); u.connect(path)
    t = threading.Thread(target=pump, args=(c, u)); t.start(); pump(u, c); t.join()
    u.close(); c.close()

mode, path = sys.argv[1], sys.argv[2]
cmd = sys.argv[sys.argv.index("--") + 1:]
lst = socket.socket(); lst.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
lst.bind(("127.0.0.1", 3128)); lst.listen(128)
if mode == "unix":
    def loop():
        while True:
            c, _ = lst.accept()
            threading.Thread(target=forward, args=(c, path), daemon=True).start()
    threading.Thread(target=loop, daemon=True).start()
else:
    ctrl = socket.socket(socket.AF_UNIX); ctrl.connect(path)
    socket.send_fds(ctrl, [b"L"], [lst.fileno()]); ctrl.close(); lst.close()
sys.exit(subprocess.call(cmd))
