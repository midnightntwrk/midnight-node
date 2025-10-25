use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use serde::{Deserialize, Serialize};
use sp_std::{collections::btree_map::BTreeMap, vec::Vec};

use crate::MappingEntry;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(serde_valid::Validate))]
pub struct CNightGenesis {
	#[cfg_attr(feature = "std", validate)]
	pub addresses: CNightAddresses,
	pub observed_utxos: ObservedUtxos,
	pub mappings: BTreeMap<Vec<u8>, Vec<MappingEntry>>,
	pub utxo_owners: BTreeMap<Vec<u8>, Vec<u8>>,
	pub next_cardano_position: CardanoPosition,
	pub system_tx: Option<SystemTx>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SystemTx(#[serde(with = "hex")] pub Vec<u8>);

#[cfg(test)]
mod tests {
	use crate::config::CNightGenesis;
	use serde_valid::Validate;

	#[test]
	fn test_validation_ok() {
		let my_json = r#"{
  "addresses": {
    "mapping_validator_address": "addr_test1wral0lzw5kpjytmw0gmsdcgctx09au24nt85zma38py8g3crwvpwe",
    "redemption_validator_address": "addr_test1wz3t0v4r0kwdfnh44m87z4rasp4nj0rcplfpmwxvhhrzhdgl45vx4",
    "cnight_policy_id": "03cf16101d110dcad9cacb225f0d1e63a8809979e7feb60426995414",
    "cnight_asset_name": ""
  },
  "observed_utxos": {
    "start": {
      "block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
      "block_number": 0,
      "block_timestamp": 0,
      "tx_index_in_block": 0
    },
    "end": {
      "block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
      "block_number": 0,
      "block_timestamp": 0,
      "tx_index_in_block": 0
    },
    "utxos": []
  },
  "mappings": {},
  "utxo_owners": {},
  "next_cardano_position": {
    "block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
    "block_number": 0,
    "block_timestamp": 0,
    "tx_index_in_block": 0
  },
  "system_tx": null
}"#;

		let genesis: CNightGenesis = serde_json::from_str(my_json).unwrap();

		assert!(genesis.validate().is_ok());
	}

	#[test]
	fn test_validation_bad_addresses() {
		let my_json = r#"{
  "addresses": {
    "mapping_validator_address": "nonsense",
    "redemption_validator_address": "addr_test1wz3t0v4r0kwdfnh44m87z4rasp4nj0rcplfpmwxvhhrzhdgl45vx4",
    "cnight_policy_id": "03cf16101d110dcad9cacb225f0d1e63a8809979e7feb60426995414",
    "cnight_asset_name": ""
  },
  "observed_utxos": {
    "start": {
      "block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
      "block_number": 0,
      "block_timestamp": 0,
      "tx_index_in_block": 0
    },
    "end": {
      "block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
      "block_number": 0,
      "block_timestamp": 0,
      "tx_index_in_block": 0
    },
    "utxos": []
  },
  "mappings": {},
  "utxo_owners": {},
  "next_cardano_position": {
    "block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
    "block_number": 0,
    "block_timestamp": 0,
    "tx_index_in_block": 0
  },
  "system_tx": null
}"#;

		let genesis: CNightGenesis = serde_json::from_str(my_json).unwrap();

		assert!(genesis.validate().is_err());
	}
}
