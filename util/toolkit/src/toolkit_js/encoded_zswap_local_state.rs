use midnight_node_ledger_helpers::{CoinPublicKey, ContractAddress, DB, HashOutput, WalletState};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodedQualifiedShieldedCoinInfo {
	nonce: Vec<u8>,
	color: Vec<u8>,
	#[serde(with = "string")]
	value: u128,
	#[serde(with = "string")]
	mt_index: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodedShieldedCoinInfo {
	nonce: Vec<u8>,
	color: Vec<u8>,
	#[serde(with = "string")]
	value: u128,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedOutput {
	coin_info: EncodedShieldedCoinInfo,
	#[serde(with = "bytes")]
	recipient: EncodedRecipient,
}

/// Either a coin public key if the recipient is a user, or a contract address
#[derive(Clone, Debug)]
pub enum EncodedRecipient {
	CoinPublicKey(CoinPublicKey),
	ContractAddress(ContractAddress),
}

#[derive(Clone, Debug, thiserror::Error)]
pub enum InvalidRecipient {
	#[error("byte repr invalid")]
	InvalidBytes(Vec<u8>),
}

impl TryFrom<Vec<u8>> for EncodedRecipient {
	type Error = InvalidRecipient;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		match value[0] {
			0 => Ok(EncodedRecipient::CoinPublicKey(CoinPublicKey(HashOutput(
				value[1..].try_into().map_err(|_| InvalidRecipient::InvalidBytes(value))?,
			)))),
			1 => Ok(EncodedRecipient::ContractAddress(ContractAddress(HashOutput(
				value[1..].try_into().map_err(|_| InvalidRecipient::InvalidBytes(value))?,
			)))),
			_ => Err(InvalidRecipient::InvalidBytes(value)),
		}
	}
}

impl From<&EncodedRecipient> for Vec<u8> {
	fn from(value: &EncodedRecipient) -> Self {
		let mut bytes = Vec::new();
		match value {
			EncodedRecipient::CoinPublicKey(public_key) => {
				bytes.push(0);
				bytes.extend(&public_key.0.0);
			},
			EncodedRecipient::ContractAddress(contract_address) => {
				bytes.push(1);
				bytes.extend(&contract_address.0.0);
			},
		}
		bytes
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedZswapLocalState {
	coin_public_key: Vec<u8>,
	#[serde(with = "string")]
	current_index: u64,
	inputs: Vec<EncodedQualifiedShieldedCoinInfo>,
	outputs: Vec<EncodedOutput>,
}

impl EncodedZswapLocalState {
	pub fn from_zswap_state<D: DB>(value: WalletState<D>, coin_public: CoinPublicKey) -> Self {
		Self {
			coin_public_key: coin_public.0.0.to_vec(),
			current_index: value.first_free,
			inputs: vec![],
			outputs: value
				.coins
				.iter()
				.map(|(nullifier, c)| EncodedOutput {
					coin_info: EncodedShieldedCoinInfo {
						nonce: nullifier.0.0.to_vec(),
						color: c.type_.0.0.to_vec(),
						value: c.value,
					},
					recipient: EncodedRecipient::CoinPublicKey(coin_public),
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
	use serde::{Deserialize, Deserializer, Serializer, de};

	pub fn serialize<T, S>(value: T, serializer: S) -> Result<S::Ok, S::Error>
	where
		T: Into<Vec<u8>>,
		S: Serializer,
	{
		let value_bytes: Vec<u8> = value.into();
		serializer.serialize_bytes(&value_bytes)
	}

	pub fn deserialize<'de, T, D>(deserializer: D) -> Result<T, D::Error>
	where
		T: TryFrom<Vec<u8>>,
		T::Error: Display,
		D: Deserializer<'de>,
	{
		Vec::<u8>::deserialize(deserializer)?.try_into().map_err(de::Error::custom)
	}
}
