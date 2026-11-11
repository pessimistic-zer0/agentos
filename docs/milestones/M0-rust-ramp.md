# M0 — Rust ramp

| | |
|---|---|
| Weeks in the plan | 0 |
| Started / finished | 2026-11-11 / 2026-11-11 |
| Commits | `76a12b5` … the commit that adds this file (tag `m0`) |
| Status | Done |

Write this document for a reader who is new to Rust and Linux internals. Plain words first, the real term next to it.

## 1. Aim

Before building anything real, get a working Rust setup and write one small program that uses the libraries the runtime will depend on. The program reads a TOML file (the same file format as the policy) and stores rows in SQLite (the same database as the event log). It touches none of the five parts directly; it is practice for the policy and the event log.

## 2. Requirements

Exit criterion from the v1 table: *"A small CLI that reads a TOML file and writes rows to SQLite."*

| Requirement | Met? | Evidence (command and result) |
|---|---|---|
| A working Rust toolchain | Yes | `nix develop -c rustc --version` → `rustc 1.95.0` |
| The program reads a TOML file | Yes | `cargo run -- --db /tmp/ramp.db load calls.toml` → `stored 3 calls from batch `ramp-example`` |
| It writes rows to SQLite | Yes | Running `load` twice then `list` prints six rows with ids 1–6 |
| A bad file is refused with a clear message | Yes | `load bad.toml` → `Error: call 0: tool `fs.read` needs a `path` field`, exit code 1 |
| It has tests | Yes | `cargo test` in `experiments/m0-ramp` → `5 passed; 0 failed` |
| It passes the linter | Yes | `cargo clippy --all-targets` → no warnings |

All commands run from `experiments/m0-ramp` inside `nix develop`. Nothing is unmet.

## 3. What you need to know first

**Crate, package, `Cargo.toml`.** A *crate* is one Rust library or program. `Cargo.toml` lists its name and the crates it depends on; `cargo` downloads them, builds everything and runs the tests. `Cargo.lock` records the exact versions used, so the next build uses the same ones.

**Ownership and borrowing.** Every value in Rust has exactly one owner. When the owner goes away, the value is freed. You can *move* a value to a new owner (the old name can no longer be used), or *borrow* it: `&x` is a read-only borrow, `&mut x` is a borrow that may change it. While a `&mut` borrow exists, nothing else may use the value. The compiler checks all of this, which is how Rust avoids use-after-free bugs without a garbage collector.

**`Result` and `?`.** A function that can fail returns `Result<T, E>`: either `Ok(value)` or `Err(error)`. Writing `?` after a call means "if this is an error, return it from my function now; otherwise give me the value". There are no exceptions.

**`thiserror` and `anyhow`.** Two ways to make error types. `thiserror` builds an enum of specific errors that a caller can `match` on ("missing field", "unknown tool"). `anyhow` gives one general error type that carries a chain of messages ("reading nope.toml" → "No such file or directory"); it is for the top of the program, where the error is only printed. Project rule: `thiserror` where callers match, `anyhow` at the top.

**serde.** A library that turns data formats (TOML, JSON) into Rust structs and back. `#[derive(Deserialize)]` on a struct writes the conversion code for you.

**SQLite, WAL, `synchronous=FULL`.** SQLite is a database stored in one file. In WAL mode (write-ahead log) a change is first appended to a side file, which keeps old data safe if the program dies mid-write. `synchronous=FULL` makes every commit wait until the data is really on disk (`fsync`). See the glossary in `docs/decision-log.md` for WAL and `fsync`.

**Transaction.** A group of database changes that either all happen or none do.

## 4. Experiments

| Experiment | What it tried | What happened | What it taught |
|---|---|---|---|
| `experiments/m0-ramp` | The exit criterion itself | Works; see `experiments/m0-ramp/NOTES.md` | Each crate the runtime needs builds and behaves as expected |
| Dev shell on latest `nixos-unstable` | Getting Rust, pytest and hyperfine through Nix | Stalled for 22 minutes downloading a whole new compiler set at about 30 KB/s | Pin nixpkgs to the commit already on the machine (D-029) |

The v1 table lists no experiments for week 0.

## 5. What was built

| File | Purpose |
|---|---|
| `flake.nix`, `flake.lock` | The dev shell: Rust 1.95 (cargo, rustc, rustfmt, clippy), Python 3.13 with pytest 9.0.3, hyperfine, diffutils, make |
| `.gitignore` | Keeps `target/` build output and the personal `.tex` file out of git |
| `experiments/README.md` | Index of experiments |
| `experiments/m0-ramp/Cargo.toml` | The practice crate and its six dependencies |
| `experiments/m0-ramp/src/main.rs` | The whole program and its tests |
| `experiments/m0-ramp/calls.toml`, `bad.toml` | A good and a bad input file |
| `experiments/m0-ramp/NOTES.md` | What the experiment showed |

The program has one path through it:

```text
calls.toml --(toml + serde)--> RawFile --(Call::try_from_raw)--> Vec<Call> --(rusqlite, one transaction)--> calls table
```

## 6. The code, explained

All in `experiments/m0-ramp/src/main.rs`, in reading order.

### 6.1 The file format: `RawFile`, `RawCall`

```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawFile {
    name: String,
    #[serde(rename = "call", default)]
    calls: Vec<RawCall>,
}

struct RawCall {
    tool: String,
    path: Option<String>,
    argv: Option<Vec<String>>,
}
```

- `#[derive(Debug, Deserialize)]` asks the compiler to write two things for this struct: a way to print it for debugging, and serde's code to fill it from TOML. *A derive is a code generator attached to a type.*
- `deny_unknown_fields` makes a typo such as `pth = "a"` an error. Without it serde ignores unknown keys, and a policy file with a typo would silently mean something else. The real policy uses the same attribute.
- `rename = "call"`: in TOML each entry is written `[[call]]`; in Rust the list is called `calls`.
- `Option<String>` means "a string, or nothing". TOML lets the user leave `path` out, so the raw struct must allow that.

### 6.2 The strict shape: `enum Call`

```rust
enum Call {
    FsRead { path: String },
    FsList { path: String },
    Exec { argv: Vec<String> },
}
```

A Rust `enum` is a value that is exactly one of several variants, and each variant can carry its own data. An `Exec` *always* has an argv and an `FsRead` *always* has a path, so code that holds a `Call` never checks for a missing field. This is the idea "make wrong states impossible to represent", and the real `ToolCall` type in M1 is built the same way.

`match` is how you look inside an enum. The compiler refuses a `match` that forgets a variant:

```rust
fn tool_name(&self) -> &'static str {
    match self {
        Call::FsRead { .. } => "fs.read",
        Call::FsList { .. } => "fs.list",
        Call::Exec { .. } => "exec",
    }
}
```

`&self` means the method borrows the value read-only. `&'static str` is a borrowed string that lives for the whole program (a string literal baked into the binary).

### 6.3 Checking: `Call::try_from_raw` and `CallError`

```rust
#[derive(Debug, thiserror::Error, PartialEq)]
enum CallError {
    #[error("call {index}: unknown tool `{tool}` (expected fs.read, fs.list or exec)")]
    UnknownTool { index: usize, tool: String },
    #[error("call {index}: tool `{tool}` needs a `{field}` field")]
    MissingField { index: usize, tool: String, field: &'static str },
    ...
}
```

`thiserror` turns each `#[error("...")]` into the text printed for that variant. Because it is an enum, a test can check *which* error happened (`assert_eq!(err, CallError::MissingField { .. })`), not just that some text appeared.

```rust
fn try_from_raw(index: usize, raw: RawCall) -> Result<Call, CallError> {
    let missing = |field| CallError::MissingField { index, tool: raw.tool.clone(), field };
    match raw.tool.as_str() {
        "fs.read" => Ok(Call::FsRead { path: raw.path.clone().ok_or_else(|| missing("path"))? }),
        ...
        other => Err(CallError::UnknownTool { index, tool: other.to_string() }),
    }
}
```

- `|field| ...` is a *closure*: a small unnamed function that can use variables around it (`index`, `raw`).
- `raw.path.clone().ok_or_else(|| missing("path"))?` reads as: take the optional path; if it is missing, make a `MissingField` error; `?` returns that error from the function.
- `raw: RawCall` (no `&`) means the function takes ownership of the raw call; the caller cannot use it afterwards.

### 6.4 Loading the file: `load_calls`

```rust
fn load_calls(path: &Path) -> Result<(String, Vec<Call>)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let raw: RawFile = toml::from_str(&text)
        .with_context(|| format!("parsing {} as TOML", path.display()))?;
    let calls = raw.calls.into_iter().enumerate()
        .map(|(i, c)| Call::try_from_raw(i, c))
        .collect::<Result<Vec<_>, _>>()?;
    Ok((raw.name, calls))
}
```

- The return type here is `anyhow::Result`: main only prints the error, it does not match on it.
- `.with_context(...)` adds a line to the error chain. That is why a missing file prints `reading nope.toml` followed by `Caused by: No such file or directory`.
- `into_iter().enumerate().map(...)` walks the list with an index. `collect::<Result<Vec<_>, _>>()` gathers the results into one `Result`: all good gives `Ok(list)`, and the first error stops it. `?` then returns that error. The `CallError` turns into an `anyhow::Error` automatically.

### 6.5 Storing rows: `open_db` and `store_calls`

```rust
conn.pragma_update(None, "journal_mode", "WAL")?;
conn.pragma_update(None, "synchronous", "FULL")?;
```

These two settings are what the event log uses from M1 on (D-005).

```rust
fn store_calls(conn: &mut Connection, batch: &str, calls: &[Call]) -> Result<usize> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("INSERT INTO calls (batch, tool, detail) VALUES (?1, ?2, ?3)")?;
        for call in calls {
            stmt.execute(params![batch, call.tool_name(), call.detail()])?;
        }
    }
    tx.commit()?;
    Ok(calls.len())
}
```

- `&mut Connection`: a transaction needs the connection to itself, so the function asks for an exclusive borrow.
- `?1, ?2, ?3` are *placeholders*. The values are passed separately with `params!`, never pasted into the SQL text. Pasting them in would allow SQL injection; the same rule as "never build a shell string" for commands (design rule 8).
- The inner `{ ... }` block exists for the borrow checker: `stmt` borrows `tx`, and `tx.commit()` needs to take `tx` by value. Ending the block ends the borrow. Without the block, the code does not compile. This is the first place most people meet the borrow checker.
- If any insert fails, `?` returns early, `tx` is dropped without `commit()`, and SQLite rolls the whole batch back.

### 6.6 The command line: `Cli` and `main`

```rust
#[derive(Parser)]
struct Cli {
    #[arg(long, default_value = "ramp.db")]
    db: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}
```

`clap`'s derive turns the struct into a full argument parser with `--help`. `fn main() -> Result<()>` lets `main` use `?`; if it returns an error, Rust prints it and exits with code 1.

### 6.7 Tests

`#[cfg(test)] mod tests { ... }` is compiled only for `cargo test`. Each `#[test]` function checks one rule. A test that returns `Result` can use `?`; an error fails the test.

## 7. Decisions made

| Entry | Decision | Discarded alternatives |
|---|---|---|
| D-028 | The resume `.tex` file in the folder is not committed; `/*.tex` is ignored | Commit it; move or delete it |
| D-029 | Tools come from `flake.nix`, pinned to the nixpkgs commit the machine runs | rustup + pip; `rust-toolchain.toml`; latest `nixos-unstable` |
| D-030 | The week-0 program is its own crate under `experiments/` | A subcommand of `agentos`; starting the real crate early |
| D-031 | Newest version of each stack crate, locked by `Cargo.lock` | Versions already in the local cache |

## 8. Problems hit

1. **Broken Rust toolchain.** Expected `cargo` and `rustc` to work. `rustup show` said "no active toolchain", and the only installed toolchain (1.87) had no `rustc`. Found by running `rustc --version`. Fixed by taking Rust from the Nix dev shell instead (D-029). Commit `1408a05`.
2. **Slow download.** Expected `nix develop` on the latest `nixos-unstable` to take a few minutes. It ran for 22 minutes, still fetching a new gcc, LLVM and Python. `curl` measured about 30 KB/s from the Nix cache. Fixed by pinning nixpkgs to the commit already on the machine, which needed only a few MB. Commit `1408a05`.
3. **Noisy errors.** Expected `load bad.toml` to print one line. It printed the line and a 12-frame backtrace, because the dev shell set `RUST_BACKTRACE=1` and `anyhow` records a backtrace whenever that is set. Removed the variable from `flake.nix`. Commit `1408a05`.

All three are in the "Findings" table of the decision log.

## 9. How to run it

From a fresh clone, on a machine with Nix and flakes:

```sh
nix develop            # first time: downloads pytest, hyperfine, clippy
cd experiments/m0-ramp
cargo test             # expect: test result: ok. 5 passed; 0 failed
cargo run -- --db /tmp/ramp.db load calls.toml   # stored 3 calls from batch `ramp-example`
cargo run -- --db /tmp/ramp.db list              # three rows: fs.list, fs.read, exec
cargo run -- --db /tmp/ramp.db load bad.toml     # Error: call 0: tool `fs.read` needs a `path` field
```

Without Nix: install Rust with rustup (`rustup default stable`), then the same `cargo` commands.

## 10. What this milestone does not do

- No `agentos` crate yet. It starts in M1.
- No sandbox, no policy logic, no event model. M1 builds the policy, gateway and event log; M2 the sandbox.
- Threads (Rust book chapter 16) are not used here. They first appear in M1, where the `exec` watchdog and the output readers run on their own threads.

## 11. Check your understanding

1. What is the difference between `String` and `&str`, and why does `tool_name` return `&'static str`?
2. What does `?` do, and what has to be true about the error types for it to compile?
3. Why are there two structs (`RawCall` and `Call`) instead of one?
4. What would happen if `deny_unknown_fields` were removed and a user wrote `pth = "a"`?
5. When would you use `thiserror`, and when `anyhow`?
6. Why does `store_calls` take `&mut Connection` and not `&Connection`?
7. Why is there an extra `{ }` block around `stmt` in `store_calls`? What error does the compiler give without it?
8. If the second of three inserts fails, how many rows are in the table afterwards, and why?
9. Why are values passed with `?1` placeholders instead of being formatted into the SQL string?
10. What do `journal_mode=WAL` and `synchronous=FULL` each protect against? Which of the two does a `kill -9` test actually exercise?
11. Why was nixpkgs pinned to an old commit instead of the newest one, and what makes either choice reproducible?
