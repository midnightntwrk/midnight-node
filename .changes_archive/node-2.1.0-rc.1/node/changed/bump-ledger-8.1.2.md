# Bump ledger 8 version to 8.1.2
Security patch: low-level deserialization is hardened - non-canonical encodings and
invariant-violating values are now rejected, so an 8.1.2 node
accepts strictly less than an 8.1.1 one. Also carries panic/overflow fixes in Dust
parameters and seq, Zswap binding randomness, delta normalization and contract-call
cost accounting.
Release notes: https://github.com/midnightntwrk/midnight-ledger/releases/tag/ledger-8.1.2

PR: https://github.com/midnightntwrk/midnight-node/pull/2096
