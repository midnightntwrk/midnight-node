use std::sync::Arc;

use midnight_node_ledger_helpers::{
	BuildInput, BuildOutput, BuildTransient, CoinInfo, CoinPublicKey, ContractAddress, DB,
	Deserializable, EncryptionPublicKey, HashOutput, Input, LedgerContext, Nonce, Output,
	PERSISTENT_HASH_BYTES, ProofPreimage, QualifiedInfo, Recipient, Serializable,
	ShieldedTokenType, ShieldedWallet, TokenInfo, Transient, WalletState, zswap,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EncodedQualifiedShieldedCoinInfo {
	nonce: [u8; PERSISTENT_HASH_BYTES],
	color: [u8; PERSISTENT_HASH_BYTES],
	#[serde(with = "string")]
	value: u128,
	#[serde(with = "string")]
	mt_index: u64,
}

impl From<&EncodedQualifiedShieldedCoinInfo> for CoinInfo {
	fn from(value: &EncodedQualifiedShieldedCoinInfo) -> Self {
		CoinInfo {
			nonce: Nonce(HashOutput(value.nonce)),
			type_: ShieldedTokenType(HashOutput(value.color)),
			value: value.value,
		}
	}
}
impl From<&EncodedQualifiedShieldedCoinInfo> for QualifiedInfo {
	fn from(value: &EncodedQualifiedShieldedCoinInfo) -> Self {
		CoinInfo::from(value).qualify(value.mt_index)
	}
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EncodedShieldedCoinInfo {
	nonce: [u8; PERSISTENT_HASH_BYTES],
	color: [u8; PERSISTENT_HASH_BYTES],
	#[serde(with = "string")]
	value: u128,
}

impl From<&EncodedOutput> for CoinInfo {
	fn from(value: &EncodedOutput) -> Self {
		CoinInfo {
			nonce: Nonce(HashOutput(value.coin_info.nonce)),
			type_: ShieldedTokenType(HashOutput(value.coin_info.color)),
			value: value.coin_info.value,
		}
	}
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

		log::debug!("coin_info: {coin_info:?}");
		let recipient: Recipient = self.encoded_output.recipient.clone().into();

		match recipient {
			Recipient::User(public_key) => Output::new(
				rng,
				&coin_info,
				Some(self.segment),
				&public_key,
				self.encryption_public_key,
			)
			.expect("failed to construct output"),
			Recipient::Contract(contract_address) => {
				Output::new_contract_owned(rng, &coin_info, Some(self.segment), contract_address)
					.expect("failed to construct output")
			},
		}
	}
}

#[derive(Clone)]
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
			} else {
				log::warn!(
					"missing encryption_public_key for zswap output {} - output will be invisible to indexer",
					hex::encode(&encoded_output.coin_info.nonce)
				);
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

pub struct EncodedTransientInfo<D: DB + Clone> {
	pub encoded_qualified_info: EncodedQualifiedShieldedCoinInfo,
	pub segment: u16,
	pub encoded_output_info: Box<dyn BuildOutput<D>>,
}

impl<D: DB + Clone> BuildTransient<D> for EncodedTransientInfo<D> {
	fn build(
		&self,
		rng: &mut rand::prelude::StdRng,
		context: Arc<LedgerContext<D>>,
	) -> Transient<ProofPreimage, D> {
		let output = self.encoded_output_info.build(rng, context.clone());
		Transient::new_from_contract_owned_output(
			rng,
			&QualifiedInfo::from(&self.encoded_qualified_info),
			Some(self.segment),
			output,
		)
		.expect("Failed to construct Transient")
	}
}

pub struct EncodedInputInfo<D: DB + Clone> {
	pub encoded_qualified_info: EncodedQualifiedShieldedCoinInfo,
	pub segment: u16,
	pub contract_address: ContractAddress,
	pub chain_zswap_state: zswap::ledger::State<D>,
}

impl<D: DB + Clone> TokenInfo for EncodedInputInfo<D> {
	fn token_type(&self) -> ShieldedTokenType {
		ShieldedTokenType(HashOutput(self.encoded_qualified_info.color))
	}

	fn value(&self) -> u128 {
		self.encoded_qualified_info.value
	}
}

impl<D: DB + Clone> BuildInput<D> for EncodedInputInfo<D> {
	fn build(
		&mut self,
		rng: &mut rand::prelude::StdRng,
		_context: Arc<LedgerContext<D>>,
	) -> Input<ProofPreimage, D> {
		Input::new_contract_owned(
			rng,
			&QualifiedInfo::from(&self.encoded_qualified_info),
			Some(self.segment),
			self.contract_address,
			&self.chain_zswap_state.coin_coms,
		)
		.expect("Failed to construct Input")
	}
}

impl EncodedOutput {
	pub(crate) fn new(coin_info: EncodedShieldedCoinInfo, recipient: EncodedRecipient) -> Self {
		Self { coin_info, recipient }
	}
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

impl EncodedRecipient {
	pub(crate) fn user(coin_public: EncodedCoinPublic) -> Self {
		Self {
			is_left: true,
			left: coin_public,
			right: EncodedContractAddress(ContractAddress::default()),
		}
	}
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
