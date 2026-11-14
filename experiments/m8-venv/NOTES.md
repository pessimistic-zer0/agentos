# m8-venv: can a sandboxed command use a Python virtualenv?

Run by hand on 2026-11-14 (NixOS, `nix` preset, the build of commit `67796a2`), before D-091 and D-092.
No program is kept here: the experiment was four runs of the existing binary with a scripted agent.
The same set-up is now `tests/deps.rs`.

## Set-up

- `python3 -m venv --without-pip --system-site-packages <venv>`
- one package written by hand: `<venv>/lib/python3.13/site-packages/fakedep.py`
- a launcher like the ones `pip` writes: `<venv>/bin/pytest` starting with `#!<venv>/bin/python3`
- a test in the repo that does `import fakedep`
- a copy of `policies/nix.toml` with `<venv>` added to `sandbox.ro_mounts`
- `agentos` started with `<venv>/bin` first on `PATH`
- two steps: `pytest -q …` and `python3 -m pytest -q …`

## What happened

| Run | Result |
|---|---|
| A virtualenv inside the repo, not on `PATH`, not mounted | `pytest` exit 2: the virtualenv is not used at all |
| Virtualenv under `/tmp`, mounted | Both steps: `sandbox: setup failed: File exists (os error 17)`. The sandbox makes its own `/tmp` after the mounts and trips over the folder the mount created |
| Virtualenv under the home folder, mounted | Both steps pass |
| The same, printing `sys.executable` | The `python3` step was started as the *system* Python (`/nix/store/…/python3.13`; the runtime had resolved the virtualenv's link), yet it ran inside the virtualenv |

## What was learned

1. **Mounting a folder read-only is enough for packages to be found.** No new mechanism was needed for the virtualenv case.
2. **The `python3` step worked by luck.** Python finds itself by looking up its own name (`argv[0]`, here `python3`) on `PATH`, and the virtualenv's `bin` was on the sandbox's `PATH` only because the *other* allowed program, `pytest`, had been found there. With a policy that allows only `python3`, the virtualenv would have been ignored. Hence D-091: start the program through the path it was found at.
3. **A mount under `/tmp`, `/proc`, `/dev` or `/work` cannot work**, and the error did not say why. Hence the check in D-092.
4. **Packages that are not in a virtualenv need a variable** (`PYTHONPATH`, `NODE_PATH`, …), and the policy had no way to set one. Hence `[sandbox.env]` in D-092.
