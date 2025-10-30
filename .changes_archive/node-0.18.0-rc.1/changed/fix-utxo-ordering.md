# Use better ordering for ObservedUtxo

The `ObservedUtxo` ordering method wasn't quite deterministic enough, it was ordering UTXOs by block number + tx index but not by UTXO index. Take UTXO index into account.

Ticket: https://shielded.atlassian.net/browse/PM-19779
PR: https://github.com/midnightntwrk/midnight-node/pull/106