#toolkit
# Drop `structured_logger` in favour of `tracing_subscriber`; Breaking JSON structured log format change

Format now looks like:
```
{"timestamp":"2026-03-10T17:26:45.103688Z","level":"INFO","fields":{"message":"spawning 20 fetch workers","log.target":"midnight_node_toolkit::fetcher","log.module_path":"midnight_node_toolkit::fetcher","log.file":"util/toolkit/src/fetcher.rs","log.line":171},"target":"midnight_node_toolkit::fetcher"}
```

PR: https://github.com/midnightntwrk/midnight-node/pull/899
