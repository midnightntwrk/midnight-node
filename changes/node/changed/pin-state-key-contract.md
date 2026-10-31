#tests #ledger

# Pin the `StateKey` contract the indexer relies on

`pallet_midnight::StateKey` is the tagged arena key of the node's `Ledger`
wrapper with zero `block_fullness` at the end of every block. The indexer
reproduces it from its own ledger state, so it can validate every block without
loading historical ledger states once the node prunes them.

Tests now pin that contract: after `on_finalize` the wrapper's fullness is zero
at genesis and after regular, cNIGHT and bridge transactions, and `get_root`
reproduces golden block-0 `StateKey` vectors shared with the indexer for
ledger 8 and ledger 9. No runtime or client behaviour changes.

PR: https://github.com/midnightntwrk/midnight-node/pull/2345
Issue: https://github.com/midnightntwrk/midnight-node/issues/1983
