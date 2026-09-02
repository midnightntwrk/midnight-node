use super::build_txs_ext::BuildTxsExt;
use super::ledger_helpers_local::{
	BuildInput, BuildIntent, BuildOutput, BuildTransient, BuildUtxoOutput, BuildUtxoSpend,
	BuilderContext, ClaimedUnshieldedSpendsKey, CoinInfo, CoinSelectionStrategy, ContractAction,
	ContractAddress, ContractEffects, DB, DefaultDB, EncryptionPublicKey, HashOutput, Input,
	InputInfo, IntentCustom, IntentInfo, Nullifier, OfferInfo, Output, OutputInfo, ProofPreimage,
	ProofPreimageMarker, ProofProvider, PublicAddress, Recipient, ShieldedCoinSelectionError,
	ShieldedTokenType, ShieldedWallet, StdRng, TokenInfo, TokenType, TransactionWithContext,
	Transient, UnshieldedOfferInfo, UnshieldedWallet, UtxoId, UtxoOutputInfo, UtxoSpendInfo,
	Wallet, WalletAddress, WalletSeed, coin_structure::transfer::SenderEvidence, zswap,
};
use crate::{
	serde_def::SourceTransactions,
	toolkit_js::{
		EncodedZswapLocalState,
		encoded_zswap_local_state::{EncodedOutput, EncodedQualifiedShieldedCoinInfo},
	},
	tx_generator::builder::{BuildTxs, CustomContractArgs},
};
use async_trait::async_trait;
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use rand::SeedableRng;
use std::{collections::HashMap, sync::Arc};

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

impl<D: DB + Clone, C: BuilderContext<D>> BuildOutput<D, C> for EncodedOutputInfo {
	fn build(&self, rng: &mut rand::prelude::StdRng, _context: Arc<C>) -> Output<ProofPreimage, D> {
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

pub struct EncodedTransientInfo<D: DB + Clone, C: BuilderContext<D>> {
	pub encoded_qualified_info: EncodedQualifiedShieldedCoinInfo,
	pub segment: u16,
	pub encoded_output_info: Box<dyn BuildOutput<D, C>>,
}

impl<D: DB + Clone, C: BuilderContext<D>> BuildTransient<D, C> for EncodedTransientInfo<D, C> {
	fn build(
		&self,
		rng: &mut rand::prelude::StdRng,
		context: Arc<C>,
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

impl<D: DB + Clone, C: BuilderContext<D>> BuildInput<D, C> for EncodedInputInfo<D> {
	fn build(
		&mut self,
		rng: &mut rand::prelude::StdRng,
		_context: Arc<C>,
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

/// A funding-wallet coin spent in a given segment. `InputInfo` always spends in the guaranteed
/// segment, which the ledger's Pedersen check rejects for coins placed in a fallible offer.
pub struct WalletInputInfo {
	pub origin: WalletSeed,
	pub token_type: ShieldedTokenType,
	pub value: u128,
	pub nullifier: Nullifier,
	pub segment: u16,
}

impl TokenInfo for WalletInputInfo {
	fn token_type(&self) -> ShieldedTokenType {
		self.token_type
	}

	fn value(&self) -> u128 {
		self.value
	}
}

impl<D: DB + Clone, C: BuilderContext<D>> BuildInput<D, C> for WalletInputInfo {
	fn build(&mut self, rng: &mut StdRng, context: Arc<C>) -> Input<ProofPreimage, D> {
		context.with_wallet_from_seed(self.origin.clone(), |wallet| {
			let coin = wallet
				.shielded
				.state
				.coins
				.iter()
				.find(|(nullifier, _)| *nullifier == self.nullifier)
				.map(|(_, coin)| coin)
				.expect("selected wallet coin disappeared");
			let (updated_wallet, input) = wallet
				.shielded
				.state
				.spend(rng, wallet.shielded.secret_keys(), &coin, Some(self.segment))
				.expect("Failed to spend coin");
			wallet.shielded.state = updated_wallet;
			input
		})
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
	#[error("failed to fund shielded outputs from the funding wallet")]
	ShieldedCoinSelection(#[from] ShieldedCoinSelectionError),
}

pub struct CustomContractBuilder<C: BuilderContext<DefaultDB>> {
	context: Arc<C>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	artifact_dirs: Vec<String>,
	intent_files: Vec<String>,
	utxo_inputs: Vec<UtxoId>,
	zswap_state_file: Option<String>,
	shielded_destinations: Vec<WalletAddress>,
}

impl<C: BuilderContext<DefaultDB>> CustomContractBuilder<C> {
	pub fn new(
		args: CustomContractArgs,
		context: Arc<C>,
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

impl<C: BuilderContext<DefaultDB>> BuildTxsExt<C> for CustomContractBuilder<C> {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}

	fn context(&self) -> &Arc<C> {
		&self.context
	}

	fn prover(&self) -> &Arc<dyn ProofProvider<DefaultDB>> {
		&self.prover
	}
}

impl<C: BuilderContext<DefaultDB>> CustomContractBuilder<C> {
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
impl<C: BuilderContext<DefaultDB>> BuildTxs for CustomContractBuilder<C> {
	type Error = CustomContractBuilderError;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		log::info!("Building Txs for CustomContract");

		// - LedgerContext and TransactionInfo
		let (context, mut tx_info) = self.context_and_tx_info();

		let funding_utxos: Vec<_> = context
			.unshielded_utxos(self.funding_seed())
			.await
			.into_iter()
			.map(|(utxo, _ctime)| utxo)
			.collect();

		// Use segment 1 for the custom contract
		let contract_segment = 1;

		// - Intents
		let contract_intent = self.build_intent()?;
		let zswap_state = self.read_zswap_file()?;
		let (guaranteed_effects, fallible_effects) = contract_intent.find_effects();

		let mut guaranteed_unshielded_offer_info: Option<UnshieldedOfferInfo<DefaultDB, C>> = None;
		let mut fallible_unshielded_offer_info: Option<UnshieldedOfferInfo<DefaultDB, C>> = None;
		let find_outputs = |effects_vec: Vec<ContractEffects<DefaultDB>>| -> Result<
			Vec<Box<dyn BuildUtxoOutput<DefaultDB, C>>>,
			CustomContractBuilderError,
		> {
			let mut outputs = Vec::<Box<dyn BuildUtxoOutput<DefaultDB, C>>>::new();
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

		let mut guaranteed_inputs = Vec::<Box<dyn BuildUtxoSpend<DefaultDB, C>>>::new();
		let mut fallible_inputs = Vec::<Box<dyn BuildUtxoSpend<DefaultDB, C>>>::new();
		let fallible_effects_unshielded_inputs = fallible_effects
			.iter()
			.flat_map(|effects| effects.unshielded_inputs.clone())
			.collect::<Vec<_>>();
		for input_utxo in &self.utxo_inputs {
			let funding_match = funding_utxos
				.iter()
				.find(|u| {
					u.intent_hash == input_utxo.intent_hash
						&& u.output_no == input_utxo.output_number
				})
				.ok_or(CustomContractBuilderError::FailedToFindMatchingUtxo(*input_utxo))?;

			let input = Box::new(UtxoSpendInfo {
				value: funding_match.value,
				owner: self.funding_seed(),
				token_type: funding_match.type_,
				intent_hash: Some(funding_match.intent_hash),
				output_number: Some(funding_match.output_no),
			});

			if fallible_effects_unshielded_inputs
				.contains(&(TokenType::Unshielded(funding_match.type_), funding_match.value))
			{
				fallible_inputs.push(input);
			} else {
				guaranteed_inputs.push(input);
			}
		}

		let guaranteed_outputs = find_outputs(guaranteed_effects.clone())?;
		if !guaranteed_outputs.is_empty() || !guaranteed_inputs.is_empty() {
			guaranteed_unshielded_offer_info = Some(UnshieldedOfferInfo {
				inputs: guaranteed_inputs,
				outputs: guaranteed_outputs,
			});
		}

		let fallible_outputs = find_outputs(fallible_effects.clone())?;
		if !fallible_outputs.is_empty() || !fallible_inputs.is_empty() {
			fallible_unshielded_offer_info =
				Some(UnshieldedOfferInfo { inputs: fallible_inputs, outputs: fallible_outputs });
		}

		let mut intents: HashMap<u16, Box<dyn BuildIntent<DefaultDB, C>>> = HashMap::new();

		intents.insert(
			contract_segment,
			Box::new(IntentInfo {
				guaranteed_unshielded_offer: guaranteed_unshielded_offer_info,
				fallible_unshielded_offer: fallible_unshielded_offer_info,
				actions: vec![Box::new(contract_intent.clone())],
			}),
		);

		tx_info.set_intents(intents);

		// The ledger matches claims and balances tokens per segment, so each coin goes into the
		// offer of the transcript that claimed it: guaranteed -> 0, fallible -> contract segment.
		let shielded_wallets: Vec<ShieldedWallet<DefaultDB>> = self
			.shielded_destinations
			.iter()
			.filter_map(|addr| addr.try_into().ok())
			.collect();
		let claimed_in =
			|effects: &[ContractEffects<DefaultDB>],
			 claim: &dyn Fn(&ContractEffects<DefaultDB>) -> bool| { effects.iter().any(claim) };
		let claim_segment = |claim: &dyn Fn(&ContractEffects<DefaultDB>) -> bool| -> Option<u16> {
			if claimed_in(&guaranteed_effects, claim) {
				Some(0)
			} else if claimed_in(&fallible_effects, claim) {
				Some(contract_segment)
			} else {
				None
			}
		};

		let mut shielded_balance: HashMap<(u16, ShieldedTokenType), i128> = HashMap::new();
		let contract_address = contract_intent.find_contract_address();
		if let Some(address) = contract_address {
			for (segment, effects) in
				[(0, &guaranteed_effects), (contract_segment, &fallible_effects)]
			{
				for effect in effects {
					for (domain_sep, value) in effect.shielded_mints.clone() {
						*shielded_balance
							.entry((segment, address.custom_shielded_token_type(domain_sep)))
							.or_default() += value as i128;
					}
				}
			}
		}

		let empty_offer = || OfferInfo { inputs: vec![], outputs: vec![], transients: vec![] };
		let mut offers: HashMap<u16, OfferInfo<DefaultDB, C>> = HashMap::new();
		if let Some(zswap_state) = zswap_state {
			let mut pending_outputs: HashMap<CoinInfo, EncodedOutputInfo> = HashMap::new();
			for encoded_output in zswap_state.outputs.into_iter() {
				let coin_info: CoinInfo = (&encoded_output).into();
				let recipient: Recipient = (&encoded_output.recipient).into();
				let commitment = coin_info.commitment(&recipient);
				let claim = |effects: &ContractEffects<DefaultDB>| match recipient {
					Recipient::Contract(_) => effects.claimed_shielded_receives.member(&commitment),
					Recipient::User(_) => effects.claimed_shielded_spends.member(&commitment),
				};
				// An unclaimed user output is a mint: it belongs where its token was minted.
				let segment = claim_segment(&claim).unwrap_or_else(|| {
					if shielded_balance.contains_key(&(contract_segment, coin_info.type_)) {
						contract_segment
					} else {
						0
					}
				});
				pending_outputs.insert(
					coin_info,
					EncodedOutputInfo::new(encoded_output, segment, &shielded_wallets),
				);
			}

			if !zswap_state.inputs.is_empty() {
				let contract_address = contract_address.expect("Contract address should be set");
				let chain_zswap_state = context.zswap_state().await;
				for encoded_input in zswap_state.inputs.into_iter() {
					let coin_info: CoinInfo = (&encoded_input).into();
					let nullifier =
						coin_info.nullifier(&SenderEvidence::Contract(contract_address));
					let segment =
						claim_segment(&|effects| effects.claimed_nullifiers.member(&nullifier))
							.unwrap_or(0);
					let offer = offers.entry(segment).or_insert_with(empty_offer);
					if let Some(mut output) = pending_outputs.remove(&coin_info) {
						output.segment = segment;
						offer.transients.push(Box::new(EncodedTransientInfo {
							encoded_qualified_info: encoded_input,
							segment,
							encoded_output_info: Box::new(output),
						}));
					} else {
						offer.inputs.push(Box::new(EncodedInputInfo {
							encoded_qualified_info: encoded_input,
							segment,
							contract_address,
							chain_zswap_state: chain_zswap_state.clone(),
						}));
					}
				}
			}

			for output in pending_outputs.into_values() {
				offers
					.entry(output.segment)
					.or_insert_with(empty_offer)
					.outputs
					.push(Box::new(output));
			}
		}

		// Whatever the call takes from the caller (`receiveShielded`) is funded from the wallet in
		// the same segment, with change back.
		for (segment, offer) in &offers {
			for output in &offer.outputs {
				*shielded_balance.entry((*segment, output.token_type())).or_default() -=
					output.value() as i128;
			}
			for input in &offer.inputs {
				*shielded_balance.entry((*segment, input.token_type())).or_default() +=
					input.value() as i128;
			}
		}
		for ((segment, token_type), balance) in shielded_balance {
			if balance >= 0 {
				continue;
			}
			let (wallet_inputs, change) = InputInfo::coins_to_cover_value(
				context.clone(),
				self.funding_seed(),
				balance.unsigned_abs(),
				token_type,
				CoinSelectionStrategy::default(),
			)?;
			let offer = offers.entry(segment).or_insert_with(empty_offer);
			for input in wallet_inputs {
				offer.inputs.push(Box::new(WalletInputInfo {
					origin: input.origin,
					token_type: input.token_type,
					value: input.value,
					nullifier: input.nullifier.expect("wallet coin has a nullifier"),
					segment,
				}));
			}
			if change > 0 {
				offer.outputs.push(Box::new(OutputInfo {
					destination: self.funding_seed(),
					token_type,
					value: change,
				}));
			}
		}

		tx_info.set_guaranteed_offer(offers.remove(&0).unwrap_or_else(empty_offer));
		tx_info.set_fallible_offers(offers);

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
