#node
# Silence cNIGHT observation logs for unregistered and non-base Cardano addresses

Remove the mainchain-follower `debug!` for Cardano addresses without a
delegation part (enterprise/pointer/reward) — these can never register
for DUST by construction, so the log carries no signal.

Demote the cnight-observation pallet's "No valid dust registration" and
"No create event for UTXO" `warn!`s to `trace!`. Both fire once per
cNIGHT UTXO belonging to an unregistered Cardano reward address and
scaled with mainnet cNIGHT activity, drowning out real warnings.
Operators can still re-enable them with `RUST_LOG=pallet_cnight_observation=trace`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1324
Issue: https://github.com/midnightntwrk/midnight-node/issues/1268
