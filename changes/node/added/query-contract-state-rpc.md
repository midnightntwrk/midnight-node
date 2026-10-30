#node #rpc
# Add `midnight_queryContractState` RPC for reading individual contract state fields

`midnight_contractState` returns a contract's whole serialized state, which for a large contract
is impractical or impossible to fetch. `midnight_queryContractState(contract_address, paths, at?)`
instead resolves up to 100 paths of up to 16 keys each against the contract's state tree, loading
only the nodes along each path. Each key is interpreted as the VM's `idx` instruction interprets it:
an array index, map key, or merkle tree position.

Path keys and returned cells are 0x-prefixed hex of tagged-serialized `AlignedValue`s. Each result
is `{"value": "0x…"}` for a cell, `{"value": null}` for a Null leaf (including an absent map key),
or `{"error": "…"}` for a path that cannot be followed.

The RPC is node-side only: it reads the ledger `StateKey` from the trie, like
`midnight_ledgerStats`, so it needs no runtime upgrade and works on blocks from both ledger 8 and 9.

PR: https://github.com/midnightntwrk/midnight-node/pull/1078
Issue: https://github.com/midnightntwrk/midnight-node/issues/1069
