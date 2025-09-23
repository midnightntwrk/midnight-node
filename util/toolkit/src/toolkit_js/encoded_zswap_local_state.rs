use std::sync::Arc;

use midnight_node_ledger_helpers::{
	BuildOutput, CoinInfo, CoinPublicKey, ContractAddress, DB, HashOutput, LedgerContext, Nonce,
	Output, PERSISTENT_HASH_BYTES, ProofPreimage, Segment, ShieldedTokenType, TokenInfo,
	WalletState,
};
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
	nonce: [u8; PERSISTENT_HASH_BYTES],
	color: [u8; PERSISTENT_HASH_BYTES],
	#[serde(with = "string")]
	value: u128,
}

impl<D: DB + Clone> BuildOutput<D> for EncodedOutputInfo {
	fn build(
		&self,
		rng: &mut rand::prelude::StdRng,
		_context: Arc<LedgerContext<D>>,
	) -> Output<ProofPreimage, D> {
		let coin_info = CoinInfo {
			nonce: Nonce(HashOutput(self.encoded_output.coin_info.nonce)),
			type_: ShieldedTokenType(HashOutput(self.encoded_output.coin_info.color)),
			value: self.encoded_output.coin_info.value,
		};

		if self.encoded_output.recipient.is_left {
			Output::new(rng, &coin_info, self.segment, &self.encoded_output.recipient.left.0, None)
				.expect("failed to construct output")
		} else {
			Output::new_contract_owned(
				rng,
				&coin_info,
				self.segment,
				self.encoded_output.recipient.right.0,
			)
			.expect("failed to construct output")
		}
	}
}

pub struct EncodedOutputInfo {
	pub encoded_output: EncodedOutput,
	pub segment: u16,
}

impl TokenInfo for EncodedOutputInfo {
	fn token_type(&self) -> ShieldedTokenType {
		ShieldedTokenType(HashOutput(self.encoded_output.coin_info.color))
	}

	fn value(&self) -> u128 {
		self.encoded_output.coin_info.value
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedOutput {
	coin_info: EncodedShieldedCoinInfo,
	recipient: EncodedRecipient,
}

/// Either a coin public key if the recipient is a user, or a contract address
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodedRecipient {
	is_left: bool,
	#[serde(with = "bytes")]
	left: EncodedCoinPublic,
	#[serde(with = "bytes")]
	right: EncodedContractAddress,
}

#[derive(Debug, Clone)]
pub struct EncodedContractAddress(ContractAddress);

impl From<&EncodedContractAddress> for Vec<u8> {
	fn from(value: &EncodedContractAddress) -> Self {
		value.0.0.0.to_vec()
	}
}

impl TryFrom<Vec<u8>> for EncodedContractAddress {
	type Error = String;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Ok(EncodedContractAddress(ContractAddress(HashOutput(
			value.try_into().map_err(|_| "failed to convert to coin_public".to_string())?,
		))))
	}
}

#[derive(Debug, Clone)]
pub struct EncodedCoinPublic(CoinPublicKey);

impl From<&EncodedCoinPublic> for Vec<u8> {
	fn from(value: &EncodedCoinPublic) -> Self {
		value.0.0.0.to_vec()
	}
}

impl TryFrom<Vec<u8>> for EncodedCoinPublic {
	type Error = String;

	fn try_from(value: Vec<u8>) -> Result<Self, Self::Error> {
		Ok(EncodedCoinPublic(CoinPublicKey(HashOutput(
			value.try_into().map_err(|_| "failed to convert to coin_public".to_string())?,
		))))
	}
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncodedZswapLocalState {
	pub coin_public_key: Vec<u8>,
	#[serde(with = "string")]
	pub current_index: u64,
	pub inputs: Vec<EncodedQualifiedShieldedCoinInfo>,
	pub outputs: Vec<EncodedOutput>,
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
						nonce: nullifier.0.0,
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
