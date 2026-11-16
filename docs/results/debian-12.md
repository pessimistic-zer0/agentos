# The test suite on Debian 12 (`fhs` preset)

Date: 2026-11-16. Code: commit `70171f4` plus the uncommitted script `ci/debian-vm.sh` (no source change). One run.

**Where it ran.** In a virtual machine on the development machine (QEMU 10.2.4 with KVM, 4 CPUs, 6 GB), started from Debian's own cloud image `debian-12-genericcloud-amd64.qcow2`. Not in CI yet: the CI job `debian` runs the same script and has not run. So this is "passes on Debian 12 in a VM on one machine", not "verified in CI".

**The system, as it reported itself:**

| | |
|---|---|
| Distribution | Debian GNU/Linux 12 (bookworm) |
| Kernel | 6.1.0-54-cloud-amd64 |
| Security modules (`/sys/kernel/security/lsm`) | lockdown, capability, landlock, yama, apparmor, tomoyo, bpf |
| cgroups | cgroup2fs (v2) |
| Rust | 1.100.0, installed with rustup (Debian 12's own is too old) |
| Other packages | `build-essential python3-pytest python3-venv diffutils curl ca-certificates`, from Debian |

**What was run:** `AGENTOS_TEST_PRESET=fhs make test`, as the ordinary user `ci` in an SSH session (which gives a systemd user session), after `loginctl enable-linger`. Nothing else was changed on the system: no sysctl, no AppArmor profile.

**Result:** 167 passed, 0 failed, the same count as on NixOS at that commit.

| Suite | Passed |
|---|---|
| unit tests | 36 |
| `apply` | 6 |
| `benign` | 1 |
| `binary` | 4 |
| `compat` | 4 |
| `deps` | 11 |
| `edit` | 4 |
| `external` | 14 |
| `hardening` | 26 |
| `llm_agent` | 12 |
| `recovery` (3 kills per cell) | 5 |
| `rodirs` | 7 |
| `security` | 33 |
| `vertical_slice` | 4 |

**What it adds.** A third distribution, and an older kernel: 6.1 against 6.8 on the Ubuntu runner and 7.1 on the development machine. Landlock on 6.1 is two versions behind what the code is written for; the sandbox asks for what the kernel has (best effort) and the security suite passed. Debian needed none of Ubuntu's extra steps for user namespaces.

**What it does not show.** The 200-kill suite, the benchmarks and `make demo` were not run on Debian. One run on one machine.

To repeat: `ci/debian-vm.sh` (needs `qemu-system-x86_64`, `qemu-img`, `/dev/kvm`; downloads about 350 MB the first time).
