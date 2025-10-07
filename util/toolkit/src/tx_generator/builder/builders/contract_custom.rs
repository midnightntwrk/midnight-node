use crate::{
	builder::{
		BuildInput, BuildIntent, BuildOutput, BuildTxs, BuildTxsExt, CustomContractArgs, DefaultDB,
		DeserializedTransactionsWithContext, IntentCustom, OfferInfo, ProofProvider, ProofType,
		SignatureType, TransactionWithContext, Wallet, WalletSeed,
	},
	serde_def::SourceTransactions,
	toolkit_js::{EncodedOutputInfo, EncodedZswapLocalState},
};
use async_trait::async_trait;
use midnight_node_ledger_helpers::{
	Array, ClaimedUnshieldedSpendsKey, ContractAction, ProofPreimageMarker, PublicAddress,
	ShieldedWallet, StdRng, TokenType, UnshieldedOffer, UtxoOutput, WalletAddress, default_storage,
};
use rand::SeedableRng;
use std::{collections::HashMap, sync::Arc};

#[derive(Debug, thiserror::Error)]
pub enum CustomContractBuilderError {
	#[error("failed to read zswap state file")]
	FailedReadingZswapStateFile(std::io::Error),
	#[error("failed to parse zswap state")]
	FailedParsingZswapState(serde_json::Error),
	#[error("failed to prove tx")]
	FailedProvingTx(Box<dyn std::error::Error + Send + Sync>),
	#[error("failed to read intent file")]
	FailedReadingIntent(std::io::Error),
	#[error("ClaimedUnshieldedSpendsKey contains non-unshielded token type")]
	ClaimedUnshieldedSpendTokenTypeError(TokenType),
}

pub struct CustomContractBuilder {
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	artifacts_dir: String,
	intent_files: Vec<String>,
	zswap_state_file: Option<String>,
	shielded_destinations: Vec<WalletAddress>,
}

impl CustomContractBuilder {
	pub fn new(args: CustomContractArgs) -> Self {
		Self {
			funding_seed: args.info.funding_seed,
			rng_seed: args.info.rng_seed,
			artifacts_dir: args.compiled_contract_dir,
			intent_files: args.intent_files,
			zswap_state_file: args.zswap_state_file,
			shielded_destinations: args.shielded_destinations,
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
}

impl CustomContractBuilder {
	fn build_intent(&self) -> Result<IntentCustom<DefaultDB>, CustomContractBuilderError> {
		let mut rng = self.rng_seed.map(StdRng::from_seed).unwrap_or(StdRng::from_entropy());
		println!("Create intent info for contract custom");
		// This is to satisfy the `&'static` need to update the context's resolver
		// Data lives for the remainder of the program's life.
		let boxed_resolver =
			Box::new(IntentCustom::<DefaultDB>::get_resolver(self.artifacts_dir.clone()).unwrap());
		let static_ref_resolver = Box::leak(boxed_resolver);

		let mut actions: Vec<ContractAction<ProofPreimageMarker, DefaultDB>> = vec![];
		for intent in &self.intent_files {
			let custom_intent = IntentCustom::new_from_file(intent, static_ref_resolver)
				.map_err(CustomContractBuilderError::FailedReadingIntent)?;
			actions.extend(custom_intent.intent.actions.iter().map(|c| (*c).clone()));
		}

		let custom_intent =
			IntentCustom::new_from_actions(&mut rng, &actions[..], static_ref_resolver);

		println!("custom_intent: {:?}", custom_intent.intent);
		Ok(custom_intent)
	}

	fn read_zswap_file(
		&self,
	) -> Result<Option<EncodedZswapLocalState>, CustomContractBuilderError> {
		if let Some(file_path) = &self.zswap_state_file {
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
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		println!("Building Txs for CustomContract");
		// - LedgerContext and TransactionInfo
		let (_context, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

		// Use segment 1 for the custom contract
		let contract_segment = 1;

		// - Intents
		let mut contract_intent = self.build_intent()?;
		let zswap_state = self.read_zswap_file()?;

		let (guaranteed_effects, _fallible_effects) = contract_intent.find_effects();

		if let Some(effects) = guaranteed_effects {
			let mut outputs = Array::<UtxoOutput>::new();
			for (ClaimedUnshieldedSpendsKey(tt, dest), value) in effects.claimed_unshielded_spends {
				let TokenType::Unshielded(tt) = tt else {
					return Err(CustomContractBuilderError::ClaimedUnshieldedSpendTokenTypeError(
						tt,
					));
				};

				if let PublicAddress::User(addr) = dest {
					outputs = outputs.push(UtxoOutput { value, owner: addr, type_: tt });
				}
			}

			let unshielded_offer = default_storage().arena.alloc(UnshieldedOffer {
				inputs: Default::default(),
				outputs,
				signatures: Default::default(),
			});

			contract_intent.intent.guaranteed_unshielded_offer = Some(unshielded_offer);
		}

		let mut intents: HashMap<u16, Box<dyn BuildIntent<DefaultDB>>> = HashMap::new();
		intents.insert(contract_segment, Box::new(contract_intent));
		tx_info.set_intents(intents);

		//   - Input
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB>>> = vec![];

		//   - Output
		let shielded_wallets: Vec<ShieldedWallet<DefaultDB>> = self
			.shielded_destinations
			.iter()
			.filter_map(|addr| addr.try_into().ok())
			.collect();
		let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB>>> = Vec::new();
		if let Some(zswap_state) = zswap_state {
			for encoded_output in zswap_state.outputs.into_iter() {
				// NOTE: Using segment 0 here assumes that the contract is executing a guaranteed
				// transcript
				outputs_info.push(Box::new(EncodedOutputInfo::new(
					encoded_output,
					0,
					&shielded_wallets,
				)));
			}
		}

		let offer_info =
			OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] };

		tx_info.set_guaranteed_offer(offer_info);

		tx_info.set_wallet_seeds(vec![self.funding_seed()]);
		tx_info.use_mock_proofs_for_fees(false);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await.map_err(CustomContractBuilderError::FailedProvingTx)?;

		#[cfg(feature = "erase-proof")]
		let tx = tx_info
			.erase_proof()
			.await
			.map_err(CustomContractBuilderError::FailedProvingTx)?;

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] })
	}
}
