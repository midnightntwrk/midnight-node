use std::sync::Arc;

use midnight_node_ledger_helpers::{
	BuildOutput, CoinInfo, CoinPublicKey, ContractAddress, DB, Deserializable, EncryptionPublicKey,
	HashOutput, LedgerContext, Nonce, Output, PERSISTENT_HASH_BYTES, ProofPreimage, Recipient,
	Serializable, ShieldedTokenType, ShieldedWallet, TokenInfo, WalletState,
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

		println!("coin_info: {coin_info:?}");
		let recipient: Recipient = self.encoded_output.recipient.clone().into();

		match recipient {
			Recipient::User(public_key) => {
				Output::new(rng, &coin_info, self.segment, &public_key, self.encryption_public_key)
					.expect("failed to construct output")
			},
			Recipient::Contract(contract_address) => {
				Output::new_contract_owned(rng, &coin_info, self.segment, contract_address)
					.expect("failed to construct output")
			},
		}
	}
}

pub struct EncodedOutputInfo {
	pub encoded_output: EncodedOutput,
	pub segment: u16,
	pub encryption_public_key: Option<EncryptionPublicKey>,
}

impl EncodedOutputInfo {
	/// Create a new EncodedOutputInfo, searching for a matching encryption public key from
	/// possible destinations
	pub fn new<D: DB + Clone>(
		encoded_output: EncodedOutput,
		segment: u16,
		possible_destinations: &[ShieldedWallet<D>],
	) -> Self {
		let mut encryption_public_key = None;
		let recipient: Recipient = encoded_output.recipient.clone().into();
		if let Recipient::User(ref public_key) = recipient {
			if let Some(wallet) =
				possible_destinations.iter().find(|w| w.coin_public_key == *public_key)
			{
				encryption_public_key = Some(wallet.enc_public_key);
			}
		}

		Self { encoded_output, segment, encryption_public_key }
	}
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

impl From<EncodedRecipient> for Recipient {
	fn from(value: EncodedRecipient) -> Self {
		if value.is_left {
			Recipient::User(value.left.0)
		} else {
			Recipient::Contract(value.right.0)
		}
	}
}

#[derive(Debug, Clone)]
pub struct EncodedContractAddress(ContractAddress);

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
pub struct EncodedCoinPublic(CoinPublicKey);

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
