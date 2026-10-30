#node #runtime #toolkit #runtime-version

# Bump node, runtime and toolkit to 4.0.0

The `3.0.0` release line has been branched off, so main opens `4.0.0`: node,
runtime, metadata and toolkit crates move to `4.0.0` and the runtime
`spec_version` to `004_000_000`.

The toolkit block fetcher gains a `V4_0_0` runtime with its own subxt metadata
(`metadata/static/midnight_metadata_4.0.0.scale`, regenerated from a
`CFG_PRESET=dev` node) and keeps decoding `3.0.0` blocks.

Change files for everything released on the `3.0.0` line are moved to
`.changes_archive/node-3.0.0/`; change files added after the branch point
(`8dab9c55d`) stay in `changes/` for `4.0.0`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2339
