// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use rand::Rng as _;

use super::{
	BindingKind, BuildIntent, BuilderContext, ClaimKind, ClaimRewardsTransaction, DB, DustActions,
	DustLocalState, DustParameters, DustPublicKey, DustRegistration, DustSpend, HashMapStorage,
	Intent, Offer, OfferInfo, Pedersen, PedersenDowngradeable, PedersenRandomness, ProofKind,
	ProofMarker, ProofPreimage, ProofPreimageMarker, ProofProvider, PureGeneratorPedersen,
	SeedableRng, Segment, SegmentId, Serializable, Signature, SignatureKind, SigningKey, Sp,
	SplittableRng, StdRng, Storable, Tagged, Timestamp, TokenType, Transaction, WalletSeed,
	serialize, signature_verifying_key, transaction_signature,
};
use std::{collections::HashMap, error::Error, fs, fs::File, io::Write, sync::Arc};

pub type UnprovenTransaction<D> =
	Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>;
#[cfg(not(feature = "erase-proof"))]
pub type FinalizedTransaction<D> = Transaction<Signature, ProofMarker, PureGeneratorPedersen, D>;
#[cfg(feature = "erase-proof")]
pub type FinalizedTransaction<D> = Transaction<Signature, (), Pedersen, D>;

type Result<T, E = Box<dyn Error + Send + Sync>> = std::result::Result<T, E>;

type DustSpendStates<D> = HashMap<WalletSeed, Sp<DustLocalState<D>, D>>;
type GatheredDustSpends<D> = (Vec<DustSpend<ProofPreimageMarker, D>>, DustSpendStates<D>);

pub trait FromContext<D: DB + Clone, C: BuilderContext<D>> {
	fn new_from_context(
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
	) -> Self;

	fn rng(maybe_rng_seed: Option<[u8; 32]>) -> StdRng {
		maybe_rng_seed.map(StdRng::from_seed).unwrap_or(StdRng::from_entropy())
	}
}

pub struct DustRegistrationBuilder {
	pub signing_key: SigningKey,
	pub dust_address: Option<DustPublicKey>,
	pub allow_fee_payment: u128,
}

impl DustRegistrationBuilder {
	pub fn build<
		D: DB + Clone,
		P: ProofKind<D>,
		B: Storable<D> + PedersenDowngradeable<D> + Serializable,
	>(
		&self,
		intent: &Intent<Signature, P, B, D>,
		rng: &mut StdRng,
		segment_id: u16,
	) -> DustRegistration<Signature, D> {
		let data_to_sign = intent.erase_proofs().erase_signatures().data_to_sign(segment_id);
		let signature = self.signing_key.sign(rng, &data_to_sign);
		let night_key = signature_verifying_key(self.signing_key.verifying_key());

		DustRegistration {
			night_key,
			dust_address: self.dust_address.map(|address| Sp::new(address)),
			allow_fee_payment: self.allow_fee_payment,
			signature: Some(Sp::new(transaction_signature(signature))),
		}
	}
}

pub struct StandardTrasactionInfo<D: DB + Clone, C: BuilderContext<D>> {
	pub context: Arc<C>,
	pub intents: HashMap<SegmentId, Box<dyn BuildIntent<D, C>>>,
	pub guaranteed_offer: Option<OfferInfo<D, C>>,
	pub fallible_offers: HashMap<u16, OfferInfo<D, C>>,
	pub rng: StdRng,
	pub prover: Arc<dyn ProofProvider<D>>,
	pub funding_seeds: Vec<WalletSeed>,
	pub mock_proofs_for_fees: bool,
	pub dust_registrations: Vec<DustRegistrationBuilder>,
}

impl<D: DB + Clone, C: BuilderContext<D>> FromContext<D, C> for StandardTrasactionInfo<D, C> {
	fn new_from_context(
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
	) -> Self {
		let rng = Self::rng(maybe_rng_seed);

		Self {
			context,
			intents: HashMap::new(),
			guaranteed_offer: None,
			fallible_offers: HashMap::new(),
			rng,
			prover,
			funding_seeds: vec![],
			mock_proofs_for_fees: false,
			dust_registrations: vec![],
		}
	}
}

impl<D: DB + Clone, C: BuilderContext<D>> StandardTrasactionInfo<D, C> {
	pub fn set_guaranteed_offer(&mut self, offer: OfferInfo<D, C>) {
		self.guaranteed_offer = Some(offer);
	}

	pub fn set_fallible_offers(&mut self, offers: HashMap<u16, OfferInfo<D, C>>) {
		self.fallible_offers = offers;
	}

	pub fn set_intents(&mut self, intents: HashMap<u16, Box<dyn BuildIntent<D, C>>>) {
		self.intents = intents;
	}

	pub fn add_intent(&mut self, segment_id: SegmentId, intent: Box<dyn BuildIntent<D, C>>) {
		if self.intents.insert(segment_id, intent).is_some() {
			log::warn!("value of segment_id({segment_id}) has been replaced");
		};
	}

	pub fn add_dust_registration(&mut self, dust_registration: DustRegistrationBuilder) {
		self.dust_registrations.push(dust_registration);
	}

	pub fn is_empty(&self) -> bool {
		self.intents.is_empty()
			&& self.guaranteed_offer.is_none()
			&& self.fallible_offers.is_empty()
	}

	pub fn set_funding_seeds(&mut self, seeds: Vec<WalletSeed>) {
		self.funding_seeds = seeds;
	}

	pub fn use_mock_proofs_for_fees(&mut self, mock_proofs_for_fees: bool) {
		self.mock_proofs_for_fees = mock_proofs_for_fees;
	}

	async fn build(&mut self) -> Result<FinalizedTransaction<D>> {
		let now = self.context.latest_block_context().await.tblock;
		let delay = self.context.ledger_parameters().await.global_ttl;
		let ttl = now + delay;

		let guaranteed_offer: Option<Offer<ProofPreimage, D>> = self
			.guaranteed_offer
			.as_mut()
			.map(|gc| gc.build(&mut self.rng, self.context.clone()))
			.transpose()?;

		let fallible_offer = self
			.fallible_offers
			.iter_mut()
			.map(
				|(segment_id, offer_info)| -> std::result::Result<_, Box<dyn Error + Send + Sync>> {
					Ok((*segment_id, offer_info.build(&mut self.rng, self.context.clone())?))
				},
			)
			.collect::<std::result::Result<Vec<_>, _>>()?
			.into_iter()
			.collect();

		let mut intents = HashMapStorage::<
			u16,
			Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
			D,
		>::new();

		for (segment_id, intent_info) in self.intents.iter_mut() {
			let intent =
				intent_info.build(&mut self.rng, ttl, self.context.clone(), *segment_id).await;
			intents = intents.insert(*segment_id, intent);
		}

		let network_id = self.context.network_id().await;

		let tx = Transaction::new(network_id.clone(), intents, guaranteed_offer, fallible_offer);

		log::debug!("pre-proof tx: {tx:#?}");
		log::debug!("tx balance pre-fees: {:#?}", tx.balance(None));

		// Pay the outstanding DUST balance, if we have a wallet seed or dust registrations
		if self.funding_seeds.is_empty() && self.dust_registrations.is_empty() {
			self.prove_tx(tx).await
		} else {
			let tx = self.pay_fees(tx, now, ttl).await?;
			let parameters = self.context.ledger_parameters().await;
			let fees = tx.fees_with_margin(&parameters, 3)?;
			log::debug!("post-proof tx: {tx:#?}");
			log::debug!("tx-balance post-prove: {:#?}", tx.balance(Some(fees))?);
			Ok(tx)
		}
	}

	async fn pay_fees(
		&mut self,
		tx: UnprovenTransaction<D>,
		now: Timestamp,
		ttl: Timestamp,
	) -> Result<FinalizedTransaction<D>> {
		let mut missing_dust = 0;
		let dust_params = self.context.ledger_parameters().await.dust;

		for _ in 0..10 {
			let (spends, updated_states) =
				self.gather_dust_spends(missing_dust, now, &dust_params)?;
			let mut paid_tx = tx.clone();
			self.apply_dust(&mut paid_tx, &spends, self.rng.clone().split(), now, ttl);

			if self.mock_proofs_for_fees {
				let mock_proven_tx = self.mock_prove_tx(&paid_tx)?;
				let computed_missing_dust = self.compute_missing_dust(&mock_proven_tx).await?;
				if let Some(dust) = computed_missing_dust {
					missing_dust += dust;
				} else {
					self.confirm_dust_spends(&spends, updated_states)?;
					return self.prove_tx(paid_tx).await;
				}
			} else {
				let proven_tx = self.prove_tx(paid_tx).await?;
				let computed_missing_dust = self.compute_missing_dust(&proven_tx).await?;
				if let Some(dust) = computed_missing_dust {
					missing_dust += dust;
				} else {
					self.confirm_dust_spends(&spends, updated_states)?;
					return Ok(proven_tx);
				}
			}
		}
		Err("Could not balance TX".into())
	}

	#[cfg(not(feature = "erase-proof"))]
	async fn prove_tx(&mut self, tx: UnprovenTransaction<D>) -> Result<FinalizedTransaction<D>> {
		let resolver = self.context.resolver().await;
		let parameters = self.context.ledger_parameters().await;
		let mut rng = self.rng.split();
		Ok(self
			.prover
			.prove(tx, rng.split(), resolver, &parameters.cost_model.runtime_cost_model)
			.await
			.seal(rng))
	}

	#[cfg(feature = "erase-proof")]
	async fn prove_tx(&mut self, tx: UnprovenTransaction<D>) -> Result<FinalizedTransaction<D>> {
		Ok(tx.erase_proofs())
	}

	#[cfg(not(feature = "erase-proof"))]
	fn mock_prove_tx(&self, tx: &UnprovenTransaction<D>) -> Result<FinalizedTransaction<D>> {
		Ok(tx.mock_prove()?)
	}

	#[cfg(feature = "erase-proof")]
	fn mock_prove_tx(&self, tx: &UnprovenTransaction<D>) -> Result<FinalizedTransaction<D>> {
		Ok(tx.erase_proofs())
	}

	async fn compute_missing_dust(&self, tx: &FinalizedTransaction<D>) -> Result<Option<u128>> {
		let parameters = self.context.ledger_parameters().await;
		let fees = tx.fees_with_margin(&parameters, 3)?;
		let imbalances = tx.balance(Some(fees))?;
		let dust_imbalance = imbalances
			.get(&(TokenType::Dust, Segment::Guaranteed.into()))
			.copied()
			.unwrap_or_default();
		if dust_imbalance < 0 { Ok(Some(dust_imbalance.unsigned_abs())) } else { Ok(None) }
	}

	fn apply_dust(
		&self,
		tx: &mut UnprovenTransaction<D>,
		spends: &[DustSpend<ProofPreimageMarker, D>],
		mut rng: StdRng,
		now: Timestamp,
		ttl: Timestamp,
	) {
		let Transaction::Standard(stx) = tx else {
			return;
		};

		if spends.is_empty() && self.dust_registrations.is_empty() {
			return;
		}

		let segment_id = Segment::Fallible.into();
		let mut intent = match stx.intents.get(&segment_id) {
			Some(intent) => (*intent).clone(),
			None => Intent::empty(&mut rng, ttl),
		};
		let registrations = self
			.dust_registrations
			.iter()
			.map(|registration| registration.build(&intent, &mut rng, segment_id))
			.collect::<Vec<_>>()
			.into();

		intent.dust_actions = Some(Sp::new(DustActions {
			spends: spends.to_vec().into(),
			registrations,
			ctime: now,
		}));
		stx.intents = stx.intents.insert(segment_id, intent);

		// Re-compute the binding randomness
		// if we inserted an intent, we need to do this to avoid a Pedersen check error
		*tx = Transaction::new(
			stx.network_id.clone(),
			stx.intents.clone(),
			stx.guaranteed_coins.as_ref().map(|c| (**c).clone()),
			stx.fallible_coins.iter().map(|sp| (*sp.0, (*sp.1).clone())).collect(),
		);
	}

	fn gather_dust_spends(
		&self,
		required_amount: u128,
		ctime: Timestamp,
		params: &DustParameters,
	) -> Result<GatheredDustSpends<D>> {
		let mut spends = vec![];
		let mut updated_states = HashMap::new();
		let mut remaining = required_amount;
		for seed in &self.funding_seeds {
			if remaining == 0 {
				return Ok((spends, updated_states));
			}
			let (new_spends, updated_state) =
				self.context.with_wallet_from_seed(seed.clone(), |wallet| {
					wallet.dust.speculative_spend(remaining, ctime, params)
				})?;
			if !new_spends.is_empty() {
				updated_states.insert(seed.clone(), updated_state);
			}
			for spend in new_spends {
				remaining -= spend.v_fee;
				spends.push(spend);
			}
		}
		if remaining > 0 {
			Err(format!(
				"Insufficient DUST (trying to spend {required_amount}, need {remaining} more)"
			)
			.into())
		} else {
			Ok((spends, updated_states))
		}
	}

	fn confirm_dust_spends(
		&mut self,
		spends: &[DustSpend<ProofPreimageMarker, D>],
		mut updated_states: DustSpendStates<D>,
	) -> Result<()> {
		for seed in &self.funding_seeds {
			if let Some(updated_state) = updated_states.remove(seed) {
				self.context.with_wallet_from_seed(seed.clone(), |wallet| {
					wallet.dust.mark_spent(spends, updated_state);
				});
			}
		}
		Ok(())
	}

	pub async fn save_intents_to_file(mut self, parent_dir: &str, file_name: &str) -> Result<()> {
		// make sure that the dir is created, if it does not exist
		fs::create_dir_all(parent_dir)?;

		let now = self.context.latest_block_context().await.tblock;
		let ttl = now + self.context.ledger_parameters().await.global_ttl;

		let mut saved_files: Vec<String> = Vec::new();

		for (segment_id, intent_info) in self.intents.iter_mut() {
			let intent =
				intent_info.build(&mut self.rng, ttl, self.context.clone(), *segment_id).await;
			log::debug!("Serializing intent...");

			let serialized_intent = serialize(&intent).map_err(|e| {
				// Clean up any files written so far
				for path in &saved_files {
					let _ = fs::remove_file(path);
				}
				format!("failed to serialize intent for segment {segment_id}: {e}")
			})?;

			let complete_file_name = format!("{parent_dir}/{segment_id}_{file_name}_intent.mn");

			let write_result = File::create(&complete_file_name)
				.and_then(|mut file| file.write_all(&serialized_intent));

			if let Err(e) = write_result {
				// Clean up any files written so far
				for path in &saved_files {
					let _ = fs::remove_file(path);
				}
				return Err(format!("failed to write intent file {complete_file_name}: {e}").into());
			}

			log::info!("Saved {complete_file_name}");
			saved_files.push(complete_file_name);
		}

		Ok(())
	}

	pub async fn erase_proof(mut self) -> Result<Transaction<(), (), Pedersen, D>> {
		let tx_unproven = self.build().await?;
		let tx_erased_proof = tx_unproven.erase_proofs();
		let now = self.context.latest_block_context().await.tblock;
		Self::validate(self.context, now, tx_erased_proof.erase_signatures())
	}

	pub async fn prove(mut self) -> Result<FinalizedTransaction<D>> {
		let tx = self.build().await?;
		let now = self.context.latest_block_context().await.tblock;
		Self::validate(self.context, now, tx)
	}

	fn validate<
		S: SignatureKind<D>,
		P: ProofKind<D> + Storable<D>,
		B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D> + Tagged,
	>(
		context: Arc<C>,
		now: Timestamp,
		tx: Transaction<S, P, B, D>,
	) -> Result<Transaction<S, P, B, D>> {
		context.well_formed(&tx, now)?;
		Ok(tx)
	}
}

pub struct RewardsInfo {
	pub owner: WalletSeed,
	pub value: u128,
}

pub struct ClaimMintInfo<D: DB + Clone, C: BuilderContext<D>> {
	pub context: Arc<C>,
	pub coin: RewardsInfo,
	pub rng: StdRng,
	pub prover: Arc<dyn ProofProvider<D>>,
}

impl<D: DB + Clone, C: BuilderContext<D>> FromContext<D, C> for ClaimMintInfo<D, C> {
	fn new_from_context(
		context: Arc<C>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
	) -> Self {
		let rng = Self::rng(maybe_rng_seed);

		Self {
			context,
			coin: RewardsInfo { owner: WalletSeed::Short([0; 16]), value: 0 },
			rng,
			prover,
		}
	}
}

impl<D: DB + Clone, C: BuilderContext<D>> ClaimMintInfo<D, C> {
	pub fn set_rewards(&mut self, rewards: RewardsInfo) {
		self.coin = rewards;
	}

	async fn build(&mut self) -> UnprovenTransaction<D> {
		let nonce = self.rng.r#gen();
		let network_id = self.context.network_id().await;
		let claim_rewards = self.context.with_wallet_from_seed(self.coin.owner.clone(), |wallet| {
			let unsigned_claim_mint: ClaimRewardsTransaction<(), D> = ClaimRewardsTransaction {
				network_id: network_id.clone(),
				value: self.coin.value,
				owner: signature_verifying_key(wallet.unshielded.signing_key().verifying_key()),
				nonce,
				signature: (),
				kind: ClaimKind::Reward,
			};

			let data_to_sign = unsigned_claim_mint.data_to_sign();
			let signature = wallet.unshielded.signing_key().sign(&mut self.rng, &data_to_sign);
			ClaimRewardsTransaction {
				network_id: network_id.clone(),
				value: self.coin.value,
				owner: signature_verifying_key(wallet.unshielded.signing_key().verifying_key()),
				nonce,
				signature: transaction_signature(signature),
				kind: ClaimKind::Reward,
			}
		});

		Transaction::ClaimRewards(claim_rewards)
	}

	#[cfg(not(feature = "erase-proof"))]
	pub async fn prove(mut self) -> FinalizedTransaction<D> {
		let tx_unproven = self.build().await;
		let resolver = self.context.resolver().await;
		let parameters = self.context.ledger_parameters().await;
		let tx_proven = self
			.prover
			.prove(
				tx_unproven,
				self.rng.clone(),
				resolver,
				&parameters.cost_model.runtime_cost_model,
			)
			.await;
		tx_proven.seal(self.rng.clone())
	}

	#[cfg(feature = "erase-proof")]
	pub async fn prove(mut self) -> FinalizedTransaction<D> {
		let tx_unproven = self.build().await;
		tx_unproven.erase_proofs()
	}
}
