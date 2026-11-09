#node #security
# Accept a transaction that spends a UTXO it created itself

A transaction whose later intent spends an unshielded UTXO created by an
earlier intent is valid to the ledger, but the node's post-apply UTXO
integrity check reported that UTXO as both created and spent and rejected
the transaction. Mempool validation did not run that check, so the
transaction reached a block, failed at apply, and paid no fee while
occupying block weight.

UTXOs created and spent within one transaction are now netted out before
the integrity check, and mempool validation and `pre_dispatch` run the
same check on the guaranteed segment they dry-run, so a transaction the
node would reject after apply never reaches a block. Both ship as new
host-function versions (`apply_transaction` v3, `validate_transaction` and
`validate_guaranteed_execution` v2): runtimes bound to the old versions
replay unchanged, and a runtime that binds the new ones cannot be
imported by a node without this change. Node binaries must therefore roll
out before the runtime upgrade.
