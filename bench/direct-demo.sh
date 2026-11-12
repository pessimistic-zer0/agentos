#!/bin/sh
# The demo task done directly, without AgentOS: the same steps as
# examples/scripts/fix-demo.toml (look, run tests, read, fix, run tests), on
# a fresh copy of the repo. Baseline for the end-to-end benchmark.
# Usage: direct-demo.sh <repo> <scratch-dir>
set -eu
repo=$1
work=$2/direct-$$
rm -rf "$work"
cp -a "$repo" "$work"
cd "$work"
ls > /dev/null
pytest -q > /dev/null 2>&1 || true          # fails before the fix
cat tests/test_stats.py src/stats.py > /dev/null
sed -i 's|sum(values) / (len(values) - 1)|sum(values) / len(values)|' src/stats.py
pytest -q > /dev/null 2>&1                  # must pass now
cd /
rm -rf "$work"
