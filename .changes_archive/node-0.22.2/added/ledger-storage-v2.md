#node

# Enable ledger storage v2 layout

Enable the `layout-v2` feature on `midnight-storage-core` 1.1.0, which removes
reference counting from the storage layer to eliminate quasi-exponential write
cost growth. Ledger 7 deps moved to crates.io with `[patch.crates-io]` to
enable the storage-core override.

PR: https://github.com/midnightntwrk/midnight-node/pull/847
JIRA: https://shielded.atlassian.net/browse/PM-22058
