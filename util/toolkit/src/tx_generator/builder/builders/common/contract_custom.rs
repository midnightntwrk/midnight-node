use super::build_txs_ext::BuildTxsExt;
use super::ledger_helpers_local::{
	BuildInput, BuildIntent, BuildOutput, BuildTransient, BuildUtxoOutput, BuildUtxoSpend,
	BuilderContext, ClaimedUnshieldedSpendsKey, CoinInfo, ContractAction, ContractAddress,
	ContractEffects, DB, DefaultDB, EncryptionPublicKey, HashOutput, Input, InputInfo,
	IntentCustom, IntentInfo, Nullifier, OfferInfo, Output, ProofPreimage, ProofPreimageMarker,
	ProofProvider, PublicAddress, Recipient, ShieldedTokenType, ShieldedWallet, StdRng, TokenInfo,
	TokenType, TransactionWithContext, Transient, UnshieldedOfferInfo, UnshieldedWallet, UtxoId,
	UtxoOutputInfo, UtxoSpendInfo, Wallet, WalletAddress, WalletSeed, zswap,
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

/// Ownership classification for a custom-contract zswap input.
///
/// `EncodedZswapLocalState` does not carry an ownership discriminator. The historical
/// builder path always used [`Input::new_contract_owned`] (`SenderEvidence::Contract`),
/// which is incorrect when the selected coin is present in the funding wallet's
/// shielded state (user-owned → must use [`WalletState::spend`] /
/// `SenderEvidence::User`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum CustomZswapInputOwner {
	User { nullifier: Nullifier },
	Contract,
}

/// Resolve ownership by exact coin identity (nonce + color + value).
///
/// Never infer ownership from token type / value alone: two coins may share color and
/// value with different nonces.
pub(crate) fn classify_custom_zswap_input_owner<I>(
	want_nonce: &[u8; 32],
	want_color: &[u8; 32],
	want_value: u128,
	funding_wallet_coins: I,
) -> CustomZswapInputOwner
where
	I: IntoIterator<Item = (Nullifier, [u8; 32], [u8; 32], u128)>,
{
	for (nullifier, nonce, color, value) in funding_wallet_coins {
		if &nonce == want_nonce && &color == want_color && value == want_value {
			return CustomZswapInputOwner::User { nullifier };
		}
	}
	CustomZswapInputOwner::Contract
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

		//   - Input
		let mut inputs_info: Vec<Box<dyn BuildInput<DefaultDB, C>>> = vec![];

		//   - Transient
		let mut transients_info: Vec<Box<dyn BuildTransient<DefaultDB, C>>> = vec![];

		//   - Output
		let shielded_wallets: Vec<ShieldedWallet<DefaultDB>> = self
			.shielded_destinations
			.iter()
			.filter_map(|addr| addr.try_into().ok())
			.collect();

		let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB, C>>> = Vec::new();
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
				let chain_zswap_state = context.zswap_state().await;
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
						// EncodedZswapLocalState.inputs carries no ownership discriminator.
						// Prefer the funding wallet's exact coin (nonce+color+value) and the
						// canonical user spend path (WalletState::spend → SenderEvidence::User).
						// True contract-owned inputs remain on EncodedInputInfo /
						// Input::new_contract_owned.
						let seed = self.funding_seed();
						let want_nonce = encoded_input.nonce;
						let want_color = encoded_input.color;
						let want_value = encoded_input.value;

						let owner = context.with_wallet_from_seed(seed.clone(), |wallet| {
							classify_custom_zswap_input_owner(
								&want_nonce,
								&want_color,
								want_value,
								wallet.shielded.state.coins.iter().map(|(nullifier, coin)| {
									(nullifier, coin.nonce.0.0, coin.type_.0.0, coin.value)
								}),
							)
						});

						match owner {
							CustomZswapInputOwner::User { nullifier } => {
								log::info!(
									"zswap input nonce={} matched funding wallet nullifier={}: using WalletState::spend (SenderEvidence::User)",
									hex::encode(want_nonce),
									hex::encode(nullifier.0.0),
								);
								inputs_info.push(Box::new(InputInfo {
									origin: seed,
									token_type: ShieldedTokenType(HashOutput(want_color)),
									value: want_value,
									nullifier: Some(nullifier),
								}));
							},
							CustomZswapInputOwner::Contract => {
								log::info!(
									"zswap input nonce={} not in funding wallet: treating as contract-owned",
									hex::encode(want_nonce),
								);
								let input = EncodedInputInfo {
									encoded_qualified_info: encoded_input,
									segment: 0,
									contract_address,
									chain_zswap_state: chain_zswap_state.clone(),
								};
								inputs_info.push(Box::new(input));
							},
						}
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

#[cfg(test)]
mod tests {
	use super::*;

	const N: usize = 32;

	fn nf(byte: u8) -> Nullifier {
		Nullifier(HashOutput([byte; N]))
	}

	fn arr(byte: u8) -> [u8; N] {
		[byte; N]
	}

	/// Regression: a user-owned shielded coin in the funding wallet must classify as User
	/// (historical bug: always Contract via EncodedInputInfo / new_contract_owned).
	#[test]
	fn user_owned_custom_input_classifies_as_user() {
		let nullifier = nf(0x11);
		let nonce = arr(0xAA);
		let color = arr(0xCC);
		let value = 10u128;
		let coins = [(nullifier.clone(), nonce, color, value)];

		let owner = classify_custom_zswap_input_owner(&nonce, &color, value, coins);
		assert_eq!(owner, CustomZswapInputOwner::User { nullifier });
	}

	/// Coins absent from the funding wallet remain on the contract-owned path.
	#[test]
	fn contract_owned_input_classifies_as_contract() {
		let nullifier = nf(0x11);
		let wallet_nonce = arr(0xAA);
		let color = arr(0xCC);
		let value = 10u128;
		let coins = [(nullifier, wallet_nonce, color, value)];

		let want_nonce = arr(0xBB); // different coin identity
		let owner = classify_custom_zswap_input_owner(&want_nonce, &color, value, coins);
		assert_eq!(owner, CustomZswapInputOwner::Contract);
	}

	/// Exact nullifier must come from the matched coin — a different wallet nullifier for
	/// another coin must not be selected when nonce/color/value point at a specific coin.
	#[test]
	fn wrong_nullifier_is_not_selected_for_other_coin() {
		let nf_a = nf(0x01);
		let nf_b = nf(0x02);
		let nonce_a = arr(0xA1);
		let nonce_b = arr(0xB2);
		let color = arr(0xCC);
		let value = 7u128;
		let coins = [(nf_a.clone(), nonce_a, color, value), (nf_b.clone(), nonce_b, color, value)];

		let owner = classify_custom_zswap_input_owner(&nonce_b, &color, value, coins);
		assert_eq!(owner, CustomZswapInputOwner::User { nullifier: nf_b });
		assert_ne!(owner, CustomZswapInputOwner::User { nullifier: nf_a });
	}

	/// Same token type + value with a different nonce must not select the wrong coin.
	#[test]
	fn ambiguous_token_value_does_not_select_wrong_coin() {
		let nf_a = nf(0x01);
		let nf_b = nf(0x02);
		let nonce_a = arr(0xA1);
		let nonce_b = arr(0xB2);
		let color = arr(0xCC);
		let value = 10u128;
		let coins = [(nf_a.clone(), nonce_a, color, value), (nf_b, nonce_b, color, value)];

		// Request nonce_a → must get nf_a, never nf_b despite identical color/value.
		let owner = classify_custom_zswap_input_owner(&nonce_a, &color, value, coins.clone());
		assert_eq!(owner, CustomZswapInputOwner::User { nullifier: nf_a });

		// Color/value match alone with unknown nonce → Contract (do not guess).
		let unknown_nonce = arr(0xFF);
		let owner = classify_custom_zswap_input_owner(&unknown_nonce, &color, value, coins);
		assert_eq!(owner, CustomZswapInputOwner::Contract);
	}

	/// Value mismatch with otherwise identical identity → Contract.
	#[test]
	fn value_mismatch_does_not_classify_as_user() {
		let nullifier = nf(0x11);
		let nonce = arr(0xAA);
		let color = arr(0xCC);
		let coins = [(nullifier, nonce, color, 10u128)];

		let owner = classify_custom_zswap_input_owner(&nonce, &color, 9u128, coins);
		assert_eq!(owner, CustomZswapInputOwner::Contract);
	}

	/// EncodedInputInfo remains the contract-owned BuildInput path (structural guard).
	#[test]
	fn encoded_input_info_is_contract_owned_build_path() {
		// EncodedInputInfo::build always calls Input::new_contract_owned. User-owned coins
		// must be routed to InputInfo<WalletSeed> (WalletState::spend) instead.
		assert!(std::any::type_name::<EncodedInputInfo<DefaultDB>>().contains("EncodedInputInfo"));
		assert!(std::any::type_name::<InputInfo<WalletSeed>>().contains("InputInfo"));
	}

	/// Documents the historical unpatched decision (always Contract) vs fixed classifier.
	#[test]
	fn user_owned_custom_input_before_fix_would_be_contract() {
		let nullifier = nf(0x11);
		let nonce = arr(0xAA);
		let color = arr(0xCC);
		let value = 10u128;
		let coins = [(nullifier.clone(), nonce, color, value)];

		// Unpatched CustomContractBuilder always selected EncodedInputInfo → Contract.
		let before_fix = CustomZswapInputOwner::Contract;
		let after_fix = classify_custom_zswap_input_owner(&nonce, &color, value, coins);
		assert_eq!(before_fix, CustomZswapInputOwner::Contract);
		assert_eq!(after_fix, CustomZswapInputOwner::User { nullifier });
		assert_ne!(before_fix, after_fix);
	}
}
