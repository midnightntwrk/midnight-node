#ci
# Add srtool WASM build workflow for releases

New `srtool Build` GitHub Actions workflow that builds deterministic runtime WASM artifacts and uploads them to GitHub releases. Can be triggered standalone to backfill past releases, or runs automatically as part of the `Create Release` workflow.

Artifacts attached to releases: `midnight_node_runtime.wasm`, `.compact.wasm`, `.compact.compressed.wasm`, `srtool-digest.json`, checksums, and Cosign signatures.

A new `node-only` option on the `Create Release` workflow skips the srtool build for node-only releases that don't include runtime changes.

PR: https://github.com/midnightntwrk/midnight-node/pull/795
JIRA: https://shielded.atlassian.net/browse/PM-22075
