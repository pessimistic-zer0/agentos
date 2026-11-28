# `norn trace --otel` sent to a real OpenTelemetry collector, `nix` preset

*2026-11-28, development machine (NixOS, kernel 7.1.8), norn at the M26 working tree (after `d68aed0`), debug build. Plan item X1 of `docs/v4-scope.md`, and the done criterion "the OTLP file is accepted by a collector" of its §7.*

## Setup

- The OpenTelemetry Collector 0.151.0 from `nixpkgs` (`nix build nixpkgs#opentelemetry-collector`), with the configuration of experiment E4, `experiments/m26-otlp/collector.yaml`: an OTLP/HTTP receiver on `127.0.0.1:4318` and the `debug` exporter at `detailed`, which prints every span it receives.
- Four executions: the three frozen 1.2.0 executions of `tests/fixtures/compat/v1.2-executions.tar` (`v12-network`, `v12-recovered`, `v12-approval`) and a new scripted run of the demo task.
- For each: `norn trace <id> --otel > run.json`, then `curl -H 'Content-Type: application/json' --data-binary @run.json http://127.0.0.1:4318/v1/traces`.

## Result

| Execution | Spans in the file | Collector's answer |
|---|---|---|
| `v12-network` | 6 | 200 |
| `v12-recovered` | 10 | 200 |
| `v12-approval` | 6 | 200 |
| new scripted run | 9 | 200 `{"partialSuccess":{}}` |

The collector printed 31 spans, the sum of the four files. In its output, each part span (`start`, `recovery after a stop`) has the execution span as its `Parent ID`, and each call span has its part. The denied `fs.read` of `.env` has `Status code: Error`. The `fs.delete` that used an approval carries a `SpanLink` to the span of the call that waited for it, and the attributes `norn.summary: Str(deleted (8 bytes))` and `norn.checkpoint: Str(cp-0002)`.

## What it does not show

- That a viewer such as Jaeger or Grafana Tempo draws it well: none was run (E4: Jaeger is not in `nixpkgs`).
- Anything on `fhs`: the file is the same text on any system; the collector was run only here.
- The test suite does not run a collector (it is not in CI). It checks the rules the collector does not: one trace id, hex ids, unique span ids, every parent and link inside the file (`check_otlp` in `tests/trace.rs`).
