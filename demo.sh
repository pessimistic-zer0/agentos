#!/bin/sh
# The AgentOS demo: fix a failing test in a sandbox, then survive a kill -9.
# Usage: ./demo.sh          (or: make demo)
# Needs: cargo, pytest on PATH, systemd user session. See README.md.
set -eu
cd "$(dirname "$0")"
if [ -z "${AGENTOS_IN_SCOPE:-}" ]; then
  # The sandbox needs a cgroup that systemd delegated to this user.
  exec env AGENTOS_IN_SCOPE=1 systemd-run --user --scope -p Delegate=yes -q "$0" "$@"
fi
# Which policy preset fits this system: NixOS keeps programs in /nix/store.
if [ -n "${AGENTOS_TEST_PRESET:-}" ]; then preset=$AGENTOS_TEST_PRESET
elif [ -d /nix/store ] && [ ! -e /usr/bin/python3 ]; then preset=nix
else preset=fhs; fi
policy=policies/$preset.toml
state=${AGENTOS_DEMO_STATE:-$(mktemp -d)}
say() { printf '\n\033[1m== %s\033[0m\n' "$*"; }

say "Building agentos"
cargo build --quiet
A="target/debug/agentos --state-dir $state"

say "1. The demo repo has a failing test (run directly, outside AgentOS, on a copy)"
tmp=$(mktemp -d); cp -a examples/demo-repo "$tmp/repo"
(cd "$tmp/repo" && pytest -q 2>&1 | tail -3) || true
rm -rf "$tmp"

say "2. A scripted agent fixes it. Every call goes through policy ($policy), log and sandbox"
$A run --repo examples/demo-repo --policy "$policy" --script examples/scripts/fix-demo.toml
id1=$(ls "$state/executions" | tail -1)

say "3. The result is a patch; the original repo is untouched"
grep -v '^Binary files' "$state/executions/$id1/result.patch" | sed -n '1,12p'
git status --short examples/demo-repo 2>/dev/null | sed 's/^/   (git status) /' || true
echo "   .env in the workspace? $( [ -e "$state/executions/$id1/work/.env" ] && echo yes || echo 'no: denied files are never copied in' )"

say "4. Same task again, but the runtime is killed (SIGKILL) right after the file write, before its checkpoint"
AGENTOS_CRASH_AT=P4 AGENTOS_CRASH_CALL=6 $A run --repo examples/demo-repo --policy "$policy" --script examples/scripts/fix-demo.toml || echo "   -> exit code $? (137 = killed by signal 9)"
id2=$(ls "$state/executions" | grep -v "^$id1\$")   # the execution that is not the first one
$A status "$id2"

say "5. Recovery: was call #6 applied? The intent stored the file's hash before and after"
$A recover "$id2"

say "6. Resume: the agent continues after the recovered call; nothing is done twice"
$A run --resume "$id2"

say "7. The trace of the crashed-and-resumed run"
$A trace "$id2" | sed -n '/^#1 /,/^finished/p'

say "Done. State kept in $state (delete it when finished)"
