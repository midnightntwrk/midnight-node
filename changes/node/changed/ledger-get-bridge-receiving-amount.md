#ledger

# Add get_bridge_receiving_amount ledger host API

Adds `get_bridge_receiving_amount` alongside the existing `get_unclaimed_amount`
across all ledger host-API layers (`Ledger`, `Bridge`, and the `ledger_7`/`ledger_8`/
`ledger_9` host interfaces). It returns the NIGHT amount a beneficiary can claim from
Cardano protocol bridge transfers — read from the ledger `bridge_receiving` map, already
net of the bridge fee — keyed by the beneficiary's unshielded `UserAddress`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1766
Issue: https://github.com/midnightntwrk/midnight-node/issues/1765
