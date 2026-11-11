# m2-pids

**Tried:** a fork bomb inside a cgroup with `pids.max = 32`, under a delegated
cgroup (`systemd-run --user --scope -p Delegate=yes ./run.sh`). The script first
moves every process of the scope into a `leaf` child, enables the `pids`,
`memory` and `cpu` controllers for children, makes a `bomb` child group, starts
a Python process there that forks as fast as it can for 2 seconds, then kills
the group with `cgroup.kill`.

**Showed (2026-11-11, kernel 7.1.8):**

```text
started in cgroup /user.slice/user-1000.slice/user@1000.service/app.slice/run-p135225-i35006.scope
controllers available: cpu io memory pids
enabled: cpu memory pids
pids.current while running: 32 (max 32)
pids.events: max 7909
bomb: 31 forks ok, 14922 refused, first error EAGAIN expected
after cgroup.kill: populated=populated 0
bomb cgroup removed
```

**Taught:**

- `pids.max` is a hard ceiling: `fork` fails with `EAGAIN` once the group holds
  32 tasks; `pids.events` counts how often the limit was hit.
- The "no internal processes" rule is real: `cgroup.subtree_control` could only
  be written after the scope's own processes were moved into a leaf group.
- `cgroup.kill` (kernel 5.14+) kills every process in the group in one write,
  with no pid races; `cgroup.events` shows `populated 0` once they are gone,
  and only then can the group be removed with `rmdir`.
- Delegation works without root on this machine.
