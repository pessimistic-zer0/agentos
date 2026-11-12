# Overhead benchmarks

Machine and software:

```text
date:       2026-11-12T11:54:38Z
cpu:        13th Gen Intel(R) Core(TM) i5-13420H (12 threads)
ram:        15.3 GiB
kernel:     Linux 7.1.8
os:         NixOS 26.05 (Yarara)
filesystem: btrfs (state and workspace under target/)
rustc:      rustc 1.95.0 (59807616e 2026-04-14) (built from a source tarball)
build:      release
preset:     nix
python:     Python 3.13.15, pytest 9.0.3
hyperfine:  hyperfine 1.20.0
commit:     357c313
```

## 1. Sandbox spawn cost (`exec true`)

In-process: time from creating the cgroup to the sandboxed `true` having exited and its cgroup removed, against `fork+exec` of the same `true` binary.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| sandboxed `true` | 500 | 7.78 | 19.55 | 24.59 | 9.29 |
| direct `true` | 500 | 1.33 | 2.17 | 2.84 | 1.41 |

Added cost per spawn: p50 6.45 ms, p99 21.74 ms.

Whole process, measured with hyperfine (includes the benchmark program's own start, the startup checks and two `systemctl` calls):

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| `<repo>/target/release/examples/sandbox_spawn once` | 300 | 39.16 | 47.59 | 51.39 | 39.00 |
| `true` | 300 | 1.49 | 2.56 | 3.28 | 1.65 |

## 2. `fs.write` of 4 KB

Through the gateway (requested, intent, completed records, each fsynced; blob; temp file + fsync + rename; checkpoint) against writing the file directly. Timed inside a criterion benchmark; these are the raw per-call times.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| AgentOS `fs.write` | 1402 | 13.70 | 20.38 | 23.81 | 14.17 |
| direct: temp file + fsync + rename | 12247 | 1.94 | 2.46 | 2.80 | 1.99 |
| direct: plain write, no fsync | 201067 | 0.12 | 0.20 | 0.25 | 0.12 |

p50: 7.1× a durable direct write, 117× a plain one.

## 3. End-to-end demo task

The scripted demo (7 calls: list, pytest, 2 reads, 1 denied read, write, pytest) through `agentos run`, against a shell script doing the same steps directly on a copy of the repo (`bench/direct-demo.sh`). Measured with hyperfine.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| AgentOS: `agentos run … fix-demo.toml` | 100 | 506.84 | 619.50 | 662.58 | 525.11 |
| direct: `bench/direct-demo.sh` | 100 | 375.87 | 432.13 | 520.59 | 383.78 |

End-to-end slowdown at p50: 35% (507 ms against 376 ms).
