#runtime #node
# Bump midnight-ledger from 8.1.0-rc.1 to 8.1.0

Promotes the Ledger 8 pin from the release candidate (`crate-ledger-8.1.0-rc.1`)
to the final `ledger-8.1.0` tag. Headline upstream changes: `storage-core 1.2.0`
gains an incremental garbage collector and shared-ParityDB-backend access, plus
fixes for a race condition in `force_as_arc`, an `Sp` serialization panic, a
memory leak in pending `Updates`, and a lock-ordering violation. `midnight-ledger`
itself adds finer-grained WASM wallet bindings (wallet-facing only).

All midnight-ledger workspace crates are now sourced from a single tag
(`ledger-8.1.0`) to keep the dependency graph consistent — without this, the L7
chain's crates.io-resolved `base-crypto`/`coin-structure`/`zkir`/`storage`
copies conflict with the L8 chain's git-sourced versions (same crate version,
drifted source).

PR: https://github.com/midnightntwrk/midnight-node/pull/1510
