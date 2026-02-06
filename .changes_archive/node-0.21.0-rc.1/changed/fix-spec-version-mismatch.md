#client
# Fix genesis mismatch on sync when node client version != genesis runtime version

The genesis block version digest was using the compiled-in
midnight_node_runtime::VERSION.spec_version, which could differ from the
chainspec's WASM runtime. Replace resolve_state_version_from_wasm with a local
resolve_runtime_version_from_wasm that returns the full RuntimeVersion, so both
state_version and spec_version come from the chainspec WASM blob.

Ticket: https://shielded.atlassian.net/browse/PM-21720
PR: https://github.com/midnightntwrk/midnight-node/pull/615
