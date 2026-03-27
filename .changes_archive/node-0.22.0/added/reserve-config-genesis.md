#client #node #runtime #ledger
# Add reserve contract observation and use result to initialize genesis

Genesis generation now uses `LedgerState::with_genesis_settings()` to set
`locked_pool` from the reserve config, representing cNIGHT circulating on
Cardano. The remaining supply is allocated to `reserve_pool`. When no reserve
config is provided, behaviour is unchanged (locked_pool=0, reserve_pool=MAX_SUPPLY).

PR: https://github.com/midnightntwrk/midnight-node/pull/658
JIRA: https://shielded.atlassian.net/browse/PM-21785
