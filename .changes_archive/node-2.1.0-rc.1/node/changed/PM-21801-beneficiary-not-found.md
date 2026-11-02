#ledger #rpc
# Return BeneficiaryNotFound for absent beneficiaries in get_unclaimed_amount

`Bridge::get_unclaimed_amount` now returns `Err(LedgerApiError::BeneficiaryNotFound)`
when the queried beneficiary does not exist in the ledger state, rather than `Ok(0)`.
This allows RPC consumers to distinguish between a non-existent beneficiary and a
registered beneficiary with zero unclaimed rewards.

PR: https://github.com/midnightntwrk/midnight-node/pull/1359
JIRA: https://shielded.atlassian.net/browse/PM-21801
