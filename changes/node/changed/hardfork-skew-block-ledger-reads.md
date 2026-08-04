#node #ledger
# Serve ledger state reads at the ledger-hardfork `set_code` block

On a chain that hardforks ledger 8 -> 9 via a governance `set_code`, exactly one historical
block was permanently unreadable: every ledger state read at that hash failed with
`Deserialization(TypedArenaKey)`. The pre-fork runtime shipped `system_version: 1`, so
`frame_system` overwrote `:code` *inside* the `set_code` block while pallet-midnight's v8 -> v9
state translation only ran in the next block's `initialize_block`. That block's committed state
therefore pairs ledger-9 `:code` with a ledger-8 `StateKey` forever, and any read at that hash
executes ledger-9 code against a ledger-8 arena root.

The read-only accessors of the ledger-9 host API now check the tagged-serialization header of
the `state_key` they are handed. If it is a ledger-8 arena root, the read is served from the
ledger-8 bridge instead — v8 and v9 share one storage crate and hence one arena, so this is a
pure dispatch with no data movement. The `StateKey` tag is the signal rather than the pallet
storage version, because the 2.0.0 runtime already ran ledger 9 while still reporting
pallet-midnight storage version 1.

Because the dispatch sits in the host function, it covers every caller of the affected reads —
the `midnight_*` JSON-RPCs, `MidnightRuntimeApi` through `state_call`, and subxt-based tooling
such as `chain-indexer` — rather than only the node's own RPC layer. Affected reads:
`get_contract_state`, `get_zswap_chain_state`, `get_zswap_state_root`, `get_ledger_state_root`,
`get_ledger_parameters`, `get_c_to_m_bridge_min_amount`, `get_unclaimed_amount`,
`get_bridge_receiving_amount`.

Not covered, deliberately: the transaction paths (`get_transaction_cost`,
`validate_transaction`, `apply_transaction`). At the skew block those concern ledger-9-format
transactions, which ledger-8 code cannot deserialize in any case, and they resolve on their own
one block later once the migration has run.

Behaviour note: at the `set_code` block `midnight_ledgerStateRoot` now returns a **v8-tagged**
root — correct, since that block's state *is* v8 — so consumers walking the fork see the tag
flip at the migration block rather than a hole.

PR: https://github.com/midnightntwrk/midnight-node/pull/1982
Issue: https://github.com/midnightntwrk/midnight-node/issues/1959
