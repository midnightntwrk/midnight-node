# Added `cngd-config.json` file for cNight Generates Dust chain-spec generation

At the moment, this file just includes the following:

```rust
	/// Address of the cNight mapping validator
	pub mapping_validator_address: String,
	/// Address of the glacier drop redemption validator
	pub redemption_validator_address: String,
	/// Policy ID of the currency token (i.e. cNIGHT)
	pub policy_id: String,
	/// Asset name of the currency token (i.e. cNIGHT)
	pub asset_name: String,
```

In future, it will include any cNight data we need for genesis generation.

PR: https://github.com/midnightntwrk/midnight-node/pull/47
Ticket: https://shielded.atlassian.net/browse/PM-19172
