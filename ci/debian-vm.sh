#!/usr/bin/env bash
# Run the test suite on Debian, in a virtual machine (D-128).
#
# GitHub only offers Ubuntu machines, and a container shares the host's
# kernel and cannot give an ordinary user a systemd session with a delegated
# cgroup. So this starts a real Debian system under QEMU/KVM from Debian's
# own cloud image, copies the repository in, installs what the README lists,
# and runs `make test` there as an ordinary user, with the `fhs` preset.
#
#   ci/debian-vm.sh            Debian 12 "bookworm" (kernel 6.1)
#   DEBIAN=13 ci/debian-vm.sh  Debian 13 "trixie"
#
# Needs on the host: qemu-system-x86_64, qemu-img, /dev/kvm, ssh, curl,
# python3. Downloads about 350 MB the first time (kept in $CACHE). Nothing
# on the host is changed outside $WORK and $CACHE.
set -euo pipefail
cd "$(dirname "$0")/.."
DEBIAN=${DEBIAN:-12}
case "$DEBIAN" in
  12) code=bookworm ;;
  13) code=trixie ;;
  *) echo "DEBIAN must be 12 or 13" >&2; exit 2 ;;
esac
CACHE=${CACHE:-$HOME/.cache/agentos-ci}
WORK=${WORK:-$PWD/target/debian-vm}
PORT=${PORT:-2222}
SEED_PORT=${SEED_PORT:-8791}
image=$CACHE/debian-$DEBIAN-genericcloud-amd64.qcow2
url=https://cloud.debian.org/images/cloud/$code/latest/debian-$DEBIAN-genericcloud-amd64.qcow2

mkdir -p "$CACHE"
rm -rf "$WORK"; mkdir -p "$WORK/seed"
if [ ! -s "$image" ]; then
  echo "== downloading $url"
  curl --fail --location --silent --show-error --retry 3 -o "$image.part" "$url"
  mv "$image.part" "$image"
fi

# A key for this run only; the VM's first-boot setup (cloud-init) makes a
# user `ci` that accepts it and may use sudo.
ssh-keygen -q -t ed25519 -N '' -f "$WORK/key"
cat > "$WORK/seed/user-data" <<SEED
#cloud-config
users:
  - name: ci
    shell: /bin/bash
    sudo: ALL=(ALL) NOPASSWD:ALL
    ssh_authorized_keys:
      - $(cat "$WORK/key.pub")
SEED
printf 'instance-id: agentos-ci\nlocal-hostname: debian-ci\n' > "$WORK/seed/meta-data"
# cloud-init reads its settings over HTTP from the host (10.0.2.2 is the
# host as the VM sees it), which avoids building a CD image.
python3 -m http.server "$SEED_PORT" --bind 127.0.0.1 --directory "$WORK/seed" > "$WORK/seed.log" 2>&1 &
seed_pid=$!

# The image itself stays untouched: the VM writes to a layer on top of it.
qemu-img create -q -f qcow2 -b "$image" -F qcow2 "$WORK/disk.qcow2" 24G
qemu-system-x86_64 -enable-kvm -cpu host -smp "${VM_CPUS:-4}" -m "${VM_MEM:-6G}" \
  -display none -serial "file:$WORK/console.log" \
  -drive "file=$WORK/disk.qcow2,if=virtio" \
  -netdev "user,id=n0,hostfwd=tcp:127.0.0.1:$PORT-:22" -device virtio-net-pci,netdev=n0 \
  -smbios "type=1,serial=ds=nocloud-net;s=http://10.0.2.2:$SEED_PORT/" \
  -daemonize -pidfile "$WORK/qemu.pid"
cleanup() {
  kill "$seed_pid" 2>/dev/null || true
  [ -f "$WORK/qemu.pid" ] && kill "$(cat "$WORK/qemu.pid")" 2>/dev/null || true
}
trap cleanup EXIT

vm() { ssh -q -i "$WORK/key" -p "$PORT" -o StrictHostKeyChecking=no -o UserKnownHostsFile=/dev/null -o ConnectTimeout=5 ci@127.0.0.1 "$@"; }
echo "== waiting for the VM"
for i in $(seq 1 90); do
  if vm true 2>/dev/null; then break; fi
  [ "$i" = 90 ] && { echo "the VM did not come up; last lines of its console:"; tail -n 30 "$WORK/console.log"; exit 1; }
  sleep 2
done
vm 'cloud-init status --wait >/dev/null 2>&1 || true; . /etc/os-release; echo "system: $PRETTY_NAME, kernel $(uname -r), $(nproc) CPUs"; echo "lsm: $(cat /sys/kernel/security/lsm)"; echo "cgroup: $(stat -fc %T /sys/fs/cgroup)"'

echo "== installing what the README lists"
vm 'sudo DEBIAN_FRONTEND=noninteractive apt-get update -qq && sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq build-essential python3-pytest python3-venv diffutils curl ca-certificates > /dev/null && sudo loginctl enable-linger ci'
vm 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable > /dev/null 2>&1; . ~/.cargo/env; rustc --version'

echo "== copying the repository (tracked files, as they are in the working tree)"
git ls-files -z | tar --null -T - -cf - | vm 'mkdir -p agentos && tar -C agentos -xf -'

echo "== make test, preset fhs"
set +e
vm '. ~/.cargo/env; cd agentos && AGENTOS_TEST_PRESET=fhs make test 2>&1' | tee "$WORK/test.log" | grep -E "^test .*(FAILED|ok)$|test result|panicked|Running|error" 
status=${PIPESTATUS[0]}
set -e
echo "== summary"
grep -E "^test result" "$WORK/test.log" | awk '{p+=$4; f+=$6} END {print "passed", p+0, "failed", f+0}'
vm 'sudo poweroff' 2>/dev/null || true
exit "$status"
