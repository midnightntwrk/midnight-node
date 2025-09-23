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
}

pub struct CustomContractBuilder {
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	artifacts_dir: String,
	intent_file: String,
	zswap_state_file: Option<String>,
}

impl CustomContractBuilder {
	pub fn new(args: CustomContractArgs) -> Self {
		Self {
			funding_seed: args.info.funding_seed,
			rng_seed: args.info.rng_seed,
			artifacts_dir: args.compiled_contract_dir,
			intent_file: args.intent_file,
			zswap_state_file: args.zswap_state_file,
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
		println!("Create intent info for contract custom");
		// This is to satisfy the `&'static` need to update the context's resolver
		// Data lives for the remainder of the program's life.
		let boxed_resolver =
			Box::new(IntentCustom::<DefaultDB>::get_resolver(self.artifacts_dir.clone()).unwrap());
		let static_ref_resolver = Box::leak(boxed_resolver);

		let custom_intent = IntentCustom::new_from_file(&self.intent_file, static_ref_resolver)
			.map_err(CustomContractBuilderError::FailedReadingIntent)?;

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
		let (_, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

		// Use segment 1 for the custom contract
		let contract_segment = 1;

		// - Intents
		let contract_intent = self.build_intent()?;
		let zswap_state = self.read_zswap_file()?;

		let mut intents: HashMap<u16, Box<dyn BuildIntent<DefaultDB> + Send>> = HashMap::new();
		intents.insert(contract_segment, Box::new(contract_intent));
		tx_info.set_intents(intents);

		//   - Input
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB> + Send>> = vec![];

		//   - Output
		let mut outputs_info: Vec<Box<dyn BuildOutput<DefaultDB> + Send>> = Vec::new();
		if let Some(zswap_state) = zswap_state {
			for encoded_output in zswap_state.outputs.into_iter() {
				outputs_info.push(Box::new(EncodedOutputInfo {
					encoded_output,
					segment: contract_segment,
				}));
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
