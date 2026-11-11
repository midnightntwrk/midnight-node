#node #toolkit
# Bump node and toolkit to 2.1.200; runtime stays 2.1.100

The node and toolkit binaries move to `2.1.200`, as does `docs/openrpc.json`.
The runtime does not: the runtime and metadata crates stay `2.1.100` and
`spec_version` stays `002_001_100`, so the wasm is unchanged and there is no
runtime upgrade. The revert of #2421 backed out the runtime bump and, with it,
this binary bump, which was meant to stay.

PR: TODO
