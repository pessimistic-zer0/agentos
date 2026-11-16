# m12-repo-mount

**Question (D-114, B4):** `node_modules` must be at its place inside the
project, but copying it into the workspace (and into every checkpoint) is
what `skip` exists to avoid. Can the folder from the *original* repository
be mounted read-only at that place in the workspace copy, for commands only?

**Tried (2026-11-16, NixOS, kernel 7.1.8):** `bash run.sh`. In a private
user + mount namespace (`unshare -Urm`): bind-mount `repo/node_modules`
onto the empty folder `work/node_modules`, remount it read-only, then try
to change it from inside.

**Showed:**

| Attempt | Result |
|---|---|
| read a file in it | works |
| change, create, delete, chmod a file in it | `Read-only file system` |
| `rm -rf` the folder | refused (its files cannot be removed) |
| rename the folder (the mount point) | `Device or resource busy` |
| rename a parent folder | works; the mount moves with it |
| write next to it | works |
| the host's view of `work/node_modules` afterwards | still empty: the mount existed only in the private namespace |
| the original file | unchanged |

**Taught:**

- The kernel's read-only flag on the mount is enough; it does not depend
  on Landlock or on file permissions.
- On the host the workspace only ever holds an empty folder there, so
  checkpoints, the revert scan, the size measurement and the patch need no
  change: they never see the mounted files.
- A command can rename a parent of the mount point. That only moves things
  inside the workspace, and the revert after the command puts paths outside
  the write globs back.
- Not shown here, and needed in the build: undoing the mount needs
  `umount`, which the sandbox refuses twice (no capabilities; seccomp,
  D-116). The mount point must be a real folder with no symlink on the way
  when the mount is made. Files in the mounted folder that match deny rules
  would become visible, so the run must refuse to start if there are any.
