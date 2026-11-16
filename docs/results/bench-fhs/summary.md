# Overhead benchmarks

Machine and software:

```text
date:       2026-11-16T16:06:08Z
cpu:        AMD EPYC 9V74 80-Core Processor (2 threads)
ram:        7.7 GiB
kernel:     Linux 7.0.0-1017-azure
os:         Ubuntu 24.04.5 LTS
filesystem: ext2/ext3 (state and workspace under target/)
rustc:      rustc 1.100.0 (0ef1bc386 2026-11-09)
build:      release
preset:     fhs
python:     Python 3.12.3, pytest 7.4.4
hyperfine:  hyperfine 1.18.0
commit:     f38fff9
```

## 1. Sandbox spawn cost (`exec true`)

In-process: time from creating the cgroup to the sandboxed `true` having exited and its cgroup removed, against `fork+exec` of the same `true` binary.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| sandboxed `true` | 500 | 5.42 | 7.20 | 7.64 | 5.32 |
| direct `true` | 500 | 0.45 | 0.60 | 0.76 | 0.47 |

Added cost per spawn: p50 4.97 ms, p99 6.88 ms.

Whole process, measured with hyperfine (includes the benchmark program's own start, the startup checks and two `systemctl` calls):

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| `<repo>/target/release/examples/sandbox_spawn once` | 300 | 15.01 | 16.74 | 20.41 | 15.19 |
| `true` | 300 | 0.49 | 0.54 | 0.56 | 0.50 |

## 2. `fs.write` of 4 KB

Through the gateway (requested, intent, completed records, each fsynced; blob; temp file + fsync + rename; checkpoint) against writing the file directly. Timed inside a criterion benchmark; these are the raw per-call times.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| AgentOS `fs.write` | 4336 | 3.77 | 12.88 | 135.66 | 8.55 |
| direct: temp file + fsync + rename | 11223 | 0.39 | 0.65 | 3.15 | 0.95 |
| direct: plain write, no fsync | 55366 | 0.22 | 0.30 | 1.14 | 0.41 |

p50: 9.6× a durable direct write, 17× a plain one.

## 3. End-to-end demo task

The scripted demo (7 calls: list, pytest, 2 reads, 1 denied read, write, pytest) through `agentos run`, against a shell script doing the same steps directly on a copy of the repo (`bench/direct-demo.sh`). Measured with hyperfine.

| what | samples | p50 ms | p95 ms | p99 ms | mean ms |
|---|---|---|---|---|---|
| AgentOS: `agentos run … fix-demo.toml` | 100 | 400.74 | 728.03 | 929.43 | 452.61 |
| direct: `bench/direct-demo.sh` | 100 | 340.66 | 461.99 | 540.36 | 358.04 |

End-to-end slowdown at p50: 18% (401 ms against 341 ms).
