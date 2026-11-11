//! m0-ramp: read a TOML file of pretend tool calls and store them as rows in SQLite.
//!
//! This is the week-0 practice program. Each piece maps to something the real
//! runtime needs later:
//! - `serde` + `toml`   -> reading the policy file (M1)
//! - `thiserror`        -> errors that callers can match on (policy errors, M1)
//! - `anyhow`           -> errors at the top level, with context added on the way up
//! - `rusqlite`         -> the event log (M1), with a transaction per batch
//! - `clap`             -> the `agentos` command line

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use rusqlite::{params, Connection};
use serde::Deserialize;

// ---------------------------------------------------------------------------
// 1. The file format, exactly as it appears in TOML.
//
// `RawFile` and `RawCall` mirror the file one-to-one. Fields that only some
// tools use are `Option`, because TOML lets the user leave them out. Turning
// this loose shape into a strict one is a separate step (`Call::try_from`), so
// the checking rules live in one place and can be tested on their own.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)] // a typo such as `pth = "..."` is an error, not silently ignored
struct RawFile {
    name: String,
    #[serde(rename = "call", default)]
    calls: Vec<RawCall>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCall {
    tool: String,
    path: Option<String>,
    argv: Option<Vec<String>>,
}

// ---------------------------------------------------------------------------
// 2. The strict shape the rest of the program uses.
//
// An enum makes wrong states impossible to represent: an `Exec` always has an
// argv, an `FsRead` always has a path. Code that receives a `Call` never has
// to check for a missing field again.
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Call {
    FsRead { path: String },
    FsList { path: String },
    Exec { argv: Vec<String> },
}

impl Call {
    fn tool_name(&self) -> &'static str {
        match self {
            Call::FsRead { .. } => "fs.read",
            Call::FsList { .. } => "fs.list",
            Call::Exec { .. } => "exec",
        }
    }

    /// The tool-specific part, as one string for the `detail` column.
    fn detail(&self) -> String {
        match self {
            Call::FsRead { path } | Call::FsList { path } => path.clone(),
            Call::Exec { argv } => argv.join(" "),
        }
    }
}

/// Errors in the *content* of the file. These are the errors a caller may want
/// to tell apart (for example to print "line 3 is wrong" in a UI), so they are
/// a `thiserror` enum rather than a plain string.
#[derive(Debug, thiserror::Error, PartialEq)]
enum CallError {
    #[error("call {index}: unknown tool `{tool}` (expected fs.read, fs.list or exec)")]
    UnknownTool { index: usize, tool: String },
    #[error("call {index}: tool `{tool}` needs a `{field}` field")]
    MissingField { index: usize, tool: String, field: &'static str },
    #[error("call {index}: `argv` must not be empty")]
    EmptyArgv { index: usize },
}

impl Call {
    /// Check one raw call and turn it into a strict `Call`.
    ///
    /// It takes `raw` by value (ownership moves in), so the `String`s inside
    /// can be moved into the result instead of copied.
    fn try_from_raw(index: usize, raw: RawCall) -> Result<Call, CallError> {
        let missing = |field| CallError::MissingField { index, tool: raw.tool.clone(), field };
        match raw.tool.as_str() {
            "fs.read" => Ok(Call::FsRead { path: raw.path.clone().ok_or_else(|| missing("path"))? }),
            "fs.list" => Ok(Call::FsList { path: raw.path.clone().ok_or_else(|| missing("path"))? }),
            "exec" => {
                let argv = raw.argv.clone().ok_or_else(|| missing("argv"))?;
                if argv.is_empty() {
                    return Err(CallError::EmptyArgv { index });
                }
                Ok(Call::Exec { argv })
            }
            other => Err(CallError::UnknownTool { index, tool: other.to_string() }),
        }
    }
}

/// Read and check a whole file. `anyhow::Result` is used here because the
/// caller (main) only reports the error; it does not match on it.
fn load_calls(path: &Path) -> Result<(String, Vec<Call>)> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading {}", path.display()))?;
    let raw: RawFile = toml::from_str(&text)
        .with_context(|| format!("parsing {} as TOML", path.display()))?;
    let calls = raw
        .calls
        .into_iter()
        .enumerate()
        .map(|(i, c)| Call::try_from_raw(i, c))
        .collect::<Result<Vec<_>, _>>()?; // stops at the first bad call
    Ok((raw.name, calls))
}

// ---------------------------------------------------------------------------
// 3. Storage.
// ---------------------------------------------------------------------------

fn open_db(path: &Path) -> Result<Connection> {
    let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
    // WAL mode and synchronous=FULL are what the real event log will use; trying
    // them here shows they are one line each.
    conn.pragma_update(None, "journal_mode", "WAL")?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS calls (
             id     INTEGER PRIMARY KEY,
             batch  TEXT NOT NULL,
             tool   TEXT NOT NULL,
             detail TEXT NOT NULL
         );",
    )?;
    Ok(conn)
}

/// Insert all calls in one transaction: either every row is stored or none is.
/// `conn` is borrowed mutably (`&mut`) because a transaction needs exclusive use
/// of the connection while it is open.
fn store_calls(conn: &mut Connection, batch: &str, calls: &[Call]) -> Result<usize> {
    let tx = conn.transaction()?;
    {
        let mut stmt = tx.prepare("INSERT INTO calls (batch, tool, detail) VALUES (?1, ?2, ?3)")?;
        for call in calls {
            stmt.execute(params![batch, call.tool_name(), call.detail()])?;
        }
    } // `stmt` borrows `tx`; this block ends the borrow so `tx` can be moved into commit()
    tx.commit()?;
    Ok(calls.len())
}

fn list_rows(conn: &Connection) -> Result<Vec<(i64, String, String, String)>> {
    let mut stmt = conn.prepare("SELECT id, batch, tool, detail FROM calls ORDER BY id")?;
    let rows = stmt
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows)
}

// ---------------------------------------------------------------------------
// 4. Command line.
// ---------------------------------------------------------------------------

#[derive(Parser)]
#[command(about = "Week-0 practice: TOML in, SQLite rows out")]
struct Cli {
    /// SQLite database file
    #[arg(long, default_value = "ramp.db")]
    db: PathBuf,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Read a TOML file of calls and store them
    Load { file: PathBuf },
    /// Print every stored row
    List,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let mut conn = open_db(&cli.db)?;
    match cli.cmd {
        Cmd::Load { file } => {
            let (name, calls) = load_calls(&file)?;
            let n = store_calls(&mut conn, &name, &calls)?;
            println!("stored {n} calls from batch `{name}`");
        }
        Cmd::List => {
            for (id, batch, tool, detail) in list_rows(&conn)? {
                println!("{id:>3}  {batch:<14} {tool:<8} {detail}");
            }
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. Tests. `cargo test` runs them. Each one checks one rule.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(tool: &str, path: Option<&str>, argv: Option<&[&str]>) -> RawCall {
        RawCall {
            tool: tool.into(),
            path: path.map(String::from),
            argv: argv.map(|a| a.iter().map(|s| s.to_string()).collect()),
        }
    }

    #[test]
    fn valid_calls_convert() {
        assert_eq!(
            Call::try_from_raw(0, raw("fs.read", Some("a.py"), None)),
            Ok(Call::FsRead { path: "a.py".into() })
        );
        assert_eq!(
            Call::try_from_raw(1, raw("exec", None, Some(&["pytest", "-q"]))),
            Ok(Call::Exec { argv: vec!["pytest".into(), "-q".into()] })
        );
    }

    #[test]
    fn missing_field_is_reported_with_its_index() {
        let err = Call::try_from_raw(3, raw("fs.read", None, None)).unwrap_err();
        assert_eq!(err, CallError::MissingField { index: 3, tool: "fs.read".into(), field: "path" });
    }

    #[test]
    fn unknown_tool_and_empty_argv_are_rejected() {
        assert!(matches!(
            Call::try_from_raw(0, raw("rm", None, None)),
            Err(CallError::UnknownTool { .. })
        ));
        assert_eq!(
            Call::try_from_raw(0, raw("exec", None, Some(&[]))),
            Err(CallError::EmptyArgv { index: 0 })
        );
    }

    #[test]
    fn unknown_toml_keys_are_rejected() {
        let err = toml::from_str::<RawFile>("name = \"x\"\n[[call]]\ntool = \"fs.read\"\npth = \"a\"\n");
        assert!(err.is_err());
    }

    #[test]
    fn rows_round_trip_through_sqlite() -> Result<()> {
        let dir = std::env::temp_dir().join(format!("m0-ramp-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let db = dir.join("t.db");
        let _ = std::fs::remove_file(&db);
        let mut conn = open_db(&db)?;
        let calls = vec![
            Call::FsList { path: ".".into() },
            Call::Exec { argv: vec!["pytest".into()] },
        ];
        assert_eq!(store_calls(&mut conn, "b1", &calls)?, 2);
        let rows = list_rows(&conn)?;
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].2, "exec");
        assert_eq!(rows[1].3, "pytest");
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }
}
