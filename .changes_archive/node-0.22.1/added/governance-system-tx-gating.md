#node #runtime #ledger
# Add governance system transaction gating

Governance (federated-authority pallet) can currently dispatch any system
transaction via `MidnightSystem::send_mn_system_transaction`. This change adds a
new ledger runtime interface method that checks whether a given system
transaction is allowed for governance execution — only `OverwriteParameters`
(i.e. ledger parameter updates) is permitted.

PR: https://github.com/midnightntwrk/midnight-node/pull/658
Ticket: https://shielded.atlassian.net/browse/PM-21785
