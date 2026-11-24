#!/usr/bin/env python3
"""Turn the raw benchmark data in a results folder into summary.md.

Reads: spawn-*.txt and fs_write-*.txt (one time in microseconds per line),
hyperfine-*.json (hyperfine --export-json), system.txt.
Every number in the summary comes from these files.
"""
import json, os, sys

d = sys.argv[1]

def pct(xs, p):
    xs = sorted(xs)
    return xs[min(len(xs) - 1, round((len(xs) - 1) * p))]

def raw(name):
    with open(os.path.join(d, name)) as f:
        return [float(l) / 1000.0 for l in f if l.strip()]          # µs -> ms

def hyper(name):
    with open(os.path.join(d, name)) as f:
        return {r["command"]: [t * 1000.0 for t in r["times"]] for r in json.load(f)["results"]}

def row(label, xs):
    return f"| {label} | {len(xs)} | {pct(xs, .5):.2f} | {pct(xs, .95):.2f} | {pct(xs, .99):.2f} | {sum(xs)/len(xs):.2f} |"

head = "| what | samples | p50 ms | p95 ms | p99 ms | mean ms |\n|---|---|---|---|---|---|"
out = ["# Overhead benchmarks", "", "Machine and software:", "", "```text", open(os.path.join(d, "system.txt")).read().strip(), "```", ""]

sb, di = raw("spawn-sandbox.txt"), raw("spawn-direct.txt")
out += ["## 1. Sandbox spawn cost (`exec true`)", "",
        "In-process: time from creating the cgroup to the sandboxed `true` having exited and its cgroup removed, against `fork+exec` of the same `true` binary.", "",
        head, row("sandboxed `true`", sb), row("direct `true`", di), "",
        f"Added cost per spawn: p50 {pct(sb,.5)-pct(di,.5):.2f} ms, p99 {pct(sb,.99)-pct(di,.99):.2f} ms.", ""]
h = hyper("hyperfine-spawn.json")
out += ["Whole process, measured with hyperfine (includes the benchmark program's own start, the startup checks and two `systemctl` calls):", "", head]
out += [row(f"`{k}`", v) for k, v in h.items()] + [""]

a, dd, dp = raw("fs_write-norn.txt"), raw("fs_write-direct-durable.txt"), raw("fs_write-direct-plain.txt")
out += ["## 2. `fs.write` of 4 KB", "",
        "Through the gateway (requested, intent, completed records, each fsynced; blob; temp file + fsync + rename; checkpoint) against writing the file directly. Timed inside a criterion benchmark; these are the raw per-call times.", "",
        head, row("Norn `fs.write`", a), row("direct: temp file + fsync + rename", dd), row("direct: plain write, no fsync", dp), "",
        f"p50: {pct(a,.5)/pct(dd,.5):.1f}× a durable direct write, {pct(a,.5)/pct(dp,.5):.0f}× a plain one.", ""]

h = hyper("hyperfine-e2e.json")
ag = next(v for k, v in h.items() if "direct-demo" not in k)
dr = next(v for k, v in h.items() if "direct-demo" in k)
out += ["## 3. End-to-end demo task", "",
        "The scripted demo (7 calls: list, pytest, 2 reads, 1 denied read, write, pytest) through `norn run`, against a shell script doing the same steps directly on a copy of the repo (`bench/direct-demo.sh`). Measured with hyperfine.", "",
        head, row("Norn: `norn run … fix-demo.toml`", ag), row("direct: `bench/direct-demo.sh`", dr), ""]
slow = (pct(ag, .5) / pct(dr, .5) - 1) * 100
out += [f"End-to-end slowdown at p50: {slow:.0f}% ({pct(ag,.5):.0f} ms against {pct(dr,.5):.0f} ms).", ""]
open(os.path.join(d, "summary.md"), "w").write("\n".join(out))
print("\n".join(out))
