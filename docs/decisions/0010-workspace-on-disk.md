# ADR 0010 — The workspace is on disk, with no hard size limit

**Status:** accepted · **Log entries:** D-010

## Question
Without root, the only easy way to cap a folder's size is a tmpfs. Put the workspace on a size-capped tmpfs, or on the normal disk?

## Decision
On disk, under the execution folder. No size cap in v1.

## Why
A tmpfs mounted inside the sandbox's mount namespace disappears when the last process in that namespace dies. The whole point of the project is recovery after the runtime is killed; a workspace that vanishes with it would turn every recovery into "restore the last checkpoint" and the `fs.write` hash check would never have anything to inspect.

## Cost
A command can fill the disk. This is stated as a known limit in the README and the threat model. The sandbox's `/tmp` *is* a tmpfs and is bounded by the cgroup's memory limit.

## Look again if
Disk filling matters in practice. Options: filesystem quotas (need root or a project-quota setup), or a loop-mounted image file per execution.
