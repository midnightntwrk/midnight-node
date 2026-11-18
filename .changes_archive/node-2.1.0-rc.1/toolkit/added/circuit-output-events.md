#toolkit
# Add `--output-events` to `generate-intent circuit`

`generate-intent circuit` now accepts an optional `--output-events <file>` argument that
writes the contract log events emitted during circuit execution to a JSON file. This
forwards to the `--output-events` flag already provided by `compact-js` 2.5.5-rc.7's
circuit command, exposing the new events API through the Rust toolkit. Events are only
written when the flag is supplied, and it requires `COMPACTC_VERSION` >= 0.33.0.

PR: https://github.com/midnightntwrk/midnight-node/pull/1910
Issue: https://github.com/midnightntwrk/midnight-node/issues/1639
