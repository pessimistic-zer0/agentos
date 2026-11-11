# m2-unshare

**Tried:** `unshare -Urmpn --fork --mount-proc sh` (user, mount, PID and network
namespaces, our uid mapped to root inside) and looked around. Script: `run.sh`.

**Showed (2026-11-11, NixOS, kernel 7.1.8):**

| Check | Result |
|---|---|
| uid inside | `0`, from `uid_map` = `0 1000 1` (inside uid 0 is outside uid 1000) |
| pid inside | `1`; only 4 processes visible in `/proc` |
| `CapEff` | `000001ffffffffff`: every capability, but only *inside* the new user namespace |
| network | only `lo` in `/proc/net/dev`; `connect` to 1.1.1.1:80 fails with "Network is unreachable" |
| `/sys/class/net` | still lists the host's `enp64s0`, `wlp63s0`: `/sys` is the host's mount, not remounted |
| `~/.ssh` | **still readable** |

**Taught:**

- A new mount namespace starts as a *copy* of the host's mounts. It hides
  nothing by itself. Files are hidden only after building a new root with just
  the allowed folders and switching to it (`pivot_root`). That is the main job
  of the sandbox code.
- "root" inside a user namespace is not root on the host; the capabilities
  only count for things the namespace owns (its mounts, its network).
- The network namespace really is empty: no route at all, so even the
  loopback is down until someone brings it up.
- Anything mounted from the host (like `/sys`) can leak host facts. The
  sandbox mounts no `/sys`.
