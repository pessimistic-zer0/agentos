#!/usr/bin/env bash
# How `v1.2-executions.tar` was made, once, on 2026-11-28 (D-172). Kept as a
# record. Never run it again to "update" the fixture: the point of the
# fixture is that it was written by norn 1.2.0 and stays as it was.
#
# Usage (inside `nix develop`, in a delegated scope):
#   NORN=<norn 1.2.0 built from the tag v1.2> FAKE=<its fake_service example> \
#     tests/fixtures/compat/make-v1.2.sh <empty work folder>
#
# Three executions, renamed so the tests can find them:
#   v12-network    scripted: a denial, a command with one allowed and one
#                  refused connection, an ext.call, a write; finished
#   v12-recovered  the demo task killed at P4 of call 6, `norn recover`,
#                  then `norn run --resume` (1.2.0 records no event for the
#                  resume, D-170); finished
#   v12-approval   norn mcp: a write, then fs.delete waits for approval, `norn approve`,
#                  asked again and done; `norn finish`
set -euo pipefail
W=$(realpath "$1")
REPO_ROOT=$(cd "$(dirname "$0")/../../.." && pwd)
export NORN_STATE_DIR="$W/state" NORN_IN_SCOPE=1
mkdir -p "$W/state"

# A repository: the demo, plus a client that fetches two URLs through the proxy.
cp -a "$REPO_ROOT/examples/demo-repo" "$W/repo"
cat > "$W/repo/client.py" <<'EOF'
import os, sys, urllib.request
os.environ.pop("NO_PROXY", None); os.environ.pop("no_proxy", None)
for url in sys.argv[1:]:
    try:
        print(url, urllib.request.urlopen(url, timeout=5).read().decode())
    except Exception as e:
        print(url, "error", type(e).__name__)
EOF

# Two small web servers on the host: one the policy allows, one it does not.
python3 -m http.server 18431 --bind 127.0.0.1 --directory "$W/repo" >/dev/null 2>&1 & S1=$!
python3 -m http.server 18432 --bind 127.0.0.1 --directory "$W/repo" >/dev/null 2>&1 & S2=$!
"$FAKE" 18433 >/dev/null 2>&1 & S3=$!
trap 'kill $S1 $S2 $S3 2>/dev/null' EXIT
sleep 1

sed 's/^allow = \[$/allow = [\n  ["python3", "client.py"],/' "$REPO_ROOT/policies/nix.toml" > "$W/policy-network.toml"
cat >> "$W/policy-network.toml" <<'EOF'

[network]
allow = ["127.0.0.1:18431"]

[external]
timeout_ms = 2000
attempts = 1

[[external.service]]
name = "ledger"
url = "http://127.0.0.1:18433/apply"
EOF
cat > "$W/network.toml" <<'EOF'
task = "fixture v1.2: network, ext.call, a denial, a write"
[[step]]
tool = "fs.read"
path = ".env"
[[step]]
tool = "exec"
argv = ["python3", "client.py", "http://127.0.0.1:18431/README.md", "http://127.0.0.1:18432/README.md"]
[[step]]
tool = "ext.call"
service = "ledger"
payload = "order-1"
[[step]]
tool = "fs.write"
path = "src/notes.py"
content = "NOTE = 'written by the v1.2 fixture'\n"
EOF
id=$("$NORN" run --quiet --repo "$W/repo" --policy "$W/policy-network.toml" --script "$W/network.toml")
mv "$W/state/executions/$id" "$W/state/executions/v12-network"

# Killed after the effect of call 6, recovered, then resumed to the end.
NORN_CRASH_AT=P4 NORN_CRASH_CALL=6 "$NORN" run --quiet --repo "$REPO_ROOT/examples/demo-repo" --policy "$REPO_ROOT/policies/nix.toml" \
  --script "$REPO_ROOT/examples/scripts/fix-demo.toml" || true
id=$(ls "$W/state/executions" | grep -v '^v12-')
"$NORN" recover --quiet "$id"
"$NORN" run --quiet --resume "$id" --agent scripted
mv "$W/state/executions/$id" "$W/state/executions/v12-recovered"

# norn mcp, with fs.delete waiting for a person.
cp "$REPO_ROOT/policies/nix.toml" "$W/policy-approval.toml"
printf '\n[approvals]\nrequire = ["fs.delete:**"]\n' >> "$W/policy-approval.toml"
python3 -I - "$NORN" "$W/repo" "$W/policy-approval.toml" <<'EOF'
import json, re, subprocess, sys
norn, repo, policy = sys.argv[1:]
meta = {"io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {},
        "io.modelcontextprotocol/clientInfo": {"name": "fixture", "version": "0"}}
p = subprocess.Popen([norn, "mcp", "--repo", repo, "--policy", policy], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
n = 0
def call(tool, args):
    global n
    n += 1
    p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": n, "method": "tools/call", "params": {"name": tool, "arguments": args, "_meta": meta}}) + "\n")
    p.stdin.flush()
    return json.loads(p.stdout.readline())["result"]["content"][0]["text"]
call("fs_read", {"path": "src/stats.py"})
call("fs_write", {"path": "src/old.py", "content": "OLD = 1\n"})
text = call("fs_delete", {"path": "src/old.py"})
m = re.search(r"norn approve (\S+) (\S+?)[`).]", text)
subprocess.run([norn, "approve", m.group(1), m.group(2)], check=True, stdout=subprocess.DEVNULL)
print(call("fs_delete", {"path": "src/old.py"})[:60], file=sys.stderr)
p.stdin.close(); p.wait()
EOF
id=$(ls "$W/state/executions" | grep -v '^v12-')
"$NORN" finish "$id" >/dev/null
mv "$W/state/executions/$id" "$W/state/executions/v12-approval"

# Pack the three folders exactly as they are.
cd "$W/state"
tar --sort=name --owner=0 --group=0 --numeric-owner -cf "$W/v1.2-executions.tar" executions
echo "$W/v1.2-executions.tar"
