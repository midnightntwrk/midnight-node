#client #node #runtime #ledger
# Wire reserve config into LedgerState genesis

Genesis generation now uses `LedgerState::with_genesis_settings()` to set
`locked_pool` from the reserve config, representing cNIGHT circulating on
Cardano. The remaining supply is allocated to `reserve_pool`. When no reserve
config is provided, behaviour is unchanged (locked_pool=0, reserve_pool=MAX_SUPPLY).

PR:
JIRA: https://shielded.atlassian.net/browse/PM-21785
