#!/usr/bin/env python3
"""Kill a writer with SIGKILL at random moments. Is the target file ever half-written?

Two writers replace a 2 MB file over and over, alternating between content
made of 'A's and content made of 'B's:
  direct: open(target, 'w') and write in chunks
  atomic: write a temp file, fsync, rename over the target
After each kill, the target must be all-A or all-B and exactly 2 MB.
"""
import os, random, signal, subprocess, sys, tempfile, time

SIZE = 2 * 1024 * 1024
WRITER = r'''
import os, sys
mode, target = sys.argv[1], sys.argv[2]
i = 0
while True:
    data = (b"A" if i % 2 == 0 else b"B") * (2 * 1024 * 1024)
    if mode == "direct":
        with open(target, "wb") as f:
            for off in range(0, len(data), 65536):
                f.write(data[off:off + 65536])
    else:
        tmp = target + ".tmp"
        with open(tmp, "wb") as f:
            for off in range(0, len(data), 65536):
                f.write(data[off:off + 65536])
            f.flush(); os.fsync(f.fileno())
        os.rename(tmp, target)
    i += 1
'''

def trial(mode, d, runs):
    target = os.path.join(d, "target")
    torn = leftovers = 0
    for _ in range(runs):
        open(target, "wb").write(b"A" * SIZE)
        p = subprocess.Popen([sys.executable, "-c", WRITER, mode, target])
        time.sleep(random.uniform(0.02, 0.08))
        p.send_signal(signal.SIGKILL); p.wait()
        data = open(target, "rb").read()
        if not (data == b"A" * SIZE or data == b"B" * SIZE):
            torn += 1
        if os.path.exists(target + ".tmp"):
            leftovers += 1
            os.remove(target + ".tmp")
    return torn, leftovers

runs = int(sys.argv[1]) if len(sys.argv) > 1 else 200
with tempfile.TemporaryDirectory(dir=os.environ.get("EXP_DIR")) as d:
    for mode in ("direct", "atomic"):
        torn, left = trial(mode, d, runs)
        print(f"{mode:<6}: {runs} kills, {torn} torn target files, {left} leftover temp files")
