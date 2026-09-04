#runtime #ledger

# Emit contract events for the succeeding segments of a partial-success transaction

A partial-success transaction emitted no contract events at all. The set of segments
withheld from the event was built from *every* segment the transaction attempted rather
than only the ones that failed, so a single failing fallible segment suppressed the
`ContractCall` / `ContractDeploy` / `ContractMaintain` events of the guaranteed segment
and of every succeeding fallible segment too. Fees, UTXO events, `TxPartialSuccess` and
the state root were unaffected.

`System::Events` lives in the state trie, so the correction is gated on the
`ledger_9_bridge::apply_transaction` host-function version: historical blocks execute the
runtime wasm that was on-chain at their height, which imports v1 and keeps the old
behaviour, while the rebuilt runtime imports v2 and emits the events. Node binaries must
therefore be upgraded before the runtime `set_code`.

PR: <link to PR>
