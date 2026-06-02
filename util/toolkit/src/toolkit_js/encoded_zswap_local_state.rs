use midnight_node_ledger_helpers::{
	CoinPublicKey, ContractAddress, DB, Deserializable, HashOutput, PERSISTENT_HASH_BYTES,
	Serializable, WalletState,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodedQualifiedShieldedCoinInfo {
	pub(crate) nonce: [u8; PERSISTENT_HASH_BYTES],
	pub(crate) color: [u8; PERSISTENT_HASH_BYTES],
	#[serde(with = "string")]
	pub(crate) value: u128,
	#[serde(with = "string")]
	pub(crate) mt_index: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EncodedShieldedCoinInfo {
	pub(crate) nonce: [u8; PERSISTENT_HASH_BYTES],
	pub(crate) color: [u8; PERSISTENT_HASH_BYTES],
	#[serde(with = "string")]
	pub(crate) value: u128,
}

impl EncodedShieldedCoinInfo {
	pub(crate) fn new(
		nonce: [u8; PERSISTENT_HASH_BYTES],
		color: [u8; PERSISTENT_HASH_BYTES],
		value: u128,
	) -> Self {
		Self { nonce, color, value }
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedOutput {
	pub(crate) coin_info: EncodedShieldedCoinInfo,
	pub(crate) recipient: EncodedRecipient,
}

impl EncodedOutput {
	pub(crate) fn new(coin_info: EncodedShieldedCoinInfo, recipient: EncodedRecipient) -> Self {
		Self { coin_info, recipient }
	}
}
/// Either a coin public key if the recipient is a user, or a contract address
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodedRecipient {
	pub(crate) is_left: bool,
	#[serde(with = "bytes")]
	pub(crate) left: EncodedCoinPublic,
	#[serde(with = "bytes")]
	pub(crate) right: EncodedContractAddress,
}

impl EncodedRecipient {
	pub(crate) fn user(coin_public: EncodedCoinPublic) -> Self {
		Self {
			is_left: true,
			left: coin_public,
			right: EncodedContractAddress(ContractAddress::default()),
		}
	}
}

#[derive(Debug, Clone)]
pub struct EncodedContractAddress(pub(crate) ContractAddress);

impl From<&EncodedContractAddress> for Vec<u8> {
	fn from(value: &EncodedContractAddress) -> Self {
		let mut bytes = Vec::new();
		<ContractAddress as Serializable>::serialize(&value.0, &mut bytes)
			.expect("failed to serialize contract address");
		bytes
	}
}

impl TryFrom<Vec<u8>> for EncodedContractAddress {
	type Error = String;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let contract_address = <ContractAddress as Deserializable>::deserialize(&mut &value[..], 0)
			.map_err(|e| format!("failed deserializing encoded contract address: {e}"))?;
		Ok(EncodedContractAddress(contract_address))
	}
}

#[derive(Debug, Clone)]
pub struct EncodedCoinPublic(pub(crate) CoinPublicKey);

impl EncodedCoinPublic {
	pub(crate) fn from_raw_bytes(bytes: [u8; PERSISTENT_HASH_BYTES]) -> Self {
		Self(CoinPublicKey(HashOutput(bytes)))
	}
}

impl From<&EncodedCoinPublic> for Vec<u8> {
	fn from(value: &EncodedCoinPublic) -> Self {
		let mut bytes = Vec::new();
		<CoinPublicKey as Serializable>::serialize(&value.0, &mut bytes)
			.expect("failed to serialize contract address");
		bytes
	}
}

impl TryFrom<Vec<u8>> for EncodedCoinPublic {
	type Error = String;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		let coin_public = <CoinPublicKey as Deserializable>::deserialize(&mut &value[..], 0)
			.map_err(|e| format!("failed deserializing coin public key: {e}"))?;
		Ok(EncodedCoinPublic(coin_public))
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedZswapLocalState {
	#[serde(with = "bytes")]
	pub coin_public_key: EncodedCoinPublic,
	#[serde(with = "string")]
	pub current_index: u64,
	pub inputs: Vec<EncodedQualifiedShieldedCoinInfo>,
	pub outputs: Vec<EncodedOutput>,
}

impl EncodedZswapLocalState {
	pub fn from_zswap_state<D: DB>(value: WalletState<D>, coin_public: CoinPublicKey) -> Self {
		Self {
			coin_public_key: EncodedCoinPublic(coin_public),
			current_index: value.first_free,
			inputs: vec![],
			outputs: value
				.coins
				.iter()
				.map(|(_nullifier, c)| EncodedOutput {
					coin_info: EncodedShieldedCoinInfo {
						nonce: c.nonce.0.0,
						color: c.type_.0.0,
						value: c.value,
					},
					recipient: EncodedRecipient {
						is_left: true,
						left: EncodedCoinPublic(coin_public),
						right: EncodedContractAddress(ContractAddress::default()),
					},
				})
				.collect(),
		}
	}
}

mod string {
	use std::fmt::Display;
	use std::str::FromStr;

	use serde::{Deserialize, Deserializer, Serializer, de};

	pub fn serialize<T, S>(value: &T, serializer: S) -> Result<S::Ok, S::Error>
	where
		T: Display,
		S: Serializer,
	{
		serializer.collect_str(value)
	}

	pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
	where
		T: FromStr,
		T::Err: Display,
		D: Deserializer<'de>,
	{
		String::deserialize(deserializer)?.parse().map_err(de::Error::custom)
	}
}

mod bytes {
	use core::fmt::Display;
	use serde::{Deserialize, Deserializer, Serializer, de, ser::SerializeMap};

	#[derive(Deserialize)]
	pub struct BytesSerDe {
		bytes: Vec<u8>,
	}

	pub fn serialize<T, S>(value: T, serializer: S) -> Result<S::Ok, S::Error>
	where
		T: Into<Vec<u8>>,
		S: Serializer,
	{
		let value_bytes: Vec<u8> = value.into();
		let mut map = serializer.serialize_map(Some(1))?;
		map.serialize_entry("bytes", &value_bytes)?;
		map.end()
	}

	pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
	where
		T: TryFrom<Vec<u8>>,
		T::Error: Display,
		D: Deserializer<'de>,
	{
		let bytes_struct = BytesSerDe::deserialize(deserializer)?;
		bytes_struct.bytes.try_into().map_err(de::Error::custom)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_node_ledger_helpers::coin_structure::coin::Nullifier;
	use midnight_node_ledger_helpers::{
		CoinPublicKey, DefaultDB, HashOutput, Nonce, PERSISTENT_HASH_BYTES, QualifiedInfo,
		ShieldedTokenType, WalletState,
	};

	fn make_test_state()
	-> (WalletState<DefaultDB>, [u8; PERSISTENT_HASH_BYTES], [u8; PERSISTENT_HASH_BYTES]) {
		let nonce_bytes = [0xAA_u8; PERSISTENT_HASH_BYTES];
		let nullifier_bytes = [0xBB_u8; PERSISTENT_HASH_BYTES];

		let nonce = Nonce(HashOutput(nonce_bytes));
		let nullifier = Nullifier(HashOutput(nullifier_bytes));
		let coin = QualifiedInfo {
			nonce,
			type_: ShieldedTokenType(HashOutput([0xCC_u8; PERSISTENT_HASH_BYTES])),
			value: 42,
			mt_index: 0,
		};

		let state = WalletState::<DefaultDB>::new();
		let coins = state.coins.insert(nullifier, coin);
		let state = WalletState { coins, ..state };

		(state, nonce_bytes, nullifier_bytes)
	}

	#[test]
	fn from_zswap_state_uses_coin_nonce_not_nullifier() {
		let (state, nonce_bytes, _nullifier_bytes) = make_test_state();
		let coin_public = CoinPublicKey(HashOutput([0u8; PERSISTENT_HASH_BYTES]));

		let encoded = EncodedZswapLocalState::from_zswap_state(state, coin_public);

		assert_eq!(encoded.outputs.len(), 1);
		assert_eq!(
			encoded.outputs[0].coin_info.nonce, nonce_bytes,
			"serialized nonce must match the coin value's Nonce, not the map key Nullifier"
		);
	}

	#[test]
	fn from_zswap_state_preserves_color_and_value() {
		let (state, _nonce_bytes, _nullifier_bytes) = make_test_state();
		let coin_public = CoinPublicKey(HashOutput([0u8; PERSISTENT_HASH_BYTES]));

		let encoded = EncodedZswapLocalState::from_zswap_state(state, coin_public);

		assert_eq!(encoded.outputs[0].coin_info.color, [0xCC_u8; PERSISTENT_HASH_BYTES]);
		assert_eq!(encoded.outputs[0].coin_info.value, 42);
	}

	#[test]
	fn from_zswap_state_handles_empty_wallet() {
		let state = WalletState::<DefaultDB>::new();
		let coin_public = CoinPublicKey(HashOutput([0u8; PERSISTENT_HASH_BYTES]));

		let encoded = EncodedZswapLocalState::from_zswap_state(state, coin_public);

		assert!(encoded.outputs.is_empty());
		assert!(encoded.inputs.is_empty());
		assert_eq!(encoded.current_index, 0);
	}
}
