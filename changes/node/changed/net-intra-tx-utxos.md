#node #security
# Accept a transaction that spends a UTXO it created itself

A transaction whose later intent spends an unshielded UTXO created by an
earlier intent is valid to the ledger, but the node's post-apply UTXO
integrity check reported that UTXO as both created and spent and rejected
the transaction. Mempool validation does not run that check, so the
transaction reached a block, failed at apply, and paid no fee while
occupying block weight.

UTXOs created and spent within one transaction are now netted out before
the integrity check. The change is gated on runtime `spec_version`
2.1.100 so that blocks produced before it replay unchanged.

Issue: https://github.com/shieldedtech/audit-taran/issues/58
