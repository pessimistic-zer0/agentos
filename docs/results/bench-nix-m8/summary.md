# Overhead benchmarks

Machine and software:

```text
date:       2026-11-14T15:43:37Z
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
commit:     15fab27
```

## 1. Sandbox spawn cost (`exec true`)

In-process: time from creating the cgroup to the sandboxed `true` having exited and its cgroup removed, against `fork+exec` of the same `true` binary.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| sandboxed `true` | 500 | 5.62 | 6.98 | 8.35 | 5.68 |
| direct `true` | 500 | 1.06 | 1.30 | 1.46 | 1.06 |

Added cost per spawn: p50 4.56 ms, p99 6.89 ms.

Whole process, measured with hyperfine (includes the benchmark program's own start, the startup checks and two `systemctl` calls):

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| `<repo>/target/release/examples/sandbox_spawn once` | 300 | 32.74 | 39.86 | 41.42 | 32.99 |
| `true` | 300 | 0.73 | 1.03 | 1.16 | 0.74 |

## 2. `fs.write` of 4 KB

Through the gateway (requested, intent, completed records, each fsynced; blob; temp file + fsync + rename; checkpoint) against writing the file directly. Timed inside a criterion benchmark; these are the raw per-call times.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| AgentOS `fs.write` | 2805 | 13.34 | 16.78 | 21.34 | 13.62 |
| direct: temp file + fsync + rename | 13522 | 1.92 | 2.21 | 2.69 | 1.94 |
| direct: plain write, no fsync | 295417 | 0.08 | 0.11 | 0.13 | 0.08 |

p50: 7.0× a durable direct write, 165× a plain one.

## 3. End-to-end demo task

The scripted demo (7 calls: list, pytest, 2 reads, 1 denied read, write, pytest) through `agentos run`, against a shell script doing the same steps directly on a copy of the repo (`bench/direct-demo.sh`). Measured with hyperfine.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| AgentOS: `agentos run … fix-demo.toml` | 100 | 480.78 | 495.20 | 507.45 | 481.38 |
| direct: `bench/direct-demo.sh` | 100 | 353.97 | 368.56 | 376.51 | 354.32 |

End-to-end slowdown at p50: 36% (481 ms against 354 ms).
