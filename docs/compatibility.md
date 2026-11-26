# What stays compatible

*From version 1.0 on. Decision: D-126. Tests: `tests/compat.rs`.*

Norn writes two kinds of file that outlive the program that wrote them: the **policy file** you write, and the **execution folder** a run leaves (its event log, checkpoints, blobs and patch). This page says what a later 1.x version promises about them, and what it does not.

## The promise

**1. A policy file that 1.0 accepts is accepted by every 1.x, and means the same.**

- The file says `policy_version = 1`. Every 1.x reads version 1.
- New settings are only ever added as optional, with a default that changes nothing for a file that does not use them. (That is how every setting since v0.1 was added: `skip`, `scratch`, `deny_except`, `ro_dirs`, `helpers`, `[sandbox.env]`, `[external]`, `limits.workspace_mb`, and in 1.2 `[network]`.)
- No setting is removed or given another meaning within version 1. If that ever has to happen, the file format becomes `policy_version = 2`, and a 1.x program refuses a version 2 file by name instead of guessing.

**2. An execution folder written by 1.0 or a later 1.x can be used by every later 1.x**, with every command: `status`, `trace`, `recover`, `run --resume`, `apply`, `rm`.

- The event log records `schema_version = 1`. New kinds of event and new fields in an event may be added; existing ones keep their names and meaning. A log of another schema version is refused by name.
- This holds for an interrupted execution too: one that an older version left with a call cut off is recovered by a newer version by the same rules (`docs/recovery-semantics.md`).

**3. What v0.1 wrote still works**, although v0.1 came before this promise. It is tested: the presets as v0.1 shipped them, a finished v0.1 execution and one that v0.1 left interrupted are frozen in `tests/fixtures/compat/` and are read, recovered, resumed and applied by the test suite.

## Not promised

- **The other direction.** An *older* program reading a *newer* file. A 1.0 program refuses a policy that uses a setting added in 1.1 (an unknown key is always an error, so that a misspelt `deny` never silently protects nothing), and may not understand a newer event.
- **Resuming on another machine.** An execution folder holds absolute paths from the machine it ran on (the repository, the sandbox's read-only folders). `trace` and `apply --to` work anywhere; `run --resume` needs the same paths to exist.
- **The words on the screen.** The text of `trace`, `status` and error messages may change. Programs should read the event log, not the screen.
- **The Rust library.** `norn` is built as a program; the functions inside it are not a public interface.
- **The inside of a checkpoint or blob** beyond what the commands above need, and the name or layout of temporary files.
- **Tool calls offered to a model.** The names and arguments of the tools in a script (`fs.read`, `fs.write`, `fs.edit`, `fs.list`, `fs.search`, `fs.delete`, `exec`, `ext.call`; over MCP with `_` in place of `.`) follow rule 1 (new optional arguments only). The exact wording of the tool descriptions and of the system prompt sent to a model may change.

## How it is kept

- `tests/compat.rs` runs with every `make test`, on both presets. Its fixtures are never regenerated with newer code; a release adds its own.
- A change that would break one of those tests is a change of format version, with a decision-log entry.
