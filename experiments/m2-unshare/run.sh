#!/bin/sh
# Experiment: what does a process see inside `unshare -Urmpn --fork --mount-proc`?
# -U user ns, -r map our uid to root inside, -m mount ns, -p pid ns, -n net ns.
echo "outside: uid=$(id -u) pid=$$ ifaces=$(ls /sys/class/net | tr '\n' ' ')"
unshare -Urmpn --fork --mount-proc sh -c '
  echo "inside:  uid=$(id -u) pid=$$"
  echo "  processes visible: $(ls /proc | grep -c "^[0-9]")"
  echo "  network interfaces: $(ls /sys/class/net 2>/dev/null | tr "\n" " ") (ip link: $(cat /proc/net/dev | tail -n +3 | cut -d: -f1 | tr -d " " | tr "\n" " "))"
  echo "  CapEff: $(grep CapEff /proc/self/status | cut -f2)"
  echo "  uid_map: $(cat /proc/self/uid_map | tr -s " ")"
  echo "  can read ~/.ssh? $(ls -d $HOME/.ssh 2>/dev/null && echo yes || echo no-or-absent)"
  echo "  connect to 1.1.1.1:80: $(timeout 2 sh -c "exec 3<>/dev/tcp/1.1.1.1/80" 2>&1 && echo connected || echo failed)"
  python3 -c "import socket; s=socket.socket(); s.settimeout(2); s.connect((\"1.1.1.1\",80))" 2>&1 | tail -1 | sed "s/^/  python connect: /"
  echo "  kill host pid 1: $(kill -0 1 && echo "pid 1 is $(cat /proc/1/comm)")"
'
