#runtime
# Add root extrinsics to set cNIGHT contract identifiers

Add two Root-only extrinsics to `pallet-cnight-observation`:

- `set_cnight_identifier(policy_id, asset_name)` replaces the (policy id,
  asset name) pair identifying the cNIGHT native asset on Cardano.
- `set_auth_token_asset_name(asset_name)` replaces the asset name of the
  auth token used by the mapping validator on Cardano.

These let an ephemeral fork redirect cNIGHT observation to the STAGING track
of a contract for upgradeability testing, without changing genesis. Argument
lengths are enforced at decode time (`policy_id` is a fixed 28-byte array,
asset names are `BoundedVec`s), and asset names must be ASCII — non-ASCII
bytes are rejected with a new `NonAsciiAssetName` error so a root call cannot
store values that would break inherent creation for block authors.

PR: https://github.com/midnightntwrk/midnight-node/pull/1602
Issue: https://github.com/midnightntwrk/midnight-node/issues/1561
