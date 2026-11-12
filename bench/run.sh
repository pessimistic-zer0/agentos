#!/bin/sh
# Run the three overhead benchmarks and write raw data plus a summary.
# Usage (inside the dev shell): bench/run.sh [out-dir]     (or: make bench)
# Needs: hyperfine, a release build, a delegated cgroup (this script re-runs
# itself under systemd-run if needed).
set -eu
cd "$(dirname "$0")/.."
if [ -z "${AGENTOS_IN_SCOPE:-}" ]; then
  exec env AGENTOS_IN_SCOPE=1 systemd-run --user --scope -p Delegate=yes -q "$0" "$@"
fi
preset=${AGENTOS_TEST_PRESET:-nix}
out=${1:-docs/results/bench-$preset}
mkdir -p "$out"
out=$(cd "$out" && pwd)
scratch=$PWD/target/bench-scratch
rm -rf "$scratch"; mkdir -p "$scratch"

cargo build --release --bin agentos --example sandbox_spawn
bin=$PWD/target/release

{
  echo "date:       $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "cpu:        $(grep -m1 'model name' /proc/cpuinfo | cut -d: -f2 | sed 's/^ //') ($(nproc) threads)"
  echo "ram:        $(awk '/MemTotal/ {printf "%.1f GiB", $2/1048576}' /proc/meminfo)"
  echo "kernel:     $(uname -sr)"
  echo "os:         $(. /etc/os-release && echo "$PRETTY_NAME")"
  echo "filesystem: $(stat -f -c %T "$scratch") (state and workspace under target/)"
  echo "rustc:      $(rustc --version)"
  echo "build:      release"
  echo "preset:     $preset"
  echo "python:     $(python3 --version 2>&1), pytest $(python3 -c 'import pytest; print(pytest.__version__)')"
  echo "hyperfine:  $(hyperfine --version)"
  echo "commit:     $(git rev-parse --short HEAD)$(git diff --quiet || echo ' (with uncommitted changes)')"
} > "$out/system.txt"
cat "$out/system.txt"

echo "== 1. sandbox spawn"
"$bin/examples/sandbox_spawn" samples 500 "$out"
true_bin=$(command -v true)   # not resolved: it may be a multi-call binary that checks its name
hyperfine -N --warmup 20 --runs 300 --export-json "$out/hyperfine-spawn.json" \
  "$bin/examples/sandbox_spawn once" "$true_bin" > "$out/hyperfine-spawn.txt"

echo "== 2. fs.write 4 KB"
AGENTOS_BENCH_OUT="$out" cargo bench --bench fs_write 2>&1 | grep -E "time:|Benchmarking .*: Analyzing|^fs.write|^direct" > "$out/criterion-fs_write.txt" || true

echo "== 3. end-to-end demo"
hyperfine -N --warmup 3 --runs 100 --export-json "$out/hyperfine-e2e.json" \
  --prepare "rm -rf $scratch/state" \
  "$bin/agentos --state-dir $scratch/state run --quiet --repo examples/demo-repo --policy policies/$preset.toml --script examples/scripts/fix-demo.toml" \
  "bench/direct-demo.sh examples/demo-repo $scratch" > "$out/hyperfine-e2e.txt"

rm -rf "$scratch"
python3 bench/summarize.py "$out" > /dev/null
echo "summary: $out/summary.md"
