#node #ledger

# Return `NoLedgerState` for an absent ledger root instead of panicking

`get_ledger` loaded the ledger root with `get_lazy`, which reads nothing, so a
state key whose root is missing from the node's ledger DB only failed at the
first deref, as a host panic. In the inherent data provider
(`CNightObservationApi_get_observation_cost_model`) that trap aborted every
authoring attempt on the affected validator.

The root is now read up front, for ledger 8 and ledger 9, and an absent root
returns `LedgerApiError::NoLedgerState`. The read is cached, so the deref that
follows costs nothing extra. Only the root is checked: a node missing deeper
in the tree still needs a resync.

PR: TBD
