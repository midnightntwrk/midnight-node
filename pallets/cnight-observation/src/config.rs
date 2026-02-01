use alloc::{collections::BTreeMap, string::String, vec::Vec};
use midnight_primitives_cnight_observation::{
	CNightAddresses, CardanoPosition, CardanoRewardAddressBytes, DustPublicKeyBytes, ObservedUtxos,
};
use serde::{Deserialize, Serialize};

use crate::MappingEntry;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(serde_valid::Validate))]
pub struct CNightGenesis {
	#[cfg_attr(feature = "std", validate)]
	pub addresses: CNightAddresses,
	pub observed_utxos: ObservedUtxos,
	#[serde(with = "mappings_serde")]
	pub mappings: BTreeMap<CardanoRewardAddressBytes, Vec<MappingEntry>>,
	#[serde(with = "utxo_owners_serde")]
	pub utxo_owners: BTreeMap<[u8; 32], DustPublicKeyBytes>,
	pub next_cardano_position: CardanoPosition,
	pub system_tx: Option<SystemTx>,
}

mod mappings_serde {
	use super::*;
	use serde::{Deserializer, Serializer, de::MapAccess, ser::SerializeMap};

	pub fn serialize<S>(
		map: &BTreeMap<CardanoRewardAddressBytes, Vec<MappingEntry>>,
		serializer: S,
	) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let mut ser_map = serializer.serialize_map(Some(map.len()))?;
		for (k, v) in map {
			let key_hex = hex::encode(k.0);
			ser_map.serialize_entry(&key_hex, v)?;
		}
		ser_map.end()
	}

	pub fn deserialize<'de, D>(
		deserializer: D,
	) -> Result<BTreeMap<CardanoRewardAddressBytes, Vec<MappingEntry>>, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct MapVisitor;

		impl<'de> serde::de::Visitor<'de> for MapVisitor {
			type Value = BTreeMap<CardanoRewardAddressBytes, Vec<MappingEntry>>;

			fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
				formatter.write_str("a map with hex-encoded CardanoRewardAddressBytes keys")
			}

			fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
			where
				M: MapAccess<'de>,
			{
				let mut map = BTreeMap::new();
				while let Some((key, value)) = access.next_entry::<String, Vec<MappingEntry>>()? {
					let bytes: Vec<u8> = hex::decode(&key).map_err(serde::de::Error::custom)?;
					let addr = CardanoRewardAddressBytes::try_from(bytes).map_err(|_| {
						serde::de::Error::custom("invalid CardanoRewardAddressBytes length")
					})?;
					map.insert(addr, value);
				}
				Ok(map)
			}
		}

		deserializer.deserialize_map(MapVisitor)
	}
}

mod utxo_owners_serde {
	use super::*;
	use serde::{Deserializer, Serializer, de::MapAccess, ser::SerializeMap};

	pub fn serialize<S>(
		map: &BTreeMap<[u8; 32], DustPublicKeyBytes>,
		serializer: S,
	) -> Result<S::Ok, S::Error>
	where
		S: Serializer,
	{
		let mut ser_map = serializer.serialize_map(Some(map.len()))?;
		for (k, v) in map {
			let key_hex = hex::encode(k);
			ser_map.serialize_entry(&key_hex, v)?;
		}
		ser_map.end()
	}

	pub fn deserialize<'de, D>(
		deserializer: D,
	) -> Result<BTreeMap<[u8; 32], DustPublicKeyBytes>, D::Error>
	where
		D: Deserializer<'de>,
	{
		struct MapVisitor;

		impl<'de> serde::de::Visitor<'de> for MapVisitor {
			type Value = BTreeMap<[u8; 32], DustPublicKeyBytes>;

			fn expecting(&self, formatter: &mut core::fmt::Formatter) -> core::fmt::Result {
				formatter.write_str("a map with hex-encoded [u8; 32] keys")
			}

			fn visit_map<M>(self, mut access: M) -> Result<Self::Value, M::Error>
			where
				M: MapAccess<'de>,
			{
				let mut map = BTreeMap::new();
				while let Some((key, value)) = access.next_entry::<String, DustPublicKeyBytes>()? {
					let bytes: Vec<u8> = hex::decode(&key).map_err(serde::de::Error::custom)?;
					let arr: [u8; 32] = bytes.try_into().map_err(|_| {
						serde::de::Error::custom("invalid key length, expected 32 bytes")
					})?;
					map.insert(arr, value);
				}
				Ok(map)
			}
		}

		deserializer.deserialize_map(MapVisitor)
	}
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
    "mapping_validator_address": "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng",
    "redemption_validator_address": "addr_test1wz3t0v4r0kwdfnh44m87z4rasp4nj0rcplfpmwxvhhrzhdgl45vx4",
    "auth_token_asset_name": "",
    "cnight_policy_id": "d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1",
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
    "auth_token_asset_name": "",
    "cnight_policy_id": "d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1",
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

	#[test]
	fn test_roundtrip_serialization_with_nonempty_maps() {
		use midnight_primitives_cnight_observation::CardanoRewardAddressBytes;

		// JSON with non-empty mappings and utxo_owners using hex string keys
		let my_json = r#"{
  "addresses": {
    "mapping_validator_address": "addr_test1wplxjzranravtp574s2wz00md7vz9rzpucu252je68u9a8qzjheng",
    "redemption_validator_address": "addr_test1wz3t0v4r0kwdfnh44m87z4rasp4nj0rcplfpmwxvhhrzhdgl45vx4",
    "auth_token_asset_name": "",
    "cnight_policy_id": "d2dbff622e509dda256fedbd31ef6e9fd98ed49ad91d5c0e07f68af1",
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
  "mappings": {
    "e0efc0a73bab244aa74254f9db955e9d47313c15ad9e621bfc669711be": [
      {
        "cardano_reward_address": "e0efc0a73bab244aa74254f9db955e9d47313c15ad9e621bfc669711be",
        "dust_public_key": [1, 2, 3],
        "utxo_tx_hash": "0102030405060708091011121314151617181920212223242526272829303132",
        "utxo_index": 0
      }
    ]
  },
  "utxo_owners": {
    "0102030405060708091011121314151617181920212223242526272829303132": [4, 5, 6]
  },
  "next_cardano_position": {
    "block_hash": "0000000000000000000000000000000000000000000000000000000000000000",
    "block_number": 0,
    "block_timestamp": 0,
    "tx_index_in_block": 0
  },
  "system_tx": null
}"#;

		// Deserialize
		let genesis: CNightGenesis = serde_json::from_str(my_json).unwrap();

		// Verify mappings were parsed correctly
		assert_eq!(genesis.mappings.len(), 1);
		let expected_reward_addr_bytes: [u8; 29] =
			hex::decode("e0efc0a73bab244aa74254f9db955e9d47313c15ad9e621bfc669711be")
				.unwrap()
				.try_into()
				.unwrap();
		let expected_reward_addr = CardanoRewardAddressBytes(expected_reward_addr_bytes);
		assert!(genesis.mappings.contains_key(&expected_reward_addr));

		// Verify utxo_owners were parsed correctly
		assert_eq!(genesis.utxo_owners.len(), 1);
		let expected_utxo_key: [u8; 32] =
			hex::decode("0102030405060708091011121314151617181920212223242526272829303132")
				.unwrap()
				.try_into()
				.unwrap();
		assert!(genesis.utxo_owners.contains_key(&expected_utxo_key));

		// Serialize back to JSON
		let serialized = serde_json::to_string_pretty(&genesis).unwrap();

		// Deserialize again and verify round-trip
		let genesis2: CNightGenesis = serde_json::from_str(&serialized).unwrap();
		assert_eq!(genesis.mappings.len(), genesis2.mappings.len());
		assert_eq!(genesis.utxo_owners.len(), genesis2.utxo_owners.len());

		// Verify the hex keys are present in serialized output
		assert!(serialized.contains("e0efc0a73bab244aa74254f9db955e9d47313c15ad9e621bfc669711be"));
		assert!(
			serialized.contains("0102030405060708091011121314151617181920212223242526272829303132")
		);
	}
}
