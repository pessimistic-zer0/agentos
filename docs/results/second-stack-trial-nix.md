# Trial on two projects that are not Python (`nix` preset)

Date: 2026-11-15. Code: commit `2a6a3d1` plus the helper list of D-112 (found and built during this trial). NixOS 26.05, kernel 7.1.8, btrfs, debug build.

Everything real before this was Python with `pytest`. This trial asks whether the design is tied to that. Two small projects were written for it by Claude, in `~/Temp/AgentOS_temp/` (outside this repository), each an ordinary project with seeded failing tests and a short `TASKS.md`. Like the first trial it is one session by hand, a record and not a benchmark; unlike the first, the projects were made by the same agent that built AgentOS, so they are less of a surprise test.

## The projects

| | `shortlink-node` | `wordfreq-rs` |
|---|---|---|
| Language | JavaScript (Node, ES modules) | Rust |
| Dependencies | `nanoid`, `ms` from npm, in `node_modules` (68 KB) | none |
| Tests | Node's built-in runner, `node --test`: 6 tests, 2 failing on purpose | `cargo test`: 3 tests, 1 failing on purpose |
| Policy changes from the preset | `write = ["src/**", "test/**"]`, `allow = [["node", "--test"]]` | `write = ["src/**", "tests/**"]`, `allow = [["cargo", "test"]]`, `scratch = ["target/**", ".cargo/**"]`, `helpers = ["cc"]` |

## What happened

| # | Run | Result |
|---|---|---|
| 1 | Node: `agentos check`, then `node --test` in the sandbox | Worked at once. `node_modules` was copied with the repo (24 files), the two packages were imported, the same 2 tests failed as outside |
| 2 | Node: 10 calls by Claude as the agent: read the tickets, look into `node_modules`, fix two bugs, add a method and a new test file, run the tests | `node --test`: 7 pass. `fs.write package.json` denied (not in the write paths), `npm test` and `node -e …` denied (no rule), all as the policy says |
| 3 | Node: `agentos apply` to a fresh clone | 3 changes; 7 tests pass there |
| 4 | Node through `npm test` (a policy with `allow = [["npm", "test"]]`) | **Failed:** `npm error enoent spawn sh ENOENT`. npm starts a shell, and the shell was not on the sandbox's `PATH` |
| 5 | Rust: `cargo test --offline` | **Failed:** `error: linker cc not found` |
| 6 | Both again with `exec.helpers` (D-112): `helpers = ["sh"]` and `helpers = ["cc"]` | Both run. The seeded failures show: `npm test` exit 1, `cargo test` exit 101 |
| 7 | Rust: 6 calls as the agent: read, test, fix the sort, test | `cargo test`: exit 0. `cargo build --release` and `cc --version` denied: a helper is not a rule |
| 8 | Rust: `agentos apply` to a fresh clone | 1 change (`src/lib.rs`); 3 tests pass there. The patch holds that file only: `target/` is scratch |

## What it showed

**The design is not tied to Python.** Nothing in `src/` had to learn about Node or Rust. What had to change was policy: which commands, which paths.

**One real gap, now closed: helper programs.** The sandbox's `PATH` held only the folders of the programs the policy allows. Test commands start other programs by name (a linker, a shell). With Python this never showed, because `pytest` starts nothing. Runs 4 and 5 are the same fault. `exec.helpers` names such programs; their folders go on the `PATH`; the agent still cannot call them.

**Left as they are, and parked (spec §22):**
- **Scratch folders are still copied into every checkpoint.** After two `cargo test` calls the execution held 64 MB, nearly all of it `target/`.
- **A lock file that the test command creates** (`Cargo.lock`, when the repo has none) is reverted and reported on every run: `1 change(s) outside write globs reverted`. Correct by the rules, noisy in practice. The user's answer today is to commit the lock file or put it in the write paths.
- **No network means no downloads.** Both projects worked because their dependencies were already there (`node_modules` in the repo, no crates). A project that needs `npm install` or a crate download inside the sandbox cannot get it; the dependencies must be on disk first and reachable (in the repo, or mounted).
- **`skip` still does not suit `node_modules`**; here it was small enough to copy.
