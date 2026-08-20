# Telemetry: logging, tracing, metrics

How the indexer's observability is wired, how to use it while developing, and how to turn
tracing on in a deployment. The implementation lives in `indexer-common/src/telemetry.rs`;
every executable calls `init_logging`, `init_tracing` and `init_metrics` at startup.

## The stack

| Concern | Crates | Output |
|---|---|---|
| Logging | `log` (kv feature) + `logforth` | JSON lines on stdout, filtered by `RUST_LOG` |
| Tracing | `fastrace` + `fastrace-opentelemetry` | OTLP gRPC to `telemetry.tracing.otlp_exporter_endpoint` |
| Metrics | `metrics` + `metrics-exporter-prometheus` | Prometheus scrape endpoint on `telemetry.metrics.address:port` |

Logs and traces are correlated in both directions by logforth: `FastraceDiagnostic` stamps the
current trace id onto every JSON log line, and `FastraceEvent` attaches log records to the
current span as span events.

## Instrumenting code

- Put `#[trace]` (from `fastrace`) on functions worth timing; it creates a child span of
  whatever span is current. Add context via properties:
  `#[trace(properties = { "block_id": "{block_id}" })]`.
- Spans need a root to attach to. Roots exist today at:
  - chain-indexer: one root span per indexed block, `get-and-index-block`
    (`chain-indexer/src/application.rs`);
  - indexer-api: one root span per subscription connection (see the
    `infra/api/v4/subscription/*.rs` modules);
  - indexer-api: one span tree per GraphQL operation, produced by
    `async_graphql::extensions::Tracing` (`infra/api/v4.rs`) and converted from `tracing`
    spans into fastrace spans by `fastrace-tracing`'s `FastraceCompatLayer`.
- To start a new root: `.in_span(Span::root("name", SpanContext::random()))`.
- Logging style (kv field syntax, error chains) is documented in the module docs of
  `indexer-common/src/telemetry.rs`.

## Turning tracing on

Tracing is off by default. Config (all components share the shape):

```yaml
telemetry:
  tracing:
    enabled: true
    service_name: "chain-indexer"
    otlp_exporter_endpoint: "http://localhost:4317"
```

Env form: `APP__TELEMETRY__TRACING__ENABLED=true`,
`APP__TELEMETRY__TRACING__OTLP_EXPORTER_ENDPOINT=http://...:4317`.

Local quickstart: run Jaeger with OTLP ingest
(`docker run --rm -e COLLECTOR_OTLP_ENABLED=true -p 16686:16686 -p 4317:4317 jaegertracing/all-in-one`),
set `enabled: true`, run a component, open <http://localhost:16686>. For tests there is also
`console_reporter_enabled: true`, which additionally prints finished spans to stdout.

## Deployments and Datadog

As of July 2026 no environment enables tracing: there is no `telemetry.tracing` config in any
shielded-gitops indexer overlay and nothing exports OTLP. What Datadog receives today is the
JSON logs (cluster log collection) and the Prometheus metrics (the pod/service monitors in the
overlays).

To get traces into Datadog: enable OTLP ingestion on the cluster Datadog agent (gRPC, port
4317) and set the two env vars above on the indexer workloads, with the endpoint pointing at
the node-local agent. Nothing else is needed on the indexer side; the exporter is standard
OTLP.

## Gotchas

- Keep `fastrace_opentelemetry=off` in `RUST_LOG` (the justfile and deployments already do),
  otherwise the exporter logs its own export activity.
- With `enabled: false` no reporter is installed and spans are cheap no-ops, so `#[trace]` on
  hot paths costs nothing while tracing is off.
- `instrumentation_scope_name` / `instrumentation_scope_version` default to the crate name and
  version; only override them if you need to distinguish builds.
