#toolkit
# Recognise the 1.0.300 runtime when fetching blocks

The toolkit's block fetcher rejected any block whose `spec_version` it did not know, so blocks
produced by the 1.0.300 runtime (`001_000_300`) failed with `UnsupportedBlockVersion`. Its metadata
is now bundled and mapped like the other supported runtimes, replacing support for the 1.0.3
runtime.
