#runtime
# Add root extrinsics to set cNIGHT contract identifiers

Add two Root-only extrinsics to `pallet-cnight-observation`:

- `set_cnight_identifier(policy_id, asset_name)` replaces the (policy id,
  asset name) pair identifying the cNIGHT native asset on Cardano.
- `set_auth_token_asset_name(asset_name)` replaces the asset name of the
  auth token used by the mapping validator on Cardano.

These let an ephemeral fork redirect cNIGHT observation to the STAGING track
of a contract for upgradeability testing, without changing genesis. Both
reject inputs that exceed their `BoundedVec` bounds with a new
`CardanoIdentifierLengthExceeded` error.

PR: https://github.com/midnightntwrk/midnight-node/pull/1602
Issue: https://github.com/midnightntwrk/midnight-node/issues/1561
