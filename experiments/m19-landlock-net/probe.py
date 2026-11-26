#!/usr/bin/env python3
"""E3: which Landlock ABI does this kernel have, and do its network rules
(ABI 4+) stop a TCP connect to a port not on the list, inside an empty
network namespace too? Linux x86_64 only (syscall numbers below)."""
import ctypes, os, socket, struct, sys

libc = ctypes.CDLL(None, use_errno=True)
SYS_create, SYS_add, SYS_restrict = 444, 445, 446
abi = libc.syscall(SYS_create, None, ctypes.c_size_t(0), ctypes.c_uint32(1))
print(f"kernel {os.uname().release}: Landlock ABI {abi if abi > 0 else 'none (errno %d)' % ctypes.get_errno()}")
if abi < 4:
    print("no network rules (need ABI 4, Linux 6.7)"); sys.exit(0)

BIND, CONNECT = 1, 2
# struct landlock_ruleset_attr { u64 handled_access_fs; u64 handled_access_net; }
attr = struct.pack("QQ", 0, BIND | CONNECT)
buf = ctypes.create_string_buffer(attr)
rs = libc.syscall(SYS_create, buf, ctypes.c_size_t(len(attr)), ctypes.c_uint32(0))
assert rs >= 0, ctypes.get_errno()
def allow(port, access):
    # struct landlock_net_port_attr { u64 allowed_access; u64 port; } (packed)
    a = ctypes.create_string_buffer(struct.pack("QQ", access, port))
    assert libc.syscall(SYS_add, rs, 2, a, 0) == 0, ctypes.get_errno()  # 2 = LANDLOCK_RULE_NET_PORT
allow(3128, CONNECT)
allow(8000, BIND)
libc.prctl(38, 1, 0, 0, 0)  # PR_SET_NO_NEW_PRIVS
assert libc.syscall(SYS_restrict, rs, 0) == 0, ctypes.get_errno()

def try_(what, f):
    try:
        f(); print(f"  {what}: allowed")
    except OSError as e:
        print(f"  {what}: {e.strerror}")
srv = socket.socket(); srv.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
try_("bind 127.0.0.1:8000 (listed)", lambda: (srv.bind(("127.0.0.1", 8000)), srv.listen()))
try_("bind 127.0.0.1:8001 (not listed)", lambda: socket.socket().bind(("127.0.0.1", 8001)))
try_("connect 127.0.0.1:8000 (bind-only port)", lambda: socket.create_connection(("127.0.0.1", 8000), timeout=1))
try_("connect 127.0.0.1:3128 (listed; nobody listening)", lambda: socket.create_connection(("127.0.0.1", 3128), timeout=1))
try_("UDP sendto 127.0.0.1:53", lambda: socket.socket(socket.AF_INET, socket.SOCK_DGRAM).sendto(b"x", ("127.0.0.1", 53)))
