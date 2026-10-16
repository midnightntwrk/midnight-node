#node
# Bump midnight-ledger from 8.1.0-rc.1 to 8.1.0

Promotes the Ledger 8 pin from the release candidate (`crate-ledger-8.1.0-rc.1`)
to the final `ledger-8.1.0` tag. Headline upstream changes: `storage-core 1.2.0`
gains an incremental garbage collector and shared-ParityDB-backend access, plus
fixes for a race condition in `force_as_arc`, an `Sp` serialization panic, a
memory leak in pending `Updates`, and a lock-ordering violation. `midnight-ledger`
itself adds finer-grained WASM wallet bindings (wallet-facing only).

All midnight-ledger workspace crates are now resolved from crates.io at their
8.1.0 release versions; the previous `[patch.crates-io]` block pinning them to
the `ledger-8.1.0` git tag has been removed now that the 8.1.0 crates are
published.

PR: https://github.com/midnightntwrk/midnight-node/pull/1510
