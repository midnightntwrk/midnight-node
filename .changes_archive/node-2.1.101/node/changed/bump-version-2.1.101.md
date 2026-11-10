#node #toolkit
# Bump node and toolkit crates to 2.1.101

Node and toolkit crates move to `2.1.101`, as does `docs/openrpc.json`. The
runtime is not released this time, so its `spec_version` stays at `002_001_100`
and the runtime and metadata crates stay at `2.1.100` (crate version tracks
`spec_version`, as with node 1.0.400 over runtime 1.0.300): a 2.1.101 node runs
the 2.1.100 runtime with unchanged metadata.

PR: https://github.com/midnightntwrk/midnight-node/pull/2382
