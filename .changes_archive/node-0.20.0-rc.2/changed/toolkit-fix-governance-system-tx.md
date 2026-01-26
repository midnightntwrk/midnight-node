#toolkit
# Fix toolkit not processing governance-wrapped system transactions

The toolkit now extracts system transactions from all `SystemTransactionApplied` events in a block, rather than only from direct extrinsic calls or `CNightObservation` events. This makes the toolkit resilient to system transactions wrapped in any call type, including governance motions (`FederatedAuthority::motion_dispatch`), batch calls, proxy calls, or any future wrapper patterns.

Previously, when a system transaction (like `update-ledger-parameters`) was executed through governance, the toolkit would miss it because it only looked for direct `MidnightSystem::send_mn_system_transaction` extrinsics or events from `CNightObservation` calls. This caused the toolkit's ledger state to diverge from the chain, resulting in "Ledger state root mismatch" errors.

PR: https://github.com/midnightntwrk/midnight-node/pull/484
JIRA: https://shielded.atlassian.net/browse/PM-21246
