# ADR 0011 — The sandbox's system folders come from the policy

**Status:** accepted · **Log entries:** D-012, D-044

## Question
A sandboxed command needs to see where programs and libraries live. That differs between Linux systems (`/usr`, `/lib` on most; `/nix/store` on NixOS). Where is that list?

## Decision
In the policy file (`[sandbox].ro_mounts`), with two ready-made presets: `policies/fhs.toml` (`/usr`, `/bin`, `/lib`, `/lib64`) and `policies/nix.toml` (`/nix/store`). The runtime resolves each allowed program to its real path before starting and refuses to run if that path is not under one of the listed folders.

## Why
The first design hardcoded `/usr` and `/lib`, which do not hold the programs on the development machine. Distro layout is configuration, not code: nothing in `src/` names a distro.

## Details that matter
A listed folder that is a symlink on the host (`/bin -> usr/bin`) is recreated as a symlink; one that does not exist is skipped. With the wrong preset the runtime stops with a message that names the program's real path and the two preset files.

## Status of the claim
The `nix` preset passes all suites on the development machine. The `fhs` preset passes them on Ubuntu 24.04 in CI: <https://github.com/pessimistic-zer0/agentos/actions/runs/43385661105> (2026-11-13, D-072). At tag `v0.1` this section said the `fhs` preset had not been run.
