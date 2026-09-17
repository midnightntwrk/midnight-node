#node #ledger
# Verify a transaction's ZK proofs once, not once per state it is validated against

`STRICT_TX_VALIDATION_CACHE` is keyed by `{state_hash, tx_hash, block_context_tblock}`,
and both non-`tx_hash` components move between mempool admission and block
execution: `validate_unsigned` skews the mempool's `tblock` forward by
`slot_duration * (1 + MaxSkippedSlots)` while `pre_dispatch` does not, and
`pallet_midnight` re-puts `StateKey` after every applied extrinsic. Either alone
missed the cache, and a miss re-ran the whole of `well_formed` — proofs included.
Measured on a dev node, every transaction had its proofs verified twice on the
node that both admitted and authored it (2.00x), plus once more on each importing
node.

`get_verified_transaction` now records, against the transaction's cryptographic
`transaction_hash`, the ledger state its proofs were verified at. On a later
strict-cache miss it reloads that state and re-checks the transaction through the
ledger's `RevalidationReference`, which skips `stateless_check` — proof, signature
and binding-commitment verification, all functions of the transaction bytes and
therefore unchanged — and re-runs only the state-dependent checks whose inputs
actually moved (ledger parameters, the contract's registered operation and
maintenance authority, and the Dust roots at the transaction's ctime). The same
dev-node measurement now reports 1.00x.

This applies whether or not batch verification is enabled, so nodes running the
shipped defaults benefit. It also replaces the batch ingress points' previous
handoff, a cache of "proofs already checked" keyed by the Twox128
`tx_validation_cache_key`: Twox128 collisions are constructible, so that key could
not safely gate skipping proof verification. The new key is SHA-256 over the
transaction's tagged serialization.

Measured cost of the second verification, per transaction, on the committed
contract fixture: 15.51 ms for a full `well_formed`, 4.95 ms for the previous
deferred-proofs path, 3.10 ms via `RevalidationReference`.

PR: <link to PR>
