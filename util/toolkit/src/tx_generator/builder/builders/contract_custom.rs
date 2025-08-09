use crate::{
	builder::{
		BuildInput, BuildIntent, BuildOutput, BuildTxs, BuildTxsExt, CustomContractArgs, DefaultDB,
		DeserializedTransactionsWithContext, InputInfo, IntentCustom, NetworkId, OfferInfo,
		OutputInfo, ProofProvider, ProofType, SegmentId, SignatureType, TransactionWithContext,
		Wallet, WalletSeed,
	},
	unwrapped_fee_token,
};
use async_trait::async_trait;
use std::{collections::HashMap, convert::Infallible, fs, sync::Arc};

pub struct CustomContractBuilder {
	funding_seed: String,
	rng_seed: Option<[u8; 32]>,
	fee: u128,
	artifacts_dir: String,
}

impl CustomContractBuilder {
	pub fn new(args: CustomContractArgs) -> Self {
		Self {
			funding_seed: args.info.funding_seed,
			rng_seed: args.info.rng_seed,
			fee: args.info.fee,
			artifacts_dir: args.artifacts_dir,
		}
	}

	pub fn intents_dir(&self) -> String {
		format!("{}/intents", self.artifacts_dir)
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
		let boxed_resolver = Box::new(IntentCustom::get_resolver(self.artifacts_dir.clone()));
		let static_ref_resolver = Box::leak(boxed_resolver);

		// Go over all files with .mn extension inside the intents directory
		let mut entries = fs::read_dir(self.intents_dir()).expect("directory not found");

		let mut next_segment_id: SegmentId = 1;
		while let Some(Ok(entry)) = entries.next() {
			let path = entry.path();

			if !path.is_file() {
				continue;
			}

			let Some(extension) = path.extension() else { continue };
			if extension == "mn" {
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
		received_tx: DeserializedTransactionsWithContext<SignatureType, ProofType>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<DeserializedTransactionsWithContext<SignatureType, ProofType>, Self::Error> {
		println!("Building Txs for CustomContract");
		// - LedgerContext and TransactionInfo
		let (context_arc, mut tx_info) = self.context_and_tx_info(received_tx, prover_arc);

		// - Intents
		let intents = self.create_intent_info();
		tx_info.set_intents(intents);

		let funding_seed = self.funding_seed();

		//   - Input
		let input_info =
			InputInfo { origin: funding_seed, token_type: unwrapped_fee_token(), value: self.fee };
		let inputs_info: Vec<Box<dyn BuildInput<DefaultDB> + Send>> = vec![Box::new(input_info)];

		//   - Output
		let funding_wallet = context_arc.clone().wallet_from_seed(funding_seed);
		let already_spent = input_info.min_match_coin(&funding_wallet.shielded.state).value;
		let remaining_coins = already_spent - self.fee;

		let output_info = OutputInfo {
			destination: funding_seed,
			token_type: unwrapped_fee_token(),
			value: remaining_coins,
		};
		let outputs_info: Vec<Box<dyn BuildOutput<DefaultDB> + Send>> = vec![Box::new(output_info)];

		let offer_info =
			OfferInfo { inputs: inputs_info, outputs: outputs_info, transients: vec![] };

		tx_info.set_guaranteed_coins(offer_info);

		#[cfg(not(feature = "erase-proof"))]
		let tx = tx_info.prove().await;

		#[cfg(feature = "erase-proof")]
		let tx = tx_info.erase_proof().await;

		let tx_with_context = TransactionWithContext::new(tx, None);

		Ok(DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] })
	}
}
