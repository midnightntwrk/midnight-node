use super::build_txs_ext::BuildTxsExt;
use super::ledger_helpers_local::{
	BuildInput, BuildIntent, BuildOutput, BuildTransient, BuildUtxoOutput, BuildUtxoSpend,
	ClaimedUnshieldedSpendsKey, CoinInfo, CoinPublicKey, ContractAction, ContractAddress,
	ContractEffects, DB, DefaultDB, Deserializable, EncryptionPublicKey, HashOutput, Input,
	IntentCustom, IntentInfo, LedgerContext, Nonce, OfferInfo, Output, ProofPreimage,
	ProofPreimageMarker, ProofProvider, PublicAddress, QualifiedInfo, Recipient, ShieldedTokenType,
	ShieldedWallet, StdRng, TokenInfo, TokenType, TransactionWithContext, Transient,
	UnshieldedOfferInfo, UnshieldedWallet, UtxoId, UtxoOutputInfo, UtxoSpendInfo, Wallet,
	WalletAddress, WalletSeed, zswap,
};
use crate::{
	serde_def::SourceTransactions,
	toolkit_js::{
		EncodedZswapLocalState,
		encoded_zswap_local_state::{
			EncodedOutput, EncodedQualifiedShieldedCoinInfo, EncodedRecipient,
		},
	},
	tx_generator::builder::{BuildTxs, CustomContractArgs},
};
use async_trait::async_trait;
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use rand::SeedableRng;
use std::{collections::HashMap, sync::Arc};

// --- Version-local conversion impls ---

impl From<&EncodedRecipient> for Recipient {
	fn from(value: &EncodedRecipient) -> Self {
		if value.is_left {
			let bytes = value.left.0.0.0;
			Recipient::User(CoinPublicKey(HashOutput(bytes)))
		} else {
			let mut serialized = Vec::new();
			midnight_node_ledger_helpers::Serializable::serialize(&value.right.0, &mut serialized)
				.expect("failed to serialize contract address");
			let contract_address =
				<ContractAddress as Deserializable>::deserialize(&mut &serialized[..], 0)
					.expect("failed to deserialize contract address");
			Recipient::Contract(contract_address)
		}
	}
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

// --- Version-local type definitions ---

#[derive(Clone)]
pub struct EncodedOutputInfo {
	pub encoded_output: EncodedOutput,
	pub segment: u16,
	pub encryption_public_key: Option<EncryptionPublicKey>,
}

impl EncodedOutputInfo {
	pub fn new(
		encoded_output: EncodedOutput,
		segment: u16,
		possible_destinations: &[ShieldedWallet<DefaultDB>],
	) -> Self {
		let mut encryption_public_key = None;
		let recipient: Recipient = (&encoded_output.recipient).into();
		if let Recipient::User(ref public_key) = recipient {
			if let Some(wallet) =
				possible_destinations.iter().find(|w| w.coin_public_key == *public_key)
			{
				encryption_public_key = Some(wallet.enc_public_key);
			} else {
				log::warn!(
					"warning: missing encryption_public_key for zswap output {} - output will be invisible to indexer",
					hex::encode(&encoded_output.coin_info.nonce)
				);
			}
		}

		Self { encoded_output, segment, encryption_public_key }
	}
}

impl<D: DB + Clone> BuildOutput<D> for EncodedOutputInfo {
	fn build(
		&self,
		rng: &mut rand::prelude::StdRng,
		_context: Arc<LedgerContext<D>>,
	) -> Output<ProofPreimage, D> {
		let coin_info: CoinInfo = (&self.encoded_output).into();
		let recipient: Recipient = (&self.encoded_output.recipient).into();

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

impl TokenInfo for EncodedOutputInfo {
	fn token_type(&self) -> ShieldedTokenType {
		ShieldedTokenType(HashOutput(self.encoded_output.coin_info.color))
	}

	fn value(&self) -> u128 {
		self.encoded_output.coin_info.value
	}
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
			&(&self.encoded_qualified_info).into(),
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
			&(&self.encoded_qualified_info).into(),
			Some(self.segment),
			self.contract_address,
			&self.chain_zswap_state.coin_coms,
		)
		.expect("Failed to construct Input")
	}
}

// --- Builder ---

#[derive(Debug, thiserror::Error)]
pub enum CustomContractBuilderError {
	#[error("failed to read zswap state file")]
	FailedReadingZswapStateFile(std::io::Error),
	#[error("failed to parse zswap state")]
	FailedParsingZswapState(serde_json::Error),
	#[error("failed to deserialize zswap state")]
	FailedDeserializingZswapState(String),
	#[error("failed to prove tx")]
	FailedProvingTx(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to read intent file")]
	FailedReadingIntent(std::io::Error),
	#[error("failed to find matching UTXO in wallet")]
	FailedToFindMatchingUtxo(UtxoId),
	#[error("ClaimedUnshieldedSpendsKey contains non-unshielded token type")]
	ClaimedUnshieldedSpendTokenTypeError(TokenType),
}

pub struct CustomContractBuilder {
	context: Arc<LedgerContext<DefaultDB>>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	artifact_dirs: Vec<String>,
	intent_files: Vec<String>,
	utxo_inputs: Vec<UtxoId>,
	zswap_state_file: Option<String>,
	shielded_destinations: Vec<WalletAddress>,
}

impl CustomContractBuilder {
	pub fn new(
		args: CustomContractArgs,
		context: Arc<LedgerContext<DefaultDB>>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		// Convert top-level types to version-local types via string representation
		let utxo_inputs: Vec<UtxoId> = args
			.utxo_inputs
			.iter()
			.map(|u| u.to_string().parse().expect("failed to convert UtxoId"))
			.collect();
		let shielded_destinations: Vec<WalletAddress> = args
			.shielded_destinations
			.iter()
			.map(|addr| addr.to_bech32().parse().expect("failed to convert WalletAddress"))
			.collect();
		Self {
			context,
			prover,
			funding_seed: args.funding_seed,
			rng_seed: args.rng_seed,
			artifact_dirs: args.compiled_contract_dirs,
			intent_files: args.intent_files,
			utxo_inputs,
			zswap_state_file: args.zswap_state_file,
			shielded_destinations,
		}
	}
}

impl BuildTxsExt for CustomContractBuilder {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}

	fn context(&self) -> &Arc<LedgerContext<DefaultDB>> {
		&self.context
	}

	fn prover(&self) -> &Arc<dyn ProofProvider<DefaultDB>> {
		&self.prover
	}
}

impl CustomContractBuilder {
	fn build_intent(&self) -> Result<IntentCustom<DefaultDB>, CustomContractBuilderError> {
		let mut rng = self.rng_seed.map(StdRng::from_seed).unwrap_or(StdRng::from_entropy());
		log::info!("Create intent info for contract custom");
		// This is to satisfy the `&'static` need to update the context's resolver
		// Data lives for the remainder of the program's life.
		let boxed_resolver = Box::new(
			IntentCustom::<DefaultDB>::get_resolver(&self.artifact_dirs)
				.map_err(CustomContractBuilderError::FailedReadingIntent)?,
		);
		let static_ref_resolver = Box::leak(boxed_resolver);

		let mut actions: Vec<ContractAction<ProofPreimageMarker, DefaultDB>> = vec![];
		for intent in &self.intent_files {
			let custom_intent = IntentCustom::new_from_file(intent, static_ref_resolver)
				.map_err(CustomContractBuilderError::FailedReadingIntent)?;
			actions.extend(custom_intent.intent.actions.iter().map(|c| (*c).clone()));
		}

		let custom_intent =
			IntentCustom::new_from_actions(&mut rng, &actions[..], static_ref_resolver);

		log::debug!("custom_intent: {:?}", custom_intent.intent);
		Ok(custom_intent)
	}

	fn read_zswap_file(
		&self,
	) -> Result<Option<EncodedZswapLocalState>, CustomContractBuilderError> {
		/// Maximum file size for zswap state files (64 MB)
		const MAX_ZSWAP_FILE_SIZE: u64 = 64 * 1024 * 1024;

		if let Some(file_path) = &self.zswap_state_file {
			let metadata = std::fs::metadata(file_path)
				.map_err(CustomContractBuilderError::FailedReadingZswapStateFile)?;
			if metadata.len() > MAX_ZSWAP_FILE_SIZE {
				return Err(CustomContractBuilderError::FailedReadingZswapStateFile(
					std::io::Error::new(
						std::io::ErrorKind::InvalidData,
						format!(
							"zswap state file exceeds maximum size of {} bytes",
							MAX_ZSWAP_FILE_SIZE
						),
					),
				));
			}
			let bytes = std::fs::read(file_path)
				.map_err(CustomContractBuilderError::FailedReadingZswapStateFile)?;
			let zswap_state = serde_json::from_slice(&bytes)
				.map_err(CustomContractBuilderError::FailedParsingZswapState)?;
			Ok(Some(zswap_state))
		} else {
			Ok(None)
		}
	}
}

#[async_trait]
impl BuildTxs for CustomContractBuilder {
	type Error = CustomContractBuilderError;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		log::info!("Building Txs for CustomContract");

		// - LedgerContext and TransactionInfo
		let (context, mut tx_info) = self.context_and_tx_info();

		let funding_utxos = context.with_ledger_state(|state| {
			context.with_wallet_from_seed(self.funding_seed(), |w| w.unshielded_utxos(&state))
		});

		let mut input_utxos = Vec::<Box<dyn BuildUtxoSpend<DefaultDB>>>::new();
		for input_utxo in &self.utxo_inputs {
			let funding_match = funding_utxos
				.iter()
				.find(|u| {
					u.intent_hash == input_utxo.intent_hash
						&& u.output_no == input_utxo.output_number
				})
				.ok_or(CustomContractBuilderError::FailedToFindMatchingUtxo(*input_utxo))?;

			input_utxos.push(Box::new(UtxoSpendInfo {
				value: funding_match.value,
				owner: self.funding_seed(),
				token_type: funding_match.type_,
				intent_hash: Some(funding_match.intent_hash),
				output_number: Some(funding_match.output_no),
			}));
		}

		// Use segment 1 for the custom contract
		let contract_segment = 1;

		// - Intents
		let contract_intent = self.build_intent()?;
		let zswap_state = self.read_zswap_file()?;
		let (guaranteed_effects, fallible_effects) = contract_intent.find_effects();

		let mut guaranteed_unshielded_offer_info: Option<UnshieldedOfferInfo<DefaultDB>> = None;
		let mut fallible_unshielded_offer_info: Option<UnshieldedOfferInfo<DefaultDB>> = None;
		let find_outputs = |effects_vec: Vec<ContractEffects<DefaultDB>>| -> Result<
			Vec<Box<dyn BuildUtxoOutput<DefaultDB>>>,
			CustomContractBuilderError,
		> {
			let mut outputs = Vec::<Box<dyn BuildUtxoOutput<DefaultDB>>>::new();
			for effects in effects_vec {
				for (ClaimedUnshieldedSpendsKey(tt, dest), value) in
					effects.claimed_unshielded_spends
				{
					let TokenType::Unshielded(tt) = tt else {
						return Err(
							CustomContractBuilderError::ClaimedUnshieldedSpendTokenTypeError(tt),
						);
					};

					if let PublicAddress::User(addr) = dest {
						let owner: UnshieldedWallet = addr.into();
						outputs.push(Box::new(UtxoOutputInfo { value, owner, token_type: tt }));
					}
				}
			}
			Ok(outputs)
		};

		let guaranteed_outputs = find_outputs(guaranteed_effects)?;
		if !guaranteed_outputs.is_empty() || !input_utxos.is_empty() {
			guaranteed_unshielded_offer_info =
				Some(UnshieldedOfferInfo { inputs: input_utxos, outputs: guaranteed_outputs });
		}

		let fallible_outputs = find_outputs(fallible_effects)?;
		if !fallible_outputs.is_empty() {
			fallible_unshielded_offer_info =
				Some(UnshieldedOfferInfo { inputs: vec![], outputs: fallible_outputs });
		}

		let mut intents: HashMap<u16, Box<dyn BuildIntent<DefaultDB>>> = HashMap::new();

		intents.insert(
			contract_segment,
			Box::new(IntentInfo {
				guaranteed_unshielded_offer: guaranteed_unshielded_offer_info,
				fallible_unshielded_offer: fallible_unshielded_offer_info,
				actions: vec![Box::new(contract_intent.clone())],
			}),
		);

		tx_info.set_intents(intents);

		//   - Input
		let mut inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> = vec![];

		//   - Transient
		let mut transients_info: Vec<Box<dyn BuildTransient<DefaultDB>>> = vec![];

		//   - Output
		let shielded_wallets: Vec<ShieldedWallet<DefaultDB>> = self
			.shielded_destinations
			.iter()
			.filter_map(|addr| addr.try_into().ok())
			.collect();

		let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>> = Vec::new();
		let mut encoded_output_infos: HashMap<CoinInfo, Box<EncodedOutputInfo>> = HashMap::new();

		if let Some(zswap_state) = zswap_state {
			for encoded_output in zswap_state.outputs.into_iter() {
				// NOTE: Using segment 0 here assumes that the contract is executing a guaranteed
				// transcript
				let coin_info: CoinInfo = (&encoded_output).into();
				let encoded_output_info =
					EncodedOutputInfo::new(encoded_output, 1, &shielded_wallets);
				encoded_output_infos.insert(coin_info, Box::new(encoded_output_info));
			}

			if !zswap_state.inputs.is_empty() {
				let contract_address = contract_intent
					.find_contract_address()
					.expect("Contract address should be set");
				let chain_zswap_state = context.with_ledger_state(|state| (*state.zswap).clone());
				for encoded_input in zswap_state.inputs.into_iter() {
					let coin_info: CoinInfo = (&encoded_input).into();

					if let Some(encoded_output_info) = encoded_output_infos.get(&coin_info) {
						let transient = EncodedTransientInfo {
							encoded_qualified_info: encoded_input,
							segment: 0,
							encoded_output_info: encoded_output_info.clone(),
						};
						transients_info.push(Box::new(transient));
						encoded_output_infos.remove(&coin_info);
					} else {
						let input = EncodedInputInfo {
							encoded_qualified_info: encoded_input,
							segment: 0,
							contract_address,
							chain_zswap_state: chain_zswap_state.clone(),
						};
						inputs_info.push(Box::new(input));
					}
				}
			}

			for encoded_output_info in encoded_output_infos.values() {
				outputs_info.push(encoded_output_info.clone());
			}
		}

		let offer_info =
			OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: transients_info };

		tx_info.set_guaranteed_offer(offer_info);

		tx_info.set_funding_seeds(vec![self.funding_seed()]);
		tx_info.use_mock_proofs_for_fees(false);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await.map_err(CustomContractBuilderError::FailedProvingTx)?;

		#[cfg(feature = "erase-proof")]
		let tx = tx_info
			.erase_proof()
			.await
			.map_err(CustomContractBuilderError::FailedProvingTx)?;

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(super::tx_serialization::build_single(tx_with_context))
	}
}
