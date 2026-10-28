#toolkit
# Recognise the 1.0.3 runtime when fetching blocks

The toolkit's block fetcher rejected any block whose `spec_version` it did not know, so blocks
produced by the 1.0.3 runtime (`001_000_003`) failed with `UnsupportedBlockVersion`. Its metadata
is now bundled and mapped like the other supported runtimes.

Backport of https://github.com/midnightntwrk/midnight-node/pull/2002.
