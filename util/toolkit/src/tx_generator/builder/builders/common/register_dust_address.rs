use std::{collections::VecDeque, convert::Infallible, sync::Arc};

use super::ledger_helpers_local::{
	BuildIntent, BuildUtxoOutput, BuildUtxoSpend, BuilderContext, DefaultDB,
	DustRegistrationBuilder, DustWallet, FromContext, IntentInfo, NIGHT, ProofProvider, Segment,
	StandardTrasactionInfo, Timestamp, TransactionWithContext, UnshieldedOfferInfo, UtxoOutputInfo,
	UtxoSpendInfo, WalletAddress, WalletSeed,
};
use async_trait::async_trait;

use crate::{
	progress::Spin,
	serde_def::SourceTransactions,
	tx_generator::builder::{BuildTxs, RegisterDustAddressArgs},
};
use midnight_node_ledger_helpers::fork::raw_block_data::SerializedTxBatches;

pub struct RegisterDustAddressBuilder<C: BuilderContext<DefaultDB>> {
	context: Arc<C>,
	prover: Arc<dyn ProofProvider<DefaultDB>>,
	seed: WalletSeed,
	rng_seed: Option<[u8; 32]>,
	funding_seed: Option<WalletSeed>,
	destination_dust: Option<WalletAddress>,
}

impl<C: BuilderContext<DefaultDB>> RegisterDustAddressBuilder<C> {
	pub fn new(
		args: RegisterDustAddressArgs,
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<DefaultDB>>,
	) -> Self {
		use super::type_convert::convert_wallet_seed;

		// Only the seed values are stored; their schemes are applied at context build time (see
		// `Builder::relevant_wallet_schemes`).
		let (wallet_seed, _) = args.wallet_seed.resolve();
		let funding_seed = args.funding_seed.map(|s| convert_wallet_seed(s.resolve().0));
		Self {
			context,
			prover,
			seed: convert_wallet_seed(wallet_seed),
			rng_seed: args.rng_seed,
			funding_seed,
			destination_dust: args
				.destination_dust
				.as_ref()
				.map(super::type_convert::convert_wallet_address),
		}
	}
}

#[async_trait]
impl<C: BuilderContext<DefaultDB>> BuildTxs for RegisterDustAddressBuilder<C> {
	type Error = Infallible;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		let spin = Spin::new("building register dust address transaction...");

		let seed = self.seed.clone();
		let funding_seed = self.funding_seed.clone();

		let context = self.context.clone();

		let mut tx_info = StandardTrasactionInfo::new_from_context(
			context.clone(),
			self.prover.clone(),
			self.rng_seed,
		);

		let now = context.latest_block_context().await.tblock;
		let dust_params = context.ledger_parameters().await.dust;

		// Retroactive DUST a generationless NIGHT UTXO has accrued by `now` —
		// the per-UTXO term of the ledger's `generationless_fee_availability`.
		let accrued_dust = |value: u128, ctime: Timestamp| {
			let vfull = value.saturating_mul(dust_params.night_dust_ratio.into());
			let rate = value.saturating_mul(dust_params.generation_decay_rate.into());
			let dt = u128::try_from((now - ctime).as_seconds()).unwrap_or(0);
			u128::clamp(dt.saturating_mul(rate), 0, vfull)
		};

		let mut inputs: Vec<(UtxoSpendInfo<WalletSeed>, u128)> = context
			.unshielded_utxos(seed.clone())
			.await
			.into_iter()
			.filter(|(utxo, _ctime)| utxo.type_ == NIGHT)
			.map(|(utxo, ctime)| {
				let accrued = accrued_dust(utxo.value, ctime);
				(
					UtxoSpendInfo {
						value: utxo.value,
						owner: seed.clone(),
						token_type: NIGHT,
						intent_hash: Some(utxo.intent_hash),
						output_number: Some(utxo.output_no),
					},
					accrued,
				)
			})
			.collect();

		// The ledger only credits retroactive DUST from NIGHT inputs in the intent's
		// *guaranteed* unshielded offer, and the guaranteed segment's time-to-dismiss
		// budget only fits a single input — so sort the dust-richest UTXO to the
		// front, where it becomes the guaranteed spend that funds the registration.
		inputs.sort_by(|(_, a), (_, b)| b.cmp(a));

		// Self-funding allowance: exactly the retroactive DUST of the guaranteed
		// input. Requesting more than the ledger credits for the constructed tx
		// fails validation with InsufficientDustForRegistrationFee.
		let allow_fee_payment = if funding_seed.is_none() {
			inputs.first().map(|(_, accrued)| *accrued).unwrap_or(0)
		} else {
			0
		};

		let inputs: Vec<UtxoSpendInfo<WalletSeed>> =
			inputs.into_iter().map(|(input, _)| input).collect();

		let mut outputs: VecDeque<Box<dyn BuildUtxoOutput<DefaultDB, C>>> = inputs
			.iter()
			.map(|input| {
				let output: Box<dyn BuildUtxoOutput<DefaultDB, C>> = Box::new(UtxoOutputInfo {
					value: input.value,
					owner: input.owner.clone(),
					token_type: input.token_type,
				});
				output
			})
			.collect();

		let mut inputs: VecDeque<Box<dyn BuildUtxoSpend<DefaultDB, C>>> = inputs
			.into_iter()
			.map(|input| {
				let input: Box<dyn BuildUtxoSpend<DefaultDB, C>> = Box::new(input);
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

		let boxed_intent: Box<dyn BuildIntent<DefaultDB, C>> = Box::new(intent_info);
		tx_info.add_intent(Segment::Fallible.into(), boxed_intent);

		context.with_wallet_from_seed(seed.clone(), |wallet| {
			let destination_dust = self.destination_dust.clone().map_or(
				wallet.dust.public_key,
				|destination_dust_arg| {
					DustWallet::<DefaultDB>::try_from(&destination_dust_arg)
						.expect("failed to decode dust address")
						.public_key
				},
			);
			tx_info.add_dust_registration(DustRegistrationBuilder {
				wallet: wallet.unshielded.clone(),
				dust_address: Some(destination_dust),
				allow_fee_payment,
			});
		});

		tx_info.set_funding_seeds(funding_seed.into_iter().collect());
		tx_info.use_mock_proofs_for_fees(true);

		let tx = tx_info.prove().await.expect(
			"Balancing TX failed. For self-funded registration the fee is limited to the \
			 retroactive DUST of the wallet's dust-richest NIGHT UTXO — consolidate NIGHT into \
			 fewer UTXOs, wait for more DUST to accrue, or pass --funding-seed",
		);

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		Ok(super::tx_serialization::build_single(tx_with_context))
	}
}
