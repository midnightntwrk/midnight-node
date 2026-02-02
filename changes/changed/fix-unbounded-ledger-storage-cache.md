# Fixes unbounded ledger storage cache size causing OOM on sync

Previously, the ledger storage cache was unbounded.
Now, Ledger storage cache size is now set to the default value - we should expect far less memory usage during sync and during normal operations.

PR: https://github.com/midnightntwrk/midnight-node/pull/579
