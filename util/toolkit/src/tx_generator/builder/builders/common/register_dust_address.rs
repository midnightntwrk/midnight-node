use std::{collections::VecDeque, convert::Infallible, sync::Arc};

use async_trait::async_trait;
use super::ledger_helpers_local::{
	BuildIntent, BuildUtxoOutput, BuildUtxoSpend, DefaultDB, DustRegistrationBuilder, DustWallet,
	FromContext, IntentInfo, LedgerContext, NIGHT, ProofProvider, Segment, StandardTrasactionInfo,
	TransactionWithContext, UnshieldedOfferInfo, UtxoOutputInfo, UtxoSpendInfo, Wallet,
	WalletAddress, WalletSeed,
};

use crate::{
	progress::Spin,
	serde_def::{BuiltTransactions, DeserializedTransactionsWithContext, SourceTransactions},
	tx_generator::builder::{BuildTxs, RegisterDustAddressArgs},
};

pub struct RegisterDustAddressBuilder {
	seed: String,
	rng_seed: Option<[u8; 32]>,
	funding_seed: String,
	destination_dust: Option<WalletAddress>,
}

impl RegisterDustAddressBuilder {
	pub fn new(args: RegisterDustAddressArgs) -> Self {
		Self {
			seed: args.wallet_seed,
			rng_seed: args.rng_seed,
			funding_seed: args.funding_seed,
			destination_dust: args.destination_dust,
		}
	}
}

#[async_trait]
impl BuildTxs for RegisterDustAddressBuilder {
	type Error = Infallible;

	fn relevant_wallet_seeds(&self) -> Vec<WalletSeed> {
		let seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.seed);
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);
		vec![seed, funding_seed]
	}

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
		context: Option<Arc<LedgerContext<DefaultDB>>>,
		prover_arc: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Result<BuiltTransactions, Self::Error> {
		let spin = Spin::new("building register dust address transaction...");

		let seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.seed);
		let funding_seed = Wallet::<DefaultDB>::wallet_seed_decode(&self.funding_seed);

		let context = context.expect("RegisterDustAddressBuilder requires context");

		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context.clone(),
			prover_arc.clone(),
			self.rng_seed,
		);

		let inputs = context.with_ledger_state(|ledger_state| {
			context.with_wallet_from_seed(seed, |wallet| {
				wallet
					.unshielded_utxos(ledger_state)
					.iter()
					.filter(|utxo| utxo.type_ == NIGHT)
					.map(|utxo| UtxoSpendInfo {
						value: utxo.value,
						owner: seed,
						token_type: NIGHT,
						intent_hash: Some(utxo.intent_hash),
						output_number: Some(utxo.output_no),
					})
					.collect::<Vec<_>>()
			})
		});

		let mut outputs: VecDeque<Box<dyn BuildUtxoOutput<DefaultDB>>> = inputs
			.iter()
			.map(|input| {
				let output: Box<dyn BuildUtxoOutput<DefaultDB>> = Box::new(UtxoOutputInfo {
					value: input.value,
					owner: input.owner,
					token_type: input.token_type,
				});
				output
			})
			.collect();

		let mut inputs: VecDeque<Box<dyn BuildUtxoSpend<DefaultDB>>> = inputs
			.into_iter()
			.map(|input| {
				let input: Box<dyn BuildUtxoSpend<DefaultDB>> = Box::new(input);
				input
			})
			.collect();

		let guaranteed_inputs = inputs.pop_front().into_iter().collect();
		let guaranteed_outputs = outputs.pop_front().into_iter().collect();
		let guaranteed_unshielded_offer =
			UnshieldedOfferInfo { inputs: guaranteed_inputs, outputs: guaranteed_outputs };

		let fallible_unshielded_offer = if !inputs.is_empty() && !outputs.is_empty() {
			Some(UnshieldedOfferInfo { inputs: inputs.into(), outputs: outputs.into() })
		} else {
			None
		};
		let intent_info = IntentInfo {
			guaranteed_unshielded_offer: Some(guaranteed_unshielded_offer),
			fallible_unshielded_offer,
			actions: vec![],
		};

		let boxed_intent: Box<dyn BuildIntent<DefaultDB>> = Box::new(intent_info);
		tx_info.add_intent(Segment::Fallible.into(), boxed_intent);

		context.with_wallet_from_seed(seed, |wallet| {
			let destination_dust = self.destination_dust.clone().map_or(
				wallet.dust.public_key,
				|destination_dust_arg| {
					DustWallet::<DefaultDB>::try_from(&destination_dust_arg)
						.expect("failed to decode dust address")
						.public_key
				},
			);
			tx_info.add_dust_registration(DustRegistrationBuilder {
				signing_key: wallet.unshielded.signing_key().clone(),
				dust_address: Some(destination_dust),
			});
		});

		tx_info.set_funding_seeds(vec![funding_seed]);
		tx_info.use_mock_proofs_for_fees(true);

		let tx = tx_info.prove().await.expect("Balancing TX failed");

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		let typed =
			DeserializedTransactionsWithContext { initial_tx: tx_with_context, batches: vec![] };
		Ok(BuiltTransactions::from_typed(typed))
	}
}
