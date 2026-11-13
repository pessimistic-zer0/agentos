# M5 — Benchmarks and the full security suite

| | |
|---|---|
| Weeks in the plan | 10 |
| Started / finished | 2026-11-12 / 2026-11-12 |
| Commits | `357c313` … the commit that adds this file (tag `m5`) |
| Status | Done with gaps (the `fhs` preset is not yet verified) |

Written for a reader new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Finish the evidence. M2 and M3 built the sandbox and recovery and tested them; M5 widens the attack tests past thirty cases (adding the race that the design singles out), adds the opposite test (ordinary work must never be refused), and measures what all of this costs: how much slower a command, a file write and a whole task are with AgentOS than without. No new part of the system is added. One command, `make eval`, now runs all three suites of the v1 design.

## 2. Requirements

Exit criterion from the v1 table: *"Three numbers with p50/p95/p99 and hardware details."* The same row asks to "expand security suite to 30+ cases, including the TOCTOU race".

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| Sandbox spawn cost, `exec true`, hyperfine, vs direct | Yes | `make bench` → `docs/results/bench-nix/summary.md` §1. In-process (500 samples): sandboxed p50 **7.78** / p95 19.55 / p99 24.59 ms; direct 1.33 / 2.17 / 2.84 ms. hyperfine on the whole benchmark program (300 runs): 39.16 / 47.59 / 51.39 ms vs 1.49 / 2.56 / 3.28 ms |
| `fs.write` 4 KB incl. intent fsync and checkpoint, criterion, vs direct | Yes | Same file §2 (1,402 samples): p50 **13.70** / p95 20.38 / p99 23.81 ms; durable direct write 1.94 / 2.46 / 2.80 ms; plain write 0.12 / 0.20 / 0.25 ms. criterion's own estimate: `docs/results/bench-nix/criterion-fs_write.txt` |
| End-to-end demo wall time vs direct | Yes | Same file §3 (hyperfine, 100 runs): AgentOS p50 **507** / p95 620 / p99 663 ms; direct 376 / 432 / 521 ms; **+35%** at p50 |
| CPU, RAM, kernel, filesystem reported | Yes | `docs/results/bench-nix/system.txt`: i5-13420H (12 threads), 15.3 GiB, Linux 7.1.8, btrfs, rustc 1.95.0, release build, commit `357c313` |
| Raw results and scripts published | Yes | Every sample in `docs/results/bench-nix/` (`*.txt` in µs, hyperfine `*.json`); `bench/run.sh`, `bench/summarize.py`, `bench/direct-demo.sh`, `benches/fs_write.rs`, `examples/sandbox_spawn.rs` |
| Security suite ≥ 30 cases, 0 escapes | Yes | `make eval-security` → `32 passed; 0 failed` (`docs/results/security-nix.md`) |
| …including the TOCTOU race | Yes | `toctou: 7229 swaps, 157 calls completed inside the workspace, 143 refused as leaving it`; nothing written outside, outside content read nowhere |
| …a `conftest.py` reading a denied file | Yes | `conftest_cannot_read_a_denied_file` (M2) |
| …a write outside the write globs | Yes | `exec_changes_outside_write_globs_are_reverted` (M2) |
| Benign suite: 0 false denials | Yes | `benign: 23 operations, 0 false denials, 0 failures, 0 reverts` |
| One command for all suites | Yes | `make eval` = `eval-recovery` + `eval-security` + `bench` |
| Same on the `fhs` preset | **Not yet verified** | `.github/workflows/ci.yml` is written and has never run (D-065) |
| Earlier suites still pass | Yes | `make test`: 22 unit, 2 LLM-mock, 1 benign, recovery (72 kills), 32 security, 4 vertical-slice; clippy clean |

The benchmark was run twice (before and after the commit); the two runs agree within a few percent (for example spawn p50 7.38 and 7.78 ms, end-to-end +35% both times). The numbers above are from the second run, on the committed code.

**What the numbers do not say.** One laptop, with a desktop session running, on btrfs with reflinks. On ext4 a checkpoint is a real copy and `fs.write` will cost more. hyperfine warned about outliers. Treat these as "about 6 ms per command, about 14 ms per write, about a third on a short task", not as guarantees.

## 3. What you need to know first

**Percentiles.** Sort all measured times. p50 (the median) is the middle one: half the runs were faster. p95 and p99 are the times that 95% and 99% of runs stayed under. A mean can look fine while one run in a hundred is five times slower; p99 shows that.

**Baseline.** A cost only means something next to "the same thing done without the system". Choosing the baseline is the honest part of a benchmark: comparing a durable write with a non-durable one would blame AgentOS for what fsync costs anyone.

**hyperfine and criterion.** hyperfine runs a *command* many times and times each run from outside; good for whole programs. criterion runs a *function* many times inside one process and does statistics on it; good for small operations.

**Warm-up.** The first runs of anything are slow (files not yet in memory, caches cold). Both tools run some iterations first and throw them away.

**TOCTOU, once more.** "Time of check to time of use": a program checks something (this path is inside the workspace), then acts on it (opens it), and an attacker changes it in between. The cure is to make check and use one operation, which `openat2` does.

**False denial.** The policy refusing something it should allow. A security system is judged on both kinds of mistake.

**FIFO (named pipe).** A special file that carries bytes from one process to another. Opening one for reading waits until some process opens it for writing.

## 4. Experiments

The v1 table lists none for week 10. Things tried and thrown away on the way:

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| TOCTOU attacker v1 | Host thread exchanging two workspace names at full speed | Run died at call 15 (the swapper broke `cp -a` during a checkpoint); test "passed" without its statistics | Assert that the race happened |
| TOCTOU attacker v2 | Same, with resume-until-done | 290 of 300 calls refused: a checkpoint taken mid-exchange held two symlinks | Keep the workspace valid at every instant |
| TOCTOU attacker v3 | One symlink re-pointed by atomic rename | About half the calls land inside, half are refused, 0 escapes | Kept |
| Benchmark run 1 | The whole script before committing | Same numbers as run 2 within a few percent | The measurement is repeatable on this machine |

## 5. What was built

| File | Purpose |
|---|---|
| `tests/security/main.rs` | Group 8: seven new cases (32 in total) |
| `tests/benign.rs` | The benign suite |
| `src/tools/fs.rs` | `fs.read` opens with `O_NONBLOCK` and refuses non-regular files (fix for B-005) |
| `src/runtime.rs` | `create` split from `start` |
| `examples/sandbox_spawn.rs` | Spawn benchmark program (`once`, `samples N out`) |
| `benches/fs_write.rs` | criterion benchmark for `fs.write`, with two direct baselines and raw sample dumps |
| `bench/run.sh` | Runs all three benchmarks, records the machine, scrubs local paths |
| `bench/direct-demo.sh` | The demo task done directly |
| `bench/summarize.py` | Raw data → `summary.md` |
| `Makefile` | `bench`, `eval-security` (adversarial + benign), `eval` |
| `.github/workflows/ci.yml` | CI for the `fhs` preset (never run) |
| `docs/results/bench-nix/`, `docs/results/security-nix.md` | Results of record |
| `docs/bugs.md` | B-005 |

## 6. The code, explained

### 6.1 The TOCTOU test

```rust
let (link, tmp) = (work.join("src/swap"), work.join("src/.swap-tmp"));
while !stop2.load(Ordering::SeqCst) {
    let target: &Path = if swaps.is_multiple_of(2) { &outside2 } else { Path::new("realdir") };
    let _ = std::fs::remove_file(&tmp);
    if std::os::unix::fs::symlink(target, &tmp).is_ok() && std::fs::rename(&tmp, &link).is_ok() {
        swaps += 1;
    }
    std::thread::sleep(Duration::from_micros(150));
}
```

`src/swap` is a symlink. The attacker thread makes a new link under a temporary name and renames it over `src/swap`; `rename` is atomic, so at every instant the link points either inside (`realdir`) or outside. Meanwhile the scripted agent does 150 × (`fs.write src/swap/wN.txt`, `fs.read src/swap/secret.txt`). `AtomicBool` is a flag that can be read and written from several threads safely; `Arc` is a shared pointer that lets both threads own it.

For each call the kernel resolves `src/swap` at the moment of `openat2`. If it points inside, the handle is on `src/realdir` and the write lands there. If it points outside, `openat2` fails with `EXDEV` and the call is denied. There is no third outcome, because there is no gap between check and open. The test's assertions, in plain words: nothing new in the outside folder; the outside secret unchanged; its content in no blob and not in the log; more than 1,000 swaps; at least 20 calls each way.

A text check ("does the cleaned path start with the workspace?") followed by `open` would lose this race: the path string is the same in both cases.

### 6.2 The FIFO fix (B-005)

```rust
let flags = if dir { OFlag::O_RDONLY | OFlag::O_DIRECTORY } else { OFlag::O_RDONLY | OFlag::O_NONBLOCK };
...
let kind = SFlag::from_bits_truncate(st.st_mode) & SFlag::S_IFMT;
if kind != SFlag::S_IFREG {
    return Err(ToolError::new(format!("`{}` is not a regular file", show(rel))));
}
```

`O_NONBLOCK` makes `open` on a FIFO return immediately. `fstat` on the open handle then tells what kind of thing it is (`S_IFMT` masks out the type bits of the mode); only `S_IFREG`, a regular file, is read. Checking the type on the *handle* (not on the path beforehand) means there is again no gap for a swap.

### 6.3 The other new cases, briefly

- `exec_cannot_hardlink_host_files_or_create_devices`: `os.link` from a system file into `/work` fails (different mounts); `mknod` fails (no capability); `/proc/sys` and `/dev/mem` cannot be written or opened.
- `exec_cannot_escape_through_proc_root_links`: `/proc/1/root` is the sandbox's own root; `chroot` needs a capability the command does not have.
- `shipped_policy_refuses_shells_interpreters_and_path_tricks`: nine argvs (`sh -c`, `bash`, `/bin/sh`, `python3 -c`, bare `python3`, `env pytest`, `"pytest; cat .env"` as one word, `../../bin/sh`, `python3 -m http.server`) are all denied with `no matching rule`, and none reaches an intent.
- `denied_paths_cannot_be_created_or_read_later`: writing `.env`, `src/.env`, `src/key.pem` is denied; a `.env` created by a command is removed by the post-exec revert and its content is nowhere in the execution folder.
- `output_flood_is_truncated_and_does_not_fill_the_log`: 52 MB on stdout and stderr; the command finishes (exit 0), the kept output is ≤ 64 KB, the log stays under 2 MB.

### 6.4 The benign suite

`STEPS` is a list of `(name, TOML step)` pairs. The test builds one script from them, runs it under the unmodified preset policy, and for each call requires a `tool.completed` and no `tool.denied`, `tool.failed` or `policy.violation`. The three pytest calls must exit 0. A failure message names the step and the rule, which is what makes a false denial quick to fix.

### 6.5 Spawn benchmark: `examples/sandbox_spawn.rs`

```rust
let t = Instant::now();
let cg = cgroups.create(&name, &policy.limits)?;
let out = sandbox::run(&helper, &spec, &cg, Duration::from_secs(10), 4096)?;
let d = t.elapsed();
```

Timed: create the cgroup, start the helper, build namespaces and root, apply Landlock, `execve(true)`, wait, remove the cgroup. Each sandboxed spawn is followed by a direct `Command::new(true)` for the baseline, so both see the same machine state. `arg0("true")` matters on NixOS, where `true` is a multi-call binary. An *example* in Cargo is a small program built with the library; it lives in `examples/` and is not part of the shipped binary.

hyperfine can only time a whole program, so `sandbox_spawn once` also includes that program starting, reading the policy, two `systemctl` calls and the cgroup probe: 39 ms against 7.8 ms for the spawn alone. The summary shows both and says which is which.

### 6.6 Write benchmark: `benches/fs_write.rs`

```rust
c.bench_function("fs.write 4 KB through AgentOS (intent fsync + checkpoint)", |b| {
    b.iter(|| {
        i += 1;
        let t = Instant::now();
        let r = gw.handle(ToolCall::FsWrite { path: "src/bench.py".into(), content: content(i) }).unwrap();
        agentos_samples.push(t.elapsed().as_secs_f64() * 1e6);
        assert_eq!(r.outcome, ToolOutcome::Ok);
    })
});
```

`b.iter` is criterion's loop; it decides how many times to call the closure. The closure also records its own time per call, because criterion reports a mean with a confidence interval and the plan asks for percentiles. The content changes every call (`content(i)`), so each write is a real change. Two baselines: `direct_durable` (temp file, fsync, rename: what the gateway does to the file itself) and a plain `fs::write`.

Where 13.7 ms goes was **not** measured piece by piece. By reading the code, one call does three fsynced log appends, one fsynced blob, one fsynced temp file with a rename, and one `cp -a --reflink=auto` process; at about 2 ms per fsync (the durable baseline) that adds up to roughly the measured total.

### 6.7 `bench/run.sh` and `bench/summarize.py`

The script re-runs itself under `systemd-run` if it is not already in a delegated scope, builds in release mode, writes `system.txt`, runs the three benchmarks, replaces the repository's absolute path with `<repo>` in the output, and calls the summarizer. `summarize.py` computes each percentile from the raw files (`pct` sorts and indexes), so the table cannot disagree with the data.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-061 | What each benchmark times and against which baseline; raw samples kept | A hidden CLI subcommand; plain-write-only baseline; criterion's summary as "percentiles" |
| D-062 | TOCTOU test with a host-thread attacker re-pointing one symlink | Exchanging two names; a sandboxed swapper (impossible) |
| D-063 | Benign suite of 23 operations under the shipped policy | Counting the demo as the benign suite |
| D-064 | `fs.read` reads regular files only, opened non-blocking | A timeout thread; removing special files in the revert |
| D-065 | CI workflow for `fhs`, written and marked unverified | Leaving CI out; a local container |

## 8. Problems hit

1. **B-005: FIFO hang.** Expected the new FIFO test to pass or fail quickly; it timed out at 20 s. `fs.read` on a FIFO blocked in `open`. Fixed with `O_NONBLOCK` and a file-type check. In `docs/bugs.md`.
2. **Canary in the log.** A test grepped the execution folder for a canary string and found it in `log.db` and `script.toml`: the string was part of the attack's own argv, which is logged by design. The test now assembles the canary inside the command (`'LATER-' + 'CANARY'`). Test bug.
3. **TOCTOU test passed without testing.** First version: the run died after 15 calls and the test skipped its checks (it tolerated an empty run id). Now the test resumes until done and asserts on swap and outcome counts.
4. **TOCTOU attacker spoiled its own race.** Second version: 290 of 300 refused, because a checkpoint copied during an exchange of two names contained no real folder. Third version re-points one symlink.
5. **Direct baseline failed.** `Error: direct true failed`: `true` resolved to `coreutils`, which must be called as `true`. Fixed with `arg0`.
6. **Summary script crashed.** It told the two end-to-end commands apart by the word "agentos", which is also the repository folder's name. Now it looks for `direct-demo`.
7. **Local paths in results.** The first results held this machine's absolute path. `run.sh` now writes `<repo>`; two older files were corrected.

## 9. How to run it

```sh
nix develop
make eval-security     # 32 adversarial + 1 benign test; prints the toctou and benign lines
make bench             # about 3 minutes; writes docs/results/bench-nix/
cat docs/results/bench-nix/summary.md
make eval              # all three suites (adds 200 kills per cell: about 8 more minutes)
```

`make bench` overwrites `docs/results/bench-<preset>/`; compare with `git diff` before committing new numbers.

## 10. What this milestone does not do

- **`fhs` preset not yet verified**; the CI file has never run.
- **One machine, one filesystem.** No ext4 or XFS numbers; no numbers under load.
- **No breakdown of the `fs.write` cost by measurement**, only by reading the code.
- **No benchmark of recovery time beyond the harness's p50/p99** (M3), and none of the LLM agent.
- **Still out of scope:** seccomp, kernel bugs, disk filling, side channels (threat model). `/proc/self/mountinfo` inside the sandbox shows the host path of the workspace; that is a path name, not file content, and is listed as a known information leak in the threat model (M6).
- **The TOCTOU attacker is a host thread**, which the threat model does not grant an attacker. The test shows the file tools hold even then; it does not show that checkpoints tolerate concurrent host-side changes (they do not, and need not).

## 11. Check your understanding

1. Why report p50, p95 and p99 instead of the average?
2. The spawn benchmark has two results, 7.8 ms and 39 ms. What does each include, and which one describes the cost of an `exec` call inside a run?
3. Why are there two "direct" baselines for `fs.write`, and which is the fair one?
4. List the durable operations in one `fs.write` call. Which decision could cut that number, and what would it cost?
5. The end-to-end slowdown is 35% on a half-second task. Would you expect a higher or lower percentage on a task whose tests take 30 seconds, and why?
6. In the TOCTOU test, what are the only two outcomes of a call, and why is there no third?
7. Why did the first TOCTOU test "pass", and what was added so it cannot pass that way again?
8. Why can the swapper not be a process inside the sandbox?
9. How could a command make `fs.read` hang, and why did `openat2` not prevent it?
10. Why is a benign suite part of a *security* evaluation?
11. The CI file is in the repository but the `fhs` row says "not yet verified". What would change that, and who can do it?
12. Someone runs `make bench` on ext4 and gets 40 ms per write. Which part of the design explains the difference?

## Notes added after the milestone

The text above is left as it was written. Later changes are listed here.

- **2026-11-13, `fhs` preset.** Every "not yet verified" about the `fhs` preset above describes the state when this document was written. Prince pushed on 2026-11-12. The first two CI runs failed the 200-kill suite and found bugs B-007 and B-008 (`docs/bugs.md`, D-070, D-071). The third, <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> on commit `c3c9502` (Ubuntu 24.04), is green: lint, all test suites (32 security cases, the benign suite) and 4,800 kills with 0 violations. Results of record: `docs/results/recovery-200-fhs.md`, `docs/results/security-fhs.md`. See D-072. Still not run on Ubuntu: the benchmarks and `make demo`.
