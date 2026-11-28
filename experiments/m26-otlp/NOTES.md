# E4 (v4, M26): does a real collector accept the OTLP JSON norn would write?

**Question.** X1 writes a run as OpenTelemetry spans in the OTLP JSON format,
by hand with `serde_json`, with no OpenTelemetry library. Does a real
collector accept that JSON, keep the parent links, and say clearly when
something is wrong?

**How.** The OpenTelemetry Collector 0.151.0 from `nixpkgs` (no install,
`nix build nixpkgs#opentelemetry-collector`), with an OTLP/HTTP receiver on
`127.0.0.1:4318` and the `debug` exporter, which prints every span it gets
(`collector.yaml`). `make_trace.py` writes one execution span and two call
spans in five variants; each was sent with `curl` to `/v1/traces`, the way a
person would send the file norn writes.

```sh
otelcol --config collector.yaml &
python3 -I make_trace.py good > t.json
curl -H 'Content-Type: application/json' --data-binary @t.json http://127.0.0.1:4318/v1/traces
```

**What happened** (2026-11-28):

| Variant | Answer | What the collector printed |
|---|---|---|
| `good`: ids in lower-case hex, times as strings of nanoseconds | 200 | The three spans; both calls with `Parent ID` = the execution span; attributes (`norn.agent: Str(llm)`, `norn.call_id: Int(1)`); the denied call with status `Error` and message `denied` |
| `numbers`: times as JSON numbers | 200 | The same |
| `upper`: ids in upper-case hex | 200 | The same, printed in lower case |
| `base64`: ids in base64 (protobuf's usual JSON mapping) | **400** `ID.UnmarshalJSONIter: length mismatch` | Nothing |
| `orphan`: a parent id that is not in the file | 200 | Accepted silently, `Parent ID: ffffffffffffffff` |

**What it taught.**

- The hand-written format works: no new dependency for X1.
- Ids must be **hex**, not base64: OTLP/JSON differs from protobuf's JSON
  mapping here. Write lower-case hex; times as strings (the OTLP JSON rule
  for 64-bit numbers; numbers work too).
- The collector does not check that a parent exists. norn's own test must
  check that every `parentSpanId` names a span in the file.
- A span's status: leave it unset (`code` 0) for a call that ended normally,
  and set `code` 2 with a message for a denied, failed or uncertain call.
- Sending stays outside norn (D-169): one `curl` line does it, as above.
