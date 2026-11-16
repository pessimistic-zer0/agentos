#!/usr/bin/env bash
# M11 experiment (D-114, A3): three ways to make a checkpoint that does not
# hold a scratch folder. Usage: run.sh <project with a build folder> <scratch folder name>
# Works on a copy under ./tmp; `--reflink=never` stands in for a filesystem
# without shared blocks (ext4, the CI runner).
set -euo pipefail
src=$1; scratch=$2
here=$(cd "$(dirname "$0")" && pwd); t=$here/tmp
mkdir -p "$t"; w=$t/work
[ -e "$w" ] || cp -a "$src" "$w"
echo "workspace: $(du -sm "$w" | cut -f1) MB, scratch $scratch: $(du -sm "$w/$scratch" | cut -f1) MB, $(find "$w/$scratch" | wc -l) entries"
ms() { local s=$(date +%s%N); "$@"; echo $(( ($(date +%s%N) - s) / 1000000 )); }
for mode in auto never; do
  n=0
  full()   { cp -a --reflink=$mode -- "$w" "$t/cp-$mode-full-$n"; }
  delete() { cp -a --reflink=$mode -- "$w" "$t/cp-$mode-del-$n"; find "$t/cp-$mode-del-$n/$scratch" -mindepth 1 -delete; }
  stash()  { mv "$w/$scratch" "$t/stash"; cp -a --reflink=$mode -- "$w" "$t/cp-$mode-stash-$n"; mv "$t/stash" "$w/$scratch"; }
  for f in full delete stash; do
    r=""; for n in 1 2 3; do r="$r $(ms $f)"; done
    echo "reflink=$mode $f: ms$r; one checkpoint holds $(du -sm "$t"/cp-$mode-${f:0:3}*-3 | cut -f1) MB"
  done
done
