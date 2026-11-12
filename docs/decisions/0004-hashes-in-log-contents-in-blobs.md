# ADR 0004 — Hashes in the log, contents in a blob store

**Status:** accepted · **Log entries:** D-016, D-033

## Question
Should the event log hold file contents and tool outputs, or only their hashes?

## Decision
The log holds SHA-256 hashes. The bytes (new file contents, full tool outputs) are files in `blobs/`, named by their hash, written with temp file + fsync + rename before the log record that mentions them.

## Why
- The `fs.write` intent needs the hash of the file before and after, so recovery can observe which side of the rename a crash fell on. It does not need the bytes.
- The log stays small and quick to scan. A 52 MB output flood grew the log by less than 2 MB (test `output_flood_is_truncated_and_does_not_fill_the_log`).
- A blob with a wrong hash is detectably corrupt.

## Cost
One more fsync per call that produces a blob. Blobs are never garbage-collected in v1.

## Look again if
Retention or privacy rules are needed (for example, deleting file contents but keeping the audit trail). The split already allows that.
