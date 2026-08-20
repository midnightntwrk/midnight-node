use std::sync::Arc;

use super::ledger_helpers_local::{
	BuildIntent, BuildUtxoOutput, BuildUtxoSpend, BuilderContext, DefaultDB, DustAddressParseError,
	DustParameters, DustRegistrationBuilder, DustWallet, FromContext, IntentInfo, NIGHT,
	ProofProvider, Segment, StandardTransactionInfo, Timestamp, TransactionWithContext,
	UnshieldedOfferInfo, Utxo, UtxoOutputInfo, UtxoSpendInfo, WalletAddress, WalletSeed,
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

#[derive(Debug, thiserror::Error)]
pub enum RegisterDustAddressError {
	#[error("failed to decode destination DUST address: {0:?}")]
	InvalidDustAddress(DustAddressParseError),
	#[error(
		"failed to balance the registration transaction: {0}; consolidate NIGHT into a larger \
		 UTXO, wait for more DUST to accrue, or pay the fee via --funding-seed"
	)]
	Balancing(Box<dyn std::error::Error + Send + Sync>),
	#[error(
		"every NIGHT UTXO in the wallet already backs DUST generation, so none accrues \
		 retroactive DUST for a self-funded registration fee; send the NIGHT to yourself to \
		 mint fresh UTXOs, or pay the fee via --funding-seed"
	)]
	AllUtxosBackGeneration,
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

/// Retroactive DUST a generationless NIGHT UTXO accrues between `ctime` and `now`;
/// mirrors the ledger's `generationless_fee_availability`.
fn retroactive_dust(
	value: u128,
	ctime: Timestamp,
	now: Timestamp,
	params: &DustParameters,
) -> u128 {
	let vfull = value.saturating_mul(params.night_dust_ratio.into());
	let rate = value.saturating_mul(params.generation_decay_rate.into());
	let dt = u128::try_from((now - ctime).as_seconds()).unwrap_or(0);
	dt.saturating_mul(rate).min(vfull)
}

#[async_trait]
impl<C: BuilderContext<DefaultDB>> BuildTxs for RegisterDustAddressBuilder<C> {
	type Error = RegisterDustAddressError;

	async fn build_txs_from(
		&self,
		_received_tx: SourceTransactions,
	) -> Result<SerializedTxBatches, Self::Error> {
		let spin = Spin::new("building register dust address transaction...");

		let seed = self.seed.clone();
		let funding_seed = self.funding_seed.clone();

		let context = self.context.clone();

		let mut tx_info = StandardTransactionInfo::new_from_context(
			context.clone(),
			self.prover.clone(),
			self.rng_seed,
		);

		let mut night_utxos: Vec<(Utxo, Timestamp)> = context
			.unshielded_utxos(seed.clone())
			.await
			.into_iter()
			.filter(|(utxo, _ctime)| utxo.type_ == NIGHT)
			.collect();

		// A self-funded fee is paid from retroactive DUST, which the ledger only grants for
		// generationless NIGHT spent in the guaranteed offer. Move the best such UTXO to the
		// front (it becomes the guaranteed offer) and request exactly its accrued DUST;
		// requesting more makes the transaction unbalanceable.
		let mut allow_fee_payment = 0u128;
		if funding_seed.is_none() {
			let now = context.latest_block_context().await.tblock;
			let dust_params = context.ledger_parameters().await.dust;
			let mut best: Option<(usize, u128)> = None;
			for (i, (utxo, ctime)) in night_utxos.iter().enumerate() {
				if context.backs_dust_generation(utxo).await {
					continue;
				}
				let dust = retroactive_dust(utxo.value, *ctime, now, &dust_params);
				if best.is_none_or(|(_, best_dust)| dust > best_dust) {
					best = Some((i, dust));
				}
			}
			match best {
				Some((i, dust)) => {
					night_utxos.swap(0, i);
					allow_fee_payment = dust;
				},
				// A wallet whose every NIGHT UTXO backs generation can never accrue
				// retroactive DUST, so the generic "wait for more DUST" guidance of the
				// balancing error below would mislead. An empty wallet still falls
				// through: balancing reports the missing funds.
				None if !night_utxos.is_empty() => {
					return Err(RegisterDustAddressError::AllUtxosBackGeneration);
				},
				None => {},
			}
		}

		let mut inputs: Vec<Box<dyn BuildUtxoSpend<DefaultDB, C>>> = night_utxos
			.iter()
			.map(|(utxo, _ctime)| {
				let input: Box<dyn BuildUtxoSpend<DefaultDB, C>> = Box::new(UtxoSpendInfo {
					value: utxo.value,
					owner: seed.clone(),
					token_type: NIGHT,
					intent_hash: Some(utxo.intent_hash),
					output_number: Some(utxo.output_no),
				});
				input
			})
			.collect();

		let mut outputs: Vec<Box<dyn BuildUtxoOutput<DefaultDB, C>>> = night_utxos
			.iter()
			.map(|(utxo, _ctime)| {
				let output: Box<dyn BuildUtxoOutput<DefaultDB, C>> = Box::new(UtxoOutputInfo {
					value: utxo.value,
					owner: seed.clone(),
					token_type: NIGHT,
				});
				output
			})
			.collect();

		// Only one UTXO fits the guaranteed offer - more would exceed the ledger's
		// time-to-dismiss limit. The rest go into the fallible offer: their retroactive
		// DUST is forfeited, but they still switch over to regular DUST generation.
		let fallible_inputs = inputs.split_off(usize::min(1, inputs.len()));
		let fallible_outputs = outputs.split_off(usize::min(1, outputs.len()));

		let guaranteed_unshielded_offer = UnshieldedOfferInfo { inputs, outputs };
		let fallible_unshielded_offer =
			if !fallible_inputs.is_empty() && !fallible_outputs.is_empty() {
				Some(UnshieldedOfferInfo { inputs: fallible_inputs, outputs: fallible_outputs })
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

		let destination_dust = match self.destination_dust.as_ref() {
			Some(address) => {
				DustWallet::<DefaultDB>::try_from(address)
					.map_err(RegisterDustAddressError::InvalidDustAddress)?
					.public_key
			},
			None => context.with_wallet_from_seed(seed.clone(), |wallet| wallet.dust.public_key),
		};
		context.with_wallet_from_seed(seed.clone(), |wallet| {
			tx_info.add_dust_registration(DustRegistrationBuilder {
				wallet: wallet.unshielded.clone(),
				dust_address: Some(destination_dust),
				allow_fee_payment,
			});
		});

		tx_info.set_funding_seeds(funding_seed.into_iter().collect());
		tx_info.use_mock_proofs_for_fees(true);

		let tx = tx_info.prove().await.map_err(RegisterDustAddressError::Balancing)?;

		let tx_with_context = TransactionWithContext::new(tx, None);

		spin.finish("generated tx.");

		Ok(super::tx_serialization::build_single(tx_with_context))
	}
}
