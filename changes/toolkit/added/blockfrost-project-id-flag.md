#toolkit #indexer
# Add `--blockfrost-project-id` and document the indexer path

The Blockfrost API key for a Blockfrost `--indexer-url` can now be passed as
`--blockfrost-project-id <KEY>`. `BLOCKFROST_PROJECT_ID` still works and remains the recommended
way, since a flag is visible in the process list; the flag takes precedence when both are set. The
key is redacted from `Debug` output and hidden from `--help`. A blank key is now rejected up front
like a missing one, instead of being sent as an empty header.

The toolkit README gains a section on reading wallet state from an indexer, including Blockfrost.

PR: https://github.com/midnightntwrk/midnight-node/pull/2458
