# Fix `get_utxos_up_to_capacity`

Fixes several separate bugs:
1. We were failing to accurately count how many TXs were fetched.
2. We were applying the wrong limit (a TX count) to queries which fetched UTXOs.
3. We had an off-by-one error when tracking how many TXs should be returned.

PR: https://github.com/midnightntwrk/midnight-node/pull/108
JIRA: https://shielded.atlassian.net/browse/PM-19776
