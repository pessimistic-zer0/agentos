# Benchmarks on Ubuntu (`fhs` preset): a record of one CI run

These files are the artifact `bench-fhs` of CI run 43903121563 (job `release`, commit `f38fff9`, 2026-11-16), downloaded unchanged with `gh run download 43903121563 -n bench-fhs`. `summary.md` is what `bench/run.sh` wrote there; `system.txt` describes the machine.

Read them as "the benchmarks run on Ubuntu, and this is what one shared CI machine measured", not as the project's numbers:

- The machine is a shared virtual machine with 2 CPU threads; other work on the same host changes the timings from run to run. The `fs.write` p99 here (135.66 ms against a p50 of 3.77 ms) is that kind of noise.
- The figures in the README come from the development machine (`docs/results/bench-nix-m8/`), where the machine was idle.

What this run does establish: the three benchmarks build and run to the end on Ubuntu 24.04 with the `fhs` preset, which had not been tried since B-013.
