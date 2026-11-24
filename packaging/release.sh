#!/bin/sh
# Build the release and pack it the way a user gets it.
#
#   packaging/release.sh           build, pack into target/dist/
#   packaging/release.sh --check   also unpack the archive somewhere else and
#                                  run the demo task with what is in it
#
# The archive holds the program, the policy presets, the AppArmor profile,
# the demo project and the documents a user needs. It is named after the
# version in Cargo.toml and the machine type. The program is linked against
# the C library of the system it was built on, so an archive built on Ubuntu
# 24.04 runs on systems with that glibc (2.39) or a newer one.
set -eu
cd "$(dirname "$0")/.."
version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -n 1)
name="norn-$version-$(uname -m)-linux"
dist=$PWD/target/dist
stage=$dist/$name

cargo build --release --locked --bin norn

rm -rf "$stage" "$dist/$name.tar.gz" "$dist/$name.tar.gz.sha256"
mkdir -p "$stage/docs" "$stage/examples/scripts" "$stage/packaging/apparmor"
install -m 755 target/release/norn "$stage/norn"
# The release profile keeps line tables for profiling (48 MB). The packed
# copy drops them; function names stay, so a panic still says where.
strip --strip-debug "$stage/norn"
cp policies/*.toml "$stage/" 2>/dev/null && mkdir -p "$stage/policies" && mv "$stage"/*.toml "$stage/policies/"
cp packaging/apparmor/norn "$stage/packaging/apparmor/"
cp README.md CHANGELOG.md LICENSE-MIT LICENSE-APACHE "$stage/"
cp docs/compatibility.md docs/threat-model.md docs/recovery-semantics.md "$stage/docs/"
cp -r examples/demo-repo "$stage/examples/"
cp examples/scripts/fix-demo.toml "$stage/examples/scripts/"
# No leftovers of earlier test runs in the demo project.
find "$stage/examples" \( -name __pycache__ -o -name .pytest_cache \) -prune -exec rm -rf {} +

# The same bytes for the same input: fixed order, owner and time.
tar -C "$dist" --sort=name --owner=0 --group=0 --numeric-owner --mtime='2026-01-01 00:00:00Z' -czf "$dist/$name.tar.gz" "$name"
(cd "$dist" && sha256sum "$name.tar.gz" > "$name.tar.gz.sha256")
rm -rf "$stage"
echo "packed: $dist/$name.tar.gz ($(du -h "$dist/$name.tar.gz" | cut -f1))"
cat "$dist/$name.tar.gz.sha256"

if [ "${1:-}" = "--check" ]; then
  try=$(mktemp -d)
  trap 'rm -rf "$try"' EXIT
  tar -C "$try" -xzf "$dist/$name.tar.gz"
  cd "$try/$name"
  ./norn --version
  preset=${NORN_TEST_PRESET:-nix}
  # The packed program, the packed policy, the packed demo project.
  id=$(./norn --state-dir "$try/state" run --quiet --repo examples/demo-repo --policy "policies/$preset.toml" --script examples/scripts/fix-demo.toml)
  ./norn --state-dir "$try/state" trace "$id" | tail -n 4
  ./norn --state-dir "$try/state" trace "$id" | grep -q "finished: completed"
  grep -q "return sum(values) / len(values)" "$try/state/executions/$id/result.patch"
  echo "check: the packed program ran the demo task from the archive; the patch holds the fix"
fi
