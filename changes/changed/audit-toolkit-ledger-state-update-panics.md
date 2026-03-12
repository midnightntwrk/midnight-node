#toolkit #security
# Replace expect calls with Result error propagation in ledger state updates

Replace `expect` calls in `update_from_block` and `update_from_tx` with proper `Result`-based error propagation to prevent panics and mutex poisoning. Addresses audit finding Issue AB.

PR: https://github.com/midnightntwrk/midnight-node/pull/927
JIRA: https://shielded.atlassian.net/browse/PM-19977
