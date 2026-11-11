# m0-ramp

**Tried:** the week-0 exit criterion, "a small CLI that reads a TOML file and
writes rows to SQLite", using the same crates the runtime will use.

**Run:**

```sh
nix develop -c sh -c 'cd experiments/m0-ramp && cargo test && cargo run -- --db /tmp/ramp.db load calls.toml && cargo run -- --db /tmp/ramp.db list'
```

**Showed:**

- 5 unit tests pass (valid calls convert; a missing field names its index; an
  unknown tool and an empty argv are refused; an unknown TOML key such as
  `pth` is refused; rows survive a round trip through SQLite).
- `load calls.toml` twice, then `list`, prints six rows with ids 1 to 6.
- `load bad.toml` prints `Error: call 0: tool `fs.read` needs a `path` field`
  and exits 1. `load nope.toml` prints the context chain
  `reading nope.toml` / `Caused by: No such file or directory`.
- WAL mode and `synchronous=FULL` are one `pragma_update` call each.
- With `RUST_BACKTRACE=1` set, every error also printed a long backtrace,
  which hid the message. The dev shell no longer sets it.
