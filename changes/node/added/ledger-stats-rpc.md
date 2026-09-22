#node #rpc #ledger

# Expose ledger-state collection sizes over RPC, including the UTXO set size

`midnight_ledgerStats` reports the sizes of the collections held at the
ledger-state root: the unshielded UTXO set size and the NIGHT it holds, the
zswap and DUST commitment and nullifier totals, and the deployed contract count.

None of this was reachable before. The UTXO set lives in the midnight-ledger
arena, a content-addressed blob outside the Substrate trie, so no
`state_getStorage` query reaches it and no runtime API returned it. Measuring the
UTXO set size previously meant either replaying the chain from genesis with the
toolkit, or summing the per-block `Midnight.UnshieldedTokens` events — minutes of
work for a number the ledger already maintains.

Every value is O(1). The counts are read off annotations maintained at each
storage trie root (`NightAnn { size, value }` for the UTXO and contract maps,
`SizeAnn` for the nullifier sets) and the commitment totals are scalar
`first_free` fields, so the cost does not grow with the state.

The method is served entirely node-side. It reads the raw
`pallet_midnight::StateKey` from the trie and resolves it in the node's
already-open arena, the same way the warp ledger-sync server does, so it needs no
runtime API and ships as a binary swap rather than a runtime upgrade.

Two notes for operators:

- The arena is behind process-global locks that block execution also takes, so a
  call can contend with block production. A per-block memo means repeated polling
  of the same block touches the arena exactly once; prefer a non-validator RPC
  node regardless.
- A node with default state pruning only retains trie state for recent blocks, so
  an `at` outside that window fails even though the arena still holds the data.
  Query an archive node for historical blocks.

PR: <link to PR>
