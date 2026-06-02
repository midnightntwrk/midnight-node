#toolkit

# Add commands for initiating bridge transfers.

Adds `bridge-transfer` command that submits transaction to Cardano. Transaction is from user wallet to ICS address and has metadata that encodes either: transfer to specified Midnight UserAddress, to reserve or invalid one (will end up in Treasury).

PR: https://github.com/midnightntwrk/midnight-node/pull/1340
Required for https://github.com/midnightntwrk/midnight-node/issues/1086
