use super::build_txs_ext::BuildTxsExt;
use crate::{
	serde_def::SourceTransactions,
	toolkit_js::{
		EncodedZswapLocalState,
		encoded_zswap_local_state::{EncodedOutput, EncodedQualifiedShieldedCoinInfo},
	},
	tx_generator::builder::{BuildTxs, CustomContractArgs},
};
use async_trait::async_trait;
use ledger_helpers_local::coin_structure::{
	coin::Commitment as CoinCommitment, transfer::SenderEvidence,
};
use ledger_helpers_local::{
	BuildInput, BuildIntent, BuildOutput, BuildTransient, BuildUtxoOutput, BuildUtxoSpend,
	BuilderContext, ClaimedUnshieldedSpendsKey, CoinInfo, ContractAction, ContractAddress,
	ContractEffects, DB, DefaultDB, EncryptionPublicKey, HashOutput, Input, InputInfo,
	IntentCustom, IntentInfo, Nullifier, OfferInfo, Output, ProofPreimage, ProofPreimageMarker,
	ProofProvider, PublicAddress, Recipient, Segment, ShieldedCoinSelectionError,
	ShieldedTokenType, ShieldedWallet, StdRng, TokenInfo, TokenType, TransactionWithContext,
	Transient, UnshieldedOfferInfo, UnshieldedWallet, UtxoId, UtxoOutputInfo, UtxoSpendInfo,
	Wallet, WalletAddress, WalletSeed, zswap,
};
use midnight_ledger_unsafe_helpers::ledger_9 as ledger_helpers_local;
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;
use rand::SeedableRng;
use std::{
	cmp::Ordering,
	collections::{BTreeMap, BTreeSet, HashMap, HashSet},
	sync::Arc,
};

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

fn add_shielded_token_value(
	totals: &mut BTreeMap<ShieldedTokenType, u128>,
	token_type: ShieldedTokenType,
	value: u128,
) -> Result<(), CustomContractBuilderError> {
	let total = totals.entry(token_type).or_insert(0);
	*total = total
		.checked_add(value)
		.ok_or(CustomContractBuilderError::ShieldedBalanceOverflow)?;
	Ok(())
}

const GUARANTEED_SEGMENT: u16 = Segment::Guaranteed as u16;

struct SegmentedEffects {
	segment: u16,
	address: ContractAddress,
	effects: ContractEffects<DefaultDB>,
}

fn call_effects(intent: &IntentCustom<DefaultDB>, fallible_segment: u16) -> Vec<SegmentedEffects> {
	let mut effects = Vec::new();
	for action in intent.intent.actions.iter() {
		if let ContractAction::Call(call) = &*action {
			for (segment, transcript) in [
				(GUARANTEED_SEGMENT, &call.guaranteed_transcript),
				(fallible_segment, &call.fallible_transcript),
			] {
				if let Some(transcript) = transcript {
					effects.push(SegmentedEffects {
						segment,
						address: call.address,
						effects: transcript.effects.clone(),
					});
				}
			}
		}
	}
	effects
}

fn empty_offer<C: BuilderContext<DefaultDB>>() -> OfferInfo<DefaultDB, C> {
	OfferInfo { inputs: Vec::new(), outputs: Vec::new(), transients: Vec::new() }
}

fn claims_commitment(effects: &ContractEffects<DefaultDB>, commitment: CoinCommitment) -> bool {
	effects.claimed_shielded_receives.iter().any(|claimed| **claimed == commitment)
		|| effects.claimed_shielded_spends.iter().any(|claimed| **claimed == commitment)
}

fn claims_nullifier(effects: &ContractEffects<DefaultDB>, nullifier: Nullifier) -> bool {
	effects.claimed_nullifiers.iter().any(|claimed| **claimed == nullifier)
}

/// Resolves a spent coin's owner from the transcript that claims its nullifier.
///
/// A bundled intent can call several contracts; deriving every nullifier with the first
/// contract address would make later contract-owned spends invalid.
fn input_owner(
	effects: &[SegmentedEffects],
	coin_info: &CoinInfo,
) -> Option<(ContractAddress, Nullifier)> {
	effects.iter().find_map(|effect| {
		let nullifier = coin_info.nullifier(&SenderEvidence::Contract(effect.address));
		claims_nullifier(&effect.effects, nullifier).then_some((effect.address, nullifier))
	})
}

/// Selects funding coins for one segment, excluding coins selected for earlier segments.
///
/// The wallet advances only while building offers, so selection must reserve nullifiers to
/// prevent independent segment balances from choosing the same coin. The ordering matches
/// `CoinSelectionStrategy::LargestFirst`.
fn select_funding_coins<C: BuilderContext<DefaultDB>>(
	context: &Arc<C>,
	seed: &WalletSeed,
	required: u128,
	token_type: ShieldedTokenType,
	reserved: &mut HashSet<Nullifier>,
) -> Result<(Vec<InputInfo<WalletSeed>>, u128), CustomContractBuilderError> {
	let mut available: Vec<InputInfo<WalletSeed>> =
		context.with_wallet_from_seed(seed.clone(), |wallet| {
			wallet
				.shielded
				.state
				.coins
				.iter()
				.filter(|(nullifier, coin)| {
					coin.type_ == token_type && !reserved.contains(nullifier)
				})
				.map(|(nullifier, coin)| InputInfo {
					origin: seed.clone(),
					token_type,
					value: coin.value,
					nullifier: Some(nullifier),
				})
				.collect()
		});
	available.sort_by_key(|input| std::cmp::Reverse(input.value));

	let mut total: u128 = 0;
	let mut selected = Vec::new();
	for input in available {
		total = total
			.checked_add(input.value)
			.ok_or(CustomContractBuilderError::ShieldedBalanceOverflow)?;
		if let Some(nullifier) = input.nullifier {
			reserved.insert(nullifier);
		}
		selected.push(input);
		if let Some(change) = total.checked_sub(required) {
			return Ok((selected, change));
		}
	}
	Err(CustomContractBuilderError::ShieldedCoinSelection(
		ShieldedCoinSelectionError::InsufficientBalance {
			required,
			token_type,
			seed: seed.clone(),
		},
	))
}

/// Matches the ledger's claim-based offer partitioning.
fn shielded_segment<F>(effects: &[SegmentedEffects], claims: F) -> u16
where
	F: Fn(&ContractEffects<DefaultDB>) -> bool,
{
	effects
		.iter()
		.find(|effect| effect.segment != GUARANTEED_SEGMENT && claims(&effect.effects))
		.map_or(GUARANTEED_SEGMENT, |effect| effect.segment)
}

#[derive(Clone, Copy)]
enum Imbalance {
	Shortfall(u128),
	Surplus(u128),
}

fn shielded_imbalances<C: BuilderContext<DefaultDB>>(
	segment: u16,
	outputs: &[Box<dyn BuildOutput<DefaultDB, C>>],
	inputs: &[Box<dyn BuildInput<DefaultDB, C>>],
	effects: &[SegmentedEffects],
) -> Result<Vec<(ShieldedTokenType, Imbalance)>, CustomContractBuilderError> {
	let mut owed = BTreeMap::new();
	for output in outputs {
		add_shielded_token_value(&mut owed, output.token_type(), output.value())?;
	}

	let mut covered = BTreeMap::new();
	for input in inputs {
		add_shielded_token_value(&mut covered, input.token_type(), input.value())?;
	}
	// Mints only cover outputs in their own segment.
	for effect in effects.iter().filter(|e| e.segment == segment) {
		for entry in effect.effects.shielded_mints.iter() {
			let (domain_sep, value) = &*entry;
			add_shielded_token_value(
				&mut covered,
				effect.address.custom_shielded_token_type(**domain_sep),
				u128::from(**value),
			)?;
		}
	}

	Ok(owed
		.keys()
		.chain(covered.keys())
		.copied()
		.collect::<BTreeSet<_>>()
		.into_iter()
		.filter_map(|token_type| {
			let owed = owed.get(&token_type).copied().unwrap_or(0);
			let covered = covered.get(&token_type).copied().unwrap_or(0);
			match owed.cmp(&covered) {
				Ordering::Equal => None,
				Ordering::Greater => Some((token_type, Imbalance::Shortfall(owed - covered))),
				Ordering::Less => Some((token_type, Imbalance::Surplus(covered - owed))),
			}
		})
		.collect())
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
	#[error("arithmetic overflow while balancing the shielded offer")]
	ShieldedBalanceOverflow,
	#[error("failed to select shielded coins to fund the contract call")]
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

		let guaranteed_outputs = find_outputs(guaranteed_effects)?;
		if !guaranteed_outputs.is_empty() || !guaranteed_inputs.is_empty() {
			guaranteed_unshielded_offer_info = Some(UnshieldedOfferInfo {
				inputs: guaranteed_inputs,
				outputs: guaranteed_outputs,
			});
		}

		let fallible_outputs = find_outputs(fallible_effects)?;
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

		let mut shielded_wallets: Vec<ShieldedWallet<DefaultDB>> = self
			.shielded_destinations
			.iter()
			.filter_map(|addr| addr.try_into().ok())
			.collect();
		shielded_wallets.push(ShieldedWallet::default(self.funding_seed()));

		let effects = call_effects(&contract_intent, contract_segment);
		let mut offers: BTreeMap<u16, OfferInfo<DefaultDB, C>> = BTreeMap::new();
		let mut encoded_output_infos: HashMap<CoinInfo, Box<EncodedOutputInfo>> = HashMap::new();

		if let Some(zswap_state) = zswap_state {
			for encoded_output in zswap_state.outputs {
				let coin_info: CoinInfo = (&encoded_output).into();
				let recipient: Recipient = (&encoded_output.recipient).into();
				let commitment = coin_info.commitment(&recipient);
				let segment = shielded_segment(&effects, |e| claims_commitment(e, commitment));
				let encoded_output_info =
					EncodedOutputInfo::new(encoded_output, segment, &shielded_wallets);
				encoded_output_infos.insert(coin_info, Box::new(encoded_output_info));
			}

			if !zswap_state.inputs.is_empty() {
				// Only a fallback: `input_owner` resolves the real owner per input below.
				let fallback_address = contract_intent
					.find_contract_address()
					.expect("Contract address should be set");
				let chain_zswap_state = context.zswap_state().await;
				for encoded_input in zswap_state.inputs {
					let coin_info: CoinInfo = (&encoded_input).into();
					let (owner, nullifier) =
						input_owner(&effects, &coin_info).unwrap_or_else(|| {
							(
								fallback_address,
								coin_info.nullifier(&SenderEvidence::Contract(fallback_address)),
							)
						});

					if let Some(mut encoded_output_info) = encoded_output_infos.remove(&coin_info) {
						// A transient is fallible if either its commitment or nullifier is claimed.
						let recipient: Recipient =
							(&encoded_output_info.encoded_output.recipient).into();
						let commitment = coin_info.commitment(&recipient);
						let segment = shielded_segment(&effects, |e| {
							claims_commitment(e, commitment) || claims_nullifier(e, nullifier)
						});
						encoded_output_info.segment = segment;
						offers.entry(segment).or_insert_with(empty_offer).transients.push(
							Box::new(EncodedTransientInfo {
								encoded_qualified_info: encoded_input,
								segment,
								encoded_output_info,
							}),
						);
					} else {
						let segment =
							shielded_segment(&effects, |e| claims_nullifier(e, nullifier));
						offers.entry(segment).or_insert_with(empty_offer).inputs.push(Box::new(
							EncodedInputInfo {
								encoded_qualified_info: encoded_input,
								segment,
								contract_address: owner,
								chain_zswap_state: chain_zswap_state.clone(),
							},
						));
					}
				}
			}

			for encoded_output_info in encoded_output_infos.into_values() {
				let segment = encoded_output_info.segment;
				offers
					.entry(segment)
					.or_insert_with(empty_offer)
					.outputs
					.push(encoded_output_info);
			}
		}

		let balancing_segments: BTreeSet<u16> = offers
			.keys()
			.copied()
			.chain(
				effects
					.iter()
					.filter(|effect| !effect.effects.shielded_mints.is_empty())
					.map(|effect| effect.segment),
			)
			.collect();

		// Coins picked for one segment must not be picked again for another.
		let mut reserved_funding: HashSet<Nullifier> = HashSet::new();
		for segment in balancing_segments {
			let offer = offers.entry(segment).or_insert_with(empty_offer);
			let imbalances = shielded_imbalances(segment, &offer.outputs, &offer.inputs, &effects)?;
			for (token_type, imbalance) in imbalances {
				let change = match imbalance {
					Imbalance::Shortfall(required) => {
						let (funding_inputs, change) = select_funding_coins(
							&self.context,
							&self.funding_seed(),
							required,
							token_type,
							&mut reserved_funding,
						)?;
						let offer = offers.entry(segment).or_insert_with(empty_offer);
						for funding_input in funding_inputs {
							offer.inputs.push(Box::new(SegmentedFundingInputInfo {
								input: funding_input,
								segment,
							}));
						}
						change
					},
					Imbalance::Surplus(excess) => excess,
				};
				if change > 0 {
					offers.entry(segment).or_insert_with(empty_offer).outputs.push(Box::new(
						SegmentedFundingOutputInfo {
							destination: self.funding_seed(),
							token_type,
							value: change,
							segment,
						},
					));
				}
			}
		}

		offers.retain(|_, offer| {
			!offer.inputs.is_empty() || !offer.outputs.is_empty() || !offer.transients.is_empty()
		});
		if let Some(guaranteed_offer) = offers.remove(&GUARANTEED_SEGMENT) {
			tx_info.set_guaranteed_offer(guaranteed_offer);
		}
		if !offers.is_empty() {
			tx_info.set_fallible_offers(offers.into_iter().collect());
		}

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
/// A funding-wallet input assigned to the transcript segment that requires it.
struct SegmentedFundingInputInfo {
	input: InputInfo<WalletSeed>,
	segment: u16,
}

impl TokenInfo for SegmentedFundingInputInfo {
	fn token_type(&self) -> ShieldedTokenType {
		self.input.token_type()
	}

	fn value(&self) -> u128 {
		self.input.value()
	}
}

impl<D: DB + Clone, C: BuilderContext<D>> BuildInput<D, C> for SegmentedFundingInputInfo {
	fn build(
		&mut self,
		rng: &mut rand::prelude::StdRng,
		context: Arc<C>,
	) -> Input<ProofPreimage, D> {
		context.with_wallet_from_seed(self.input.origin.clone(), |wallet| {
			let coin = self.input.min_match_coin(&wallet.shielded.state);
			self.input.value = coin.value;

			let (updated_wallet, input) = wallet
				.shielded
				.state
				.spend(rng, wallet.shielded.secret_keys(), &coin, Some(self.segment))
				.expect("failed to spend funding coin");
			wallet.shielded.state = updated_wallet;
			input
		})
	}
}

/// A funding-wallet change output assigned to the transcript segment that produced it.
struct SegmentedFundingOutputInfo {
	destination: WalletSeed,
	token_type: ShieldedTokenType,
	value: u128,
	segment: u16,
}

impl TokenInfo for SegmentedFundingOutputInfo {
	fn token_type(&self) -> ShieldedTokenType {
		self.token_type
	}

	fn value(&self) -> u128 {
		self.value
	}
}

impl<D: DB + Clone, C: BuilderContext<D>> BuildOutput<D, C> for SegmentedFundingOutputInfo {
	fn build(&self, rng: &mut rand::prelude::StdRng, context: Arc<C>) -> Output<ProofPreimage, D> {
		context.with_wallet_from_seed(self.destination.clone(), |wallet| {
			let coin_info = CoinInfo::new(rng, self.value, self.token_type);
			wallet.shielded.state = wallet
				.shielded
				.state
				.watch_for(&wallet.shielded.secret_keys().coin_public_key(), &coin_info);

			Output::new(
				rng,
				&coin_info,
				Some(self.segment),
				&wallet.shielded.secret_keys().coin_public_key(),
				Some(wallet.shielded.secret_keys().enc_public_key()),
			)
			.expect("failed to construct funding change output")
		})
	}
}
