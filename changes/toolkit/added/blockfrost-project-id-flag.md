#toolkit #indexer
# Add `--blockfrost-project-id` and document the indexer path

The Blockfrost API key for a Blockfrost `--indexer-url` can now be passed as
`--blockfrost-project-id <KEY>`. `BLOCKFROST_PROJECT_ID` still works and remains the recommended
way, since a flag is visible in the process list; the flag takes precedence when both are set. The
key is redacted from `Debug` output and hidden from `--help`. A blank key is now rejected up front
like a missing one, instead of being sent as an empty header.

The indexer sync progress line no longer prints `dust 0/…` for a stream that has already finished.
DUST fast sync reads a snapshot and never reports event counts, so the placeholder stayed on screen
for the whole run. Each stream now reads `done` once all its wallets are synced, `pending` before
the indexer has reported anything, and `<scanned>/unknown` if it has events but no total yet.

The toolkit README gains a section on reading wallet state from an indexer, including Blockfrost.

PR: https://github.com/midnightntwrk/midnight-node/pull/2458
