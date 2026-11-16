#!/usr/bin/env bash
# M12 experiment (D-114, B4): mount a folder of the original repo read-only
# at its place inside a copy of the repo, in a private mount namespace, and
# try to change it. Usage: run.sh   (needs unshare; no root)
set -u
here=$(cd "$(dirname "$0")" && pwd); t=$here/tmp
[ -d "$t" ] && chmod -R u+w "$t" && find "$t" -mindepth 1 -delete
mkdir -p "$t/repo/node_modules/pkg" "$t/repo/src" "$t/work/src" "$t/work/node_modules"
echo 'module.exports = 42' > "$t/repo/node_modules/pkg/index.js"
echo "require('pkg')" > "$t/repo/src/a.js"; cp "$t/repo/src/a.js" "$t/work/src/"
unshare -Urm bash -c '
  t=$1
  mount --bind "$t/repo/node_modules" "$t/work/node_modules"
  mount -o remount,bind,ro,nosuid,nodev "$t/work/node_modules"
  cd "$t/work"
  try() { local name=$1; shift; if out=$("$@" 2>&1); then echo "$name: WORKED"; else echo "$name: refused ($(echo "$out" | tail -1 | sed "s|$t/||g"))"; fi; }
  try "read a file"            cat node_modules/pkg/index.js
  try "change a file"          sh -c "echo x >> node_modules/pkg/index.js"
  try "new file"               touch node_modules/new.js
  try "delete a file"          rm node_modules/pkg/index.js
  try "chmod"                  chmod 777 node_modules/pkg/index.js
  try "remove the folder"      rm -rf node_modules
  try "rename the folder"      mv node_modules nm2
  try "rename a parent"        sh -c "cd .. && mv work work2 && ls work2/node_modules/pkg && mv work2 work"
  try "write beside it"        touch src/new.js
  grep -c node_modules /proc/self/mountinfo | sed "s/^/mountinfo lines naming node_modules: /"
' _ "$t"
echo "host view of work/node_modules after: $(ls "$t/work/node_modules" | wc -l) entries (the mount was private)"
echo "original: $(cat "$t/repo/node_modules/pkg/index.js")"
