#node #testing #simulation
# Test-only `mock-proofs` and `fast-blocks` features, and a deterministic simulation testbed

`mock-proofs` (node, `midnight-node-ledger`) skips contract and DUST proof verification so
transactions from `toolkit --proof-server mock` are accepted; zswap proofs and every other check
stay on. `fast-blocks` (node, runtime) shortens the slot from 6s to 2s. Neither may be used for a
deployed chain.

`tests/simulation` (crate `midnight-simulation`) runs the real pallets and ledger in
`TestExternalities` with a virtual clock and a simulated Cardano with fault injection, checked by
property tests: an independent model of the cNIGHT observation rules, ledger-state invariants
taken from the specification, runtime/toolkit state-root parity for generated transactions, and
committee rotation on a virtual epoch clock. The spec property for an epoch boundary without
mainchain data is ignored: `create_inherent` currently panics there, which loses one slot per
sidechain epoch on a live chain.

PR: TBD
