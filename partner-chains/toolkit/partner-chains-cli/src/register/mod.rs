use crate::cardano_key;
use crate::config::config_fields;
use crate::io::IOContext;
use crate::keystore::CROSS_CHAIN;
use crate::ogmios::config::prompt_ogmios_configuration;
use crate::ogmios::get_shelley_config;
use crate::select_utxo::{query_utxos, select_from_utxos};
use anyhow::anyhow;
use ogmios_client::query_ledger_state::{QueryLedgerState, QueryUtxoByUtxoId};
use ogmios_client::query_network::QueryNetwork;
use ogmios_client::transactions::Transactions;
use partner_chains_cardano_offchain::await_tx::FixedDelayRetries;
use partner_chains_cardano_offchain::cardano_keys::CardanoPaymentSigningKey;
use partner_chains_cardano_offchain::csl::NetworkTypeExt;
use partner_chains_cardano_offchain::register::run_register;
use plutus_datum_derive::ToDatum;
use secp256k1::PublicKey;
use sidechain_domain::crypto::sc_public_key_and_signature_for_datum;
use sidechain_domain::*;
use sp_core::{Pair, ecdsa};
use std::{fmt::Display, str::FromStr};

use crate::cmd_traits::Register;

pub mod register1;
pub mod register2;
pub mod register3;

#[derive(Clone, Debug, ToDatum)]
pub struct RegisterValidatorMessage {
	pub genesis_utxo: UtxoId,
	pub sidechain_pub_key: SidechainPublicKey,
	pub registration_utxo: UtxoId,
}

#[derive(Clone, Debug)]
pub struct PartnerChainPublicKeyParam(pub SidechainPublicKey);

impl Display for PartnerChainPublicKeyParam {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "0x{}", hex::encode(&self.0.0))
	}
}

impl FromStr for PartnerChainPublicKeyParam {
	type Err = secp256k1::Error;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let trimmed = s.trim_start_matches("0x");
		let pk = PublicKey::from_str(trimmed)?;
		Ok(PartnerChainPublicKeyParam(SidechainPublicKey(pk.serialize().to_vec())))
	}
}

#[derive(Clone, Debug)]
pub struct CandidateKeyParam(pub CandidateKey);

impl CandidateKeyParam {
	pub(crate) fn new(id: [u8; 4], bytes: Vec<u8>) -> Self {
		Self(CandidateKey { id, bytes })
	}

	fn try_new_from(id: &str, bytes: Vec<u8>) -> anyhow::Result<Self> {
		let id = id
			.bytes()
			.collect::<Vec<u8>>()
			.try_into()
			.map_err(|_| anyhow::anyhow!("Incorrect key type length, must be 4"))?;
		Ok(Self::new(id, bytes))
	}
}

impl FromStr for CandidateKeyParam {
	type Err = Box<dyn std::error::Error + Send + Sync>;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Ok(Self(CandidateKey::from_str(s)?))
	}
}

impl ToString for CandidateKeyParam {
	fn to_string(&self) -> String {
		format!("{}:{}", String::from_utf8_lossy(&self.0.id), hex::encode(&self.0.bytes))
	}
}

impl From<CandidateKeyParam> for CandidateKey {
	fn from(value: CandidateKeyParam) -> Self {
		value.0
	}
}

#[derive(Clone, Debug)]
pub struct StakePoolSigningKeyParam(pub ed25519_zebra::SigningKey);

impl From<[u8; 32]> for StakePoolSigningKeyParam {
	fn from(key: [u8; 32]) -> Self {
		Self(ed25519_zebra::SigningKey::from(key))
	}
}

/// Reads the cross-chain (partner chain) ECDSA key pair from its seed-phrase file in the
/// node keystore.
pub(crate) fn get_ecdsa_pair_from_file<C: IOContext>(
	context: &C,
	keystore_path: &str,
	sidechain_pub_key: &str,
) -> Result<ecdsa::Pair, anyhow::Error> {
	let mut seed_phrase_file_name = CROSS_CHAIN.key_type_hex();
	seed_phrase_file_name.push_str(sidechain_pub_key.replace("0x", "").as_str());
	let seed_phrase_file_path = format!("{keystore_path}/{seed_phrase_file_name}");
	let seed = context
		.read_file(&seed_phrase_file_path)
		.ok_or_else(|| anyhow::anyhow!("seed phrase file {seed_phrase_file_path} not found"))?;
	let stripped_quotes = seed.trim_matches('\"');
	Ok(ecdsa::Pair::from_string(stripped_quotes, None)?)
}

/// Signs the registration message with the cross-chain (partner chain) ECDSA key.
pub(crate) fn sign_registration_message_with_sidechain_key(
	message: RegisterValidatorMessage,
	ecdsa_pair: ecdsa::Pair,
) -> Result<String, anyhow::Error> {
	let seed = ecdsa_pair.seed();
	let secret_key = secp256k1::SecretKey::from_slice(&seed).map_err(|e| anyhow!(e))?;
	let (_, sig) = sc_public_key_and_signature_for_datum(secret_key, message.clone());
	Ok(hex::encode(sig.serialize_compact()))
}

/// Derives the bech32 Cardano payment address from the payment verification key file
/// configured in the resources config (prompting for it if not yet configured).
pub(crate) fn derive_address<C: IOContext>(
	context: &C,
	cardano_network: NetworkType,
) -> Result<String, anyhow::Error> {
	let cardano_payment_verification_key_file =
		config_fields::CARDANO_PAYMENT_VERIFICATION_KEY_FILE
			.prompt_with_default_from_file_and_save(context);
	let key_bytes: [u8; 32] = cardano_key::get_payment_verification_key_bytes_from_file(
		&cardano_payment_verification_key_file,
		context,
	)?;
	let address =
		partner_chains_cardano_offchain::csl::payment_address(&key_bytes, cardano_network.to_csl());
	address.to_bech32(None).map_err(|e| anyhow!(e.to_string()))
}

/// Queries the UTXOs at the user's payment address via Ogmios and lets the user choose the
/// one to be consumed by the registration transaction.
pub(crate) fn select_registration_utxo<C: IOContext>(context: &C) -> anyhow::Result<UtxoId> {
	context.print("This wizard will query your UTXOs using address derived from the payment verification key and Ogmios service");
	let ogmios_configuration = prompt_ogmios_configuration(context)?;
	let shelley_genesis_config = get_shelley_config(&ogmios_configuration, context)?;
	let address = derive_address(context, shelley_genesis_config.network)?;
	let utxo_query_result = query_utxos(context, &ogmios_configuration, &address)?;

	if utxo_query_result.is_empty() {
		context.eprint("⚠️ No UTXOs found for the given address");
		context.eprint(
			"The registering transaction requires at least one UTXO to be present at the address.",
		);
		return Err(anyhow::anyhow!("No UTXOs found"));
	};

	let registration_utxo: UtxoId =
		select_from_utxos(context, "Select UTXO to use for registration", utxo_query_result)?;

	context.print(
		"Please do not spend this UTXO, it needs to be consumed by the registration transaction.",
	);
	context.print("");

	Ok(registration_utxo)
}

impl<T> Register for T
where
	T: QueryLedgerState + Transactions + QueryNetwork + QueryUtxoByUtxoId,
{
	async fn register(
		&self,
		await_tx: FixedDelayRetries,
		genesis_utxo: UtxoId,
		candidate_registration: &CandidateRegistration,
		payment_signing_key: &CardanoPaymentSigningKey,
	) -> Result<Option<McTxHash>, String> {
		run_register(genesis_utxo, candidate_registration, payment_signing_key, self, await_tx)
			.await
			.map_err(|e| e.to_string())
	}
}
