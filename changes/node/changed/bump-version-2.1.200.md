#node #runtime #toolkit #runtime-version
# Bump node, runtime and toolkit to 2.1.200

Node, runtime, metadata and toolkit crates move to `2.1.200`, as does
`docs/openrpc.json`, and the runtime `spec_version` to `002_001_200`. This is a
runtime release: the wasm changes in 2.1.200, so the runtime and metadata crates
move with the node this time. The toolkit block fetcher gains a `2.1.200`
runtime with its own subxt metadata and keeps decoding `2.1.100` blocks.

PR: TBD
