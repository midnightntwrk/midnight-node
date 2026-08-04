#node #rpc
# Serve ledger state RPCs at the ledger-hardfork `set_code` block

On a chain that hardforks ledger 8 -> 9 via a governance `set_code`, exactly one historical
block was permanently unreadable through `midnight_zswapStateRoot`, `midnight_ledgerStateRoot`
and `midnight_contractState` (`-32602`, underlying `Deserialization(TypedArenaKey)`). The
pre-fork runtime shipped `system_version: 1`, so `frame_system` overwrote `:code` *inside* the
`set_code` block while pallet-midnight's v8 -> v9 state translation only ran in the next block's
`initialize_block`. That block's committed state therefore pairs ledger-9 `:code` with a
ledger-8 `StateKey` forever, and any read at that hash executes ledger-9 WASM against a
ledger-8 arena root.

The node's RPC layer now reads `Midnight::StateKey` straight from the state backend and checks
its tagged-serialization header. If the key belongs to the older ledger version, the read is
served by calling that version's host functions natively against the same process-global arena;
otherwise the existing runtime-API path is used unchanged. The `StateKey` tag is the signal
rather than the pallet storage version, because the 2.0.0 runtime already ran ledger 9 while
still reporting pallet-midnight storage version 1.

Behaviour note: at the `set_code` block `midnight_ledgerStateRoot` now returns a **v8-tagged**
root — correct, since that block's state *is* v8 — so consumers walking the fork see the tag
flip at the migration block rather than a hole.

Not covered: `chain-indexer` reads `get_zswap_state_root` through a raw `state_call` runtime
API rather than the `midnight_*` RPCs, so it is unaffected by this fix and still needs either
to switch to `midnight_zswapStateRoot` or its own skip/fallback at that block.

PR: https://github.com/midnightntwrk/midnight-node/pull/1982
Issue: https://github.com/midnightntwrk/midnight-node/issues/1959
