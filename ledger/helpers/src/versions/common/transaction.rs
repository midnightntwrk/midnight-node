// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

#![cfg(feature = "can-panic")]

use rand::Rng as _;

use super::{
	BindingKind, BuildIntent, ClaimKind, ClaimRewardsTransaction, DB, Duration, DustActions,
	DustPublicKey, DustRegistration, DustSpend, HashMapStorage, Intent, LedgerContext, Offer,
	OfferInfo, Pedersen, PedersenDowngradeable, PedersenRandomness, ProofKind, ProofMarker,
	ProofPreimage, ProofPreimageMarker, ProofProvider, PureGeneratorPedersen, SeedableRng, Segment,
	SegmentId, Serializable, Signature, SignatureKind, SigningKey, Sp, SplittableRng, StdRng,
	Storable, Timestamp, TokenType, Transaction, WalletSeed, WellFormedStrictness, serialize,
};
use std::{
	collections::HashMap,
	error::Error,
	fs,
	fs::File,
	io::Write,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

type UnprovenTransaction<D> = Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>;
#[cfg(not(feature = "erase-proof"))]
type FinalizedTransaction<D> = Transaction<Signature, ProofMarker, PureGeneratorPedersen, D>;
#[cfg(feature = "erase-proof")]
type FinalizedTransaction<D> = Transaction<Signature, (), Pedersen, D>;

type Result<T, E = Box<dyn Error + Send + Sync>> = std::result::Result<T, E>;

pub trait FromContext<D: DB + Clone> {
	fn new_from_context(
		context: Arc<LedgerContext<D>>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
		now: Option<Timestamp>,
	) -> Self;

	fn rng(maybe_rng_seed: Option<[u8; 32]>) -> StdRng {
		maybe_rng_seed.map(StdRng::from_seed).unwrap_or(StdRng::from_entropy())
	}
}

pub struct DustRegistrationBuilder {
	pub signing_key: SigningKey,
	pub dust_address: Option<DustPublicKey>,
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
		let night_key = self.signing_key.verifying_key();

		DustRegistration {
			night_key,
			dust_address: self.dust_address.map(|address| Sp::new(address)),
			allow_fee_payment: 0,
			signature: Some(Sp::new(signature)),
		}
	}
}

pub struct StandardTrasactionInfo<D: DB + Clone> {
	pub context: Arc<LedgerContext<D>>,
	pub intents: HashMap<SegmentId, Box<dyn BuildIntent<D>>>,
	pub guaranteed_offer: Option<OfferInfo<D>>,
	pub fallible_offers: HashMap<u16, OfferInfo<D>>,
	pub rng: StdRng,
	pub prover: Arc<dyn ProofProvider<D>>,
	pub funding_seeds: Vec<WalletSeed>,
	pub mock_proofs_for_fees: bool,
	pub now: Timestamp,
	pub dust_registrations: Vec<DustRegistrationBuilder>,
}

impl<D: DB + Clone> FromContext<D> for StandardTrasactionInfo<D> {
	fn new_from_context(
		context: Arc<LedgerContext<D>>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
		now: Option<Timestamp>,
	) -> Self {
		let rng = Self::rng(maybe_rng_seed);
		let now = now.unwrap_or_else(|| {
			Timestamp::from_secs(
				SystemTime::now()
					.duration_since(UNIX_EPOCH)
					.expect("time went backwards")
					.as_secs(),
			)
		});

		Self {
			context,
			intents: HashMap::new(),
			guaranteed_offer: None,
			fallible_offers: HashMap::new(),
			rng,
			prover,
			funding_seeds: vec![],
			mock_proofs_for_fees: false,
			now,
			dust_registrations: vec![],
		}
	}
}

impl<D: DB + Clone> StandardTrasactionInfo<D> {
	pub fn set_guaranteed_offer(&mut self, offer: OfferInfo<D>) {
		self.guaranteed_offer = Some(offer);
	}

	pub fn set_fallible_offers(&mut self, offers: HashMap<u16, OfferInfo<D>>) {
		self.fallible_offers = offers;
	}

	pub fn set_intents(&mut self, intents: HashMap<u16, Box<dyn BuildIntent<D>>>) {
		self.intents = intents;
	}

	pub fn add_intent(&mut self, segment_id: SegmentId, intent: Box<dyn BuildIntent<D>>) {
		if self.intents.insert(segment_id, intent).is_some() {
			println!("WARN: value of segment_id({segment_id}) has been replaced.");
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

	pub fn set_wallet_seeds(&mut self, seeds: Vec<WalletSeed>) {
		self.funding_seeds = seeds;
	}

	pub fn use_mock_proofs_for_fees(&mut self, mock_proofs_for_fees: bool) {
		self.mock_proofs_for_fees = mock_proofs_for_fees;
	}

	async fn build(&mut self) -> Result<FinalizedTransaction<D>> {
		let now = self.now;
		// (10 min) max_ttl/6 - enough to produce 6 txs for a chain that starts
		// with the `Timestamp` of the first tx to be sent
		let delay = Duration::from_secs(600);

		let ttl = now + delay;

		let guaranteed_offer: Option<Offer<ProofPreimage, D>> = self
			.guaranteed_offer
			.as_mut()
			.map(|gc| gc.build(&mut self.rng, self.context.clone()));

		let fallible_offer: HashMap<u16, Offer<ProofPreimage, D>> = self
			.fallible_offers
			.iter_mut()
			.map(|(segment_id, offer_info)| {
				(*segment_id, offer_info.build(&mut self.rng, self.context.clone()))
			})
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

		let network_id = {
			let guard = self
				.context
				.ledger_state
				.lock()
				.map_err(|_| "ledger state lock was poisoned".to_string())?;
			guard.network_id.clone()
		};

		let tx = Transaction::new(network_id.clone(), intents, guaranteed_offer, fallible_offer);

		println!("pre-proof tx: {tx:#?}");
		println!("tx balance pre-fees: {:#?}", tx.balance(None));

		// Pay the outstanding DUST balance, if we have a wallet seed to pay it
		if self.funding_seeds.is_empty() {
			return self.prove_tx(tx).await;
		};

		let tx = self.pay_fees(tx, now, ttl).await?;
		let fees = self.context.with_ledger_state(|s| tx.fees_with_margin(&s.parameters, 3))?;
		println!("tx-balance post-prove: {:#?}", tx.balance(Some(fees))?);
		Ok(tx)
	}

	async fn pay_fees(
		&mut self,
		tx: UnprovenTransaction<D>,
		now: Timestamp,
		ttl: Timestamp,
	) -> Result<FinalizedTransaction<D>> {
		let mut missing_dust = 0;

		for _ in 0..10 {
			let spends = self.gather_dust_spends(missing_dust, now)?;
			let mut paid_tx = tx.clone();
			self.apply_dust(&mut paid_tx, &spends, self.rng.clone().split(), now, ttl);

			if self.mock_proofs_for_fees {
				let mock_proven_tx = self.mock_prove_tx(&paid_tx)?;
				let computed_missing_dust = self.compute_missing_dust(&mock_proven_tx)?;
				if let Some(dust) = computed_missing_dust {
					missing_dust += dust;
				} else {
					self.confirm_dust_spends(&spends)?;
					return self.prove_tx(paid_tx).await;
				}
			} else {
				let proven_tx = self.prove_tx(paid_tx).await?;
				let computed_missing_dust = self.compute_missing_dust(&proven_tx)?;
				if let Some(dust) = computed_missing_dust {
					missing_dust += dust;
				} else {
					self.confirm_dust_spends(&spends)?;
					return Ok(proven_tx);
				}
			}
		}
		Err("Could not balance TX".into())
	}

	#[cfg(not(feature = "erase-proof"))]
	async fn prove_tx(&mut self, tx: UnprovenTransaction<D>) -> Result<FinalizedTransaction<D>> {
		let resolver = self.context.resolver().await;
		let parameters = self
			.context
			.ledger_state
			.lock()
			.map_err(|_| "ledger state lock was poisoned".to_string())?
			.parameters
			.clone();
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

	fn compute_missing_dust(&self, tx: &FinalizedTransaction<D>) -> Result<Option<u128>> {
		let fees = self.context.with_ledger_state(|s| tx.fees_with_margin(&s.parameters, 3))?;
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
	}

	fn gather_dust_spends(
		&self,
		required_amount: u128,
		ctime: Timestamp,
	) -> Result<Vec<DustSpend<ProofPreimageMarker, D>>> {
		let mut spends = vec![];
		let mut remaining = required_amount;
		let state = self
			.context
			.ledger_state
			.lock()
			.map_err(|_| "ledger state lock was poisoned".to_string())?;
		let params = &state.parameters.dust;
		let mut wallets = self
			.context
			.wallets
			.lock()
			.map_err(|_| "wallet lock was poisoned".to_string())?;
		for seed in &self.funding_seeds {
			if remaining == 0 {
				return Ok(spends);
			}
			let wallet = wallets.get_mut(seed).ok_or("Unrecognized wallet seed")?;
			let new_spends = wallet.dust.speculative_spend(remaining, ctime, params)?;
			// We asked the wallet to spend `remaining` DUST,
			// so the total amount spent will be <= `remaining`.
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
			Ok(spends)
		}
	}

	fn confirm_dust_spends(&mut self, spends: &[DustSpend<ProofPreimageMarker, D>]) -> Result<()> {
		let mut wallets = self
			.context
			.wallets
			.lock()
			.map_err(|_| "wallet lock was poisoned".to_string())?;
		for wallet in wallets.values_mut() {
			wallet.dust.mark_spent(spends);
		}
		Ok(())
	}

	pub async fn save_intents_to_file(mut self, parent_dir: &str, file_name: &str) {
		// make sure that the dir is created, if it does not exist
		fs::create_dir_all(parent_dir).expect("failed to create directory");

		let now = self.now;
		let ttl = now + Duration::from_secs(600);

		for (segment_id, intent_info) in self.intents.iter_mut() {
			let intent =
				intent_info.build(&mut self.rng, ttl, self.context.clone(), *segment_id).await;
			println!("Serializing intent...");
			match serialize(&intent) {
				Ok(serialized_intent) => {
					let complete_file_name =
						format!("{parent_dir}/{segment_id}_{file_name}_intent.mn");

					let mut file =
						File::create(&complete_file_name).expect("failed to create file");
					file.write_all(&serialized_intent).expect("failed to write file");

					println!("Saved {complete_file_name}");
				},
				Err(e) => {
					println!("error({e:?}): failed to save to file {intent:#?}");
				},
			}
		}
	}

	pub async fn erase_proof(mut self) -> Result<Transaction<(), (), Pedersen, D>> {
		let tx_unproven = self.build().await?;
		let tx_erased_proof = tx_unproven.erase_proofs();
		Self::validate(self.context, self.now, tx_erased_proof.erase_signatures())
	}

	pub async fn prove(mut self) -> Result<FinalizedTransaction<D>> {
		let tx = self.build().await?;
		Self::validate(self.context, self.now, tx)
	}

	fn validate<
		S: SignatureKind<D>,
		P: ProofKind<D> + Storable<D>,
		B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D>,
	>(
		context: Arc<LedgerContext<D>>,
		now: Timestamp,
		tx: Transaction<S, P, B, D>,
	) -> Result<Transaction<S, P, B, D>> {
		let ref_state = context
			.ledger_state
			.lock()
			.map_err(|_| "ledger state lock was poisoned".to_string())?
			.clone();
		tx.well_formed(&ref_state, WellFormedStrictness::default(), now)?;
		Ok(tx)
	}
}

#[derive(Default)]
pub struct RewardsInfo {
	pub owner: WalletSeed,
	pub value: u128,
}

pub struct ClaimMintInfo<D: DB + Clone> {
	pub context: Arc<LedgerContext<D>>,
	pub coin: RewardsInfo,
	pub rng: StdRng,
	pub prover: Arc<dyn ProofProvider<D>>,
}

impl<D: DB + Clone> FromContext<D> for ClaimMintInfo<D> {
	fn new_from_context(
		context: Arc<LedgerContext<D>>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
		_now: Option<Timestamp>,
	) -> Self {
		let rng = Self::rng(maybe_rng_seed);

		Self { context, coin: RewardsInfo::default(), rng, prover }
	}
}

impl<D: DB + Clone> ClaimMintInfo<D> {
	pub fn set_rewards(&mut self, rewards: RewardsInfo) {
		self.coin = rewards;
	}

	fn build(&mut self) -> UnprovenTransaction<D> {
		let nonce = self.rng.r#gen();
		self.context.with_ledger_state(|ledger_state| {
			let claim_rewards = self.context.with_wallet_from_seed(self.coin.owner, |wallet| {
				let unsigned_claim_mint: ClaimRewardsTransaction<(), D> = ClaimRewardsTransaction {
					network_id: ledger_state.network_id.clone(),
					value: self.coin.value,
					owner: wallet.unshielded.signing_key().verifying_key(),
					nonce,
					signature: (),
					kind: ClaimKind::Reward,
				};

				let data_to_sign = unsigned_claim_mint.data_to_sign();
				let signature = wallet.unshielded.signing_key().sign(&mut self.rng, &data_to_sign);

				ClaimRewardsTransaction {
					network_id: ledger_state.network_id.clone(),
					value: self.coin.value,
					owner: wallet.unshielded.signing_key().verifying_key(),
					nonce,
					signature,
					kind: ClaimKind::Reward,
				}
			});

			Transaction::ClaimRewards(claim_rewards)
		})
	}

	#[cfg(not(feature = "erase-proof"))]
	pub async fn prove(mut self) -> FinalizedTransaction<D> {
		let tx_unproven = self.build();
		let resolver = self.context.resolver().await;
		let parameters = self
			.context
			.ledger_state
			.lock()
			.expect("ledger state lock was poisoned")
			.parameters
			.clone();
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
	pub async fn prove(self) -> FinalizedTransaction<D> {
		let tx_unproven = self.build();
		tx_unproven.erase_proofs()
	}
}
