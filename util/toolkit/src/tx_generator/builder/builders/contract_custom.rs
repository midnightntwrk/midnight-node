use crate::{
	builder::{
		BuildInput, BuildIntent, BuildOutput, BuildTxs, BuildTxsExt, CustomContractArgs, DefaultDB,
		DeserializedTransactionsWithContext, IntentCustom, NetworkId, OfferInfo, ProofProvider,
		ProofType, SegmentId, SignatureType, TransactionWithContext, Wallet, WalletSeed,
	},
	serde_def::SourceTransactions,
};
use async_trait::async_trait;
use std::{collections::HashMap, convert::Infallible, path::PathBuf, sync::Arc};

pub struct CustomContractBuilder {
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	artifacts_dir: String,
	intent_files: Vec<String>,
}

impl CustomContractBuilder {
	pub fn new(args: CustomContractArgs) -> Self {
		Self {
			funding_seed: args.info.funding_seed,
			rng_seed: args.info.rng_seed,
			artifacts_dir: args.compiled_contract_dir,
			intent_files: args.intent_files,
		}
	}
}

impl BuildTxsExt<HashMap<u16, Box<dyn BuildIntent<DefaultDB> + Send>>> for CustomContractBuilder {
	fn funding_seed(&self) -> WalletSeed {
		Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed)
	}

	fn rng_seed(&self) -> Option<[u8; 32]> {
		self.rng_seed
	}

	fn create_intent_info(&self) -> HashMap<u16, Box<dyn BuildIntent<DefaultDB> + Send>> {
		println!("Create intent info for contract custom");
		let mut intents: HashMap<u16, Box<dyn BuildIntent<DefaultDB> + Send>> = HashMap::new();
		// This is to satisfy the `&'static` need to update the context's resolver
		// Data lives for the remainder of the program's life.
		let boxed_resolver =
			Box::new(IntentCustom::get_resolver(self.artifacts_dir.clone()).unwrap());
		let static_ref_resolver = Box::leak(boxed_resolver);

		let mut next_segment_id: SegmentId = 1;
		for intent_path in &self.intent_files {
			let path = PathBuf::from(&intent_path);

			if !path.is_file() {
				println!("Warning: {} is not a file", &intent_path);
				continue;
			}

			let Some(extension) = path.extension() else { continue };
			if extension == "bin" {
				let intent_path = path.into_os_string();
				let intent_path = intent_path.to_str().expect("should return str").to_string();

				println!("Intent found: {intent_path}");

				intents.insert(
					next_segment_id,
					Box::new(IntentCustom {
						intent_path,
						network: NetworkId::Undeployed,
						resolver: static_ref_resolver,
					}),
				);

				next_segment_id += 1;
			}
		}

		intents
	}
}

#[async_trait]
impl BuildTxs for CustomContractBuilder {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		received_tx: SourceTransactions<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		println!("Building Txs for CustomContract");
		// - LedgerContext and TransactionInfo
		let (_, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

		// - Intents
		let intents = self.create_intent_info();
		tx_info.set_intents(intents);

		//   - Input
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB> + Send>> = vec![];

		//   - Output
		let outputs_info: Vec<Box<dyn BuildOutput<DefaultDB> + Send>> = vec![];

		let offer_info =
			OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] };

		tx_info.set_guaranteed_coins(offer_info);

		tx_info.set_wallet_seeds(vec![self.funding_seed()]);
		tx_info.use_mock_proofs_for_fees(false);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await.expect("Balancing TX failed");

		#[cfg(feature = "erase-proof")]
		let tx = tx_info.erase_proof().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] })
	}
}
