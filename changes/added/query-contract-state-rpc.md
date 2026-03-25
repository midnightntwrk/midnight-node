#client #node #rpc #api #runtime
# Add `midnight_queryContractState` RPC for lazy contract state access

Query specific fields from a contract's state tree by path, without loading the full state. Enables clients to fetch individual contract fields in O(log n) instead of deserializing the entire state blob.

Each query navigates the state tree using serialized `AlignedValue` keys (array index, map key, or merkle tree position), mirroring the VM's `idx` instruction. The RPC reads the state key via a new runtime API getter (`get_state_key`, bumps `api_version` to 6), then calls the bridge directly for lazy navigation in ParityDB.

- `midnight_queryContractState(address, queries)` accepts up to 100 queries with max path depth 16
- Returns per-query results with value (hex-encoded tagged-serialized `StateValue`) or error
- No new types cross the WASM boundary

PR: https://github.com/midnightntwrk/midnight-node/pull/1078
