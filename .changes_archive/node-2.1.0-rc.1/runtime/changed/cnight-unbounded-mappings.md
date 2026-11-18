#cnight
# Fix unbounded allocation in cNight Observation pallet

We were using an unbounded vec to store all cNight mappings for a given user. Replaced this with a single `StorageDoubleMap` keyed by `(reward address, sidechain_domain::UtxoId)` — inserting and removing mappings is now O(1) in space and time, and the per-address "is this address registered?" check costs at most two storage reads via bounded prefix iteration.

PR: https://github.com/midnightntwrk/midnight-node/pull/1423
Issue: https://github.com/midnightntwrk/midnight-security/issues/116
