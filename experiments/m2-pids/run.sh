#!/bin/sh
# Experiment: does pids.max stop a fork bomb? Run under a delegated cgroup:
#   systemd-run --user --scope -p Delegate=yes ./run.sh
set -u
me=$(cut -d: -f3 /proc/self/cgroup)
root=/sys/fs/cgroup$me
echo "started in cgroup $me"
echo "controllers available: $(cat $root/cgroup.controllers)"
# "No internal processes": a cgroup that hands controllers to children may not
# hold processes itself. Move everything here into a leaf first.
mkdir -p $root/leaf
for p in $(cat $root/cgroup.procs); do echo $p > $root/leaf/cgroup.procs 2>/dev/null; done
echo "+pids +memory +cpu" > $root/cgroup.subtree_control && echo "enabled: $(cat $root/cgroup.subtree_control)"
mkdir -p $root/bomb
echo 32 > $root/bomb/pids.max
# The bomb: python forks as fast as it can; each child forks too.
sh -c "echo \$\$ > $root/bomb/cgroup.procs; exec python3 -c '
import os, time
ok = fail = 0
end = time.time() + 2
while time.time() < end:
    try:
        if os.fork() == 0:
            time.sleep(3); os._exit(0)
        ok += 1
    except OSError as e:
        fail += 1
print(f\"bomb: {ok} forks ok, {fail} refused, first error EAGAIN expected\")
'" &
sleep 1
echo "pids.current while running: $(cat $root/bomb/pids.current) (max $(cat $root/bomb/pids.max))"
echo "pids.events: $(cat $root/bomb/pids.events | tr '\n' ' ')"
wait
echo 1 > $root/bomb/cgroup.kill
sleep 0.5
echo "after cgroup.kill: populated=$(grep populated $root/bomb/cgroup.events)"
rmdir $root/bomb && echo "bomb cgroup removed"
