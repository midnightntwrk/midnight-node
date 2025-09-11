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

use base_crypto::{
	rng::SplittableRng,
	time::{Duration, Timestamp},
};
use coin_structure::coin::TokenType;
use ledger_storage::{Storable, arena::Sp};
use midnight_serialize::Serializable;
use mn_ledger::{
	dust::{DustActions, DustSpend},
	structure::{BindingKind, PedersenDowngradeable, ProofKind, SignatureKind},
	verify::WellFormedStrictness,
};
use rand::Rng as _;

use crate::{
	BuildIntent, ClaimKind, ClaimRewardsTransaction, DB, HashMapStorage, Intent, LedgerContext,
	Offer, OfferInfo, Pedersen, PedersenRandomness, ProofMarker, ProofPreimage,
	ProofPreimageMarker, ProofProvider, PureGeneratorPedersen, SeedableRng, Segment, SegmentId,
	Signature, StdRng, Transaction, WalletSeed, serialize,
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

pub struct StandardTrasactionInfo<D: DB + Clone> {
	pub context: Arc<LedgerContext<D>>,
	pub intents: HashMap<SegmentId, Box<dyn BuildIntent<D> + Send>>,
	pub guaranteed_coins: Option<OfferInfo<D>>,
	pub fallible_coins: HashMap<u16, OfferInfo<D>>,
	pub rng: StdRng,
	pub prover: Arc<dyn ProofProvider<D>>,
	pub funding_seeds: Vec<WalletSeed>,
	pub mock_proofs_for_fees: bool,
	pub now: Timestamp,
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
			guaranteed_coins: None,
			fallible_coins: HashMap::new(),
			rng,
			prover,
			funding_seeds: vec![],
			mock_proofs_for_fees: false,
			now,
		}
	}
}

impl<D: DB + Clone> StandardTrasactionInfo<D> {
	pub fn set_guaranteed_coins(&mut self, offer: OfferInfo<D>) {
		self.guaranteed_coins = Some(offer);
	}

	pub fn set_fallible_coins(&mut self, offers: HashMap<u16, OfferInfo<D>>) {
		self.fallible_coins = offers;
	}

	pub fn set_intents(&mut self, intents: HashMap<u16, Box<dyn BuildIntent<D> + Send>>) {
		self.intents = intents;
	}

	pub fn add_intent(&mut self, segment_id: SegmentId, intent: Box<dyn BuildIntent<D> + Send>) {
		if self.intents.insert(segment_id, intent).is_some() {
			println!("WARN: value of segment_id({segment_id}) has been replaced.");
		};
	}

	pub fn is_empty(&self) -> bool {
		self.intents.is_empty() && self.guaranteed_coins.is_none() && self.fallible_coins.is_empty()
	}

	pub fn set_wallet_seeds(&mut self, seeds: Vec<WalletSeed>) {
		self.funding_seeds = seeds;
	}

	pub fn use_mock_proofs_for_fees(&mut self, mock_proofs_for_fees: bool) {
		self.mock_proofs_for_fees = mock_proofs_for_fees;
	}

	pub async fn build(
		&mut self,
	) -> Result<Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>> {
		let now = self.now;
		// (10 min) max_ttl/6 - enough to produce 6 txs for a chain that starts
		// with the `Timestamp` of the first tx to be sent
		let delay = Duration::from_secs(600);

		let ttl = now + delay;

		let guaranteed_coins = self
			.guaranteed_coins
			.as_mut()
			.map(|gc| gc.build(&mut self.rng, self.context.clone()));

		let fallible_coins: HashMap<u16, Offer<ProofPreimage, D>> = self
			.fallible_coins
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

		let tx = Transaction::new(network_id.clone(), intents, guaranteed_coins, fallible_coins);

		// Pay the outstanding DUST balance, if we have a wallet seed to pay it
		if self.funding_seeds.is_empty() {
			return Ok(tx);
		};

		self.pay_fees(tx, network_id, now, ttl).await
	}

	async fn pay_fees(
		&mut self,
		tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>,
		network_id: String,
		now: Timestamp,
		ttl: Timestamp,
	) -> Result<Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>> {
		let tx_sealed = tx.seal(self.rng.clone());
		let Some(mut missing_dust) = self.compute_missing_dust(&tx_sealed)? else {
			// we have enough dust after all I guess
			return Ok(tx);
		};

		for _ in 0..10 {
			let speculative_payment_tx =
				self.build_dust_spend_tx(missing_dust, &network_id, now, ttl, true)?;
			let paid_tx = tx.merge(&speculative_payment_tx)?;
			let computed_missing_dust = if self.mock_proofs_for_fees {
				self.compute_missing_dust(&paid_tx.mock_prove()?)?
			} else {
				let proven_tx = self.prove_tx(paid_tx).await?.seal(self.rng.clone());
				self.compute_missing_dust(&proven_tx)?
			};
			if let Some(dust) = computed_missing_dust {
				missing_dust += dust;
			} else {
				// We know exactly how much dust is needed to balance the TX!
				// So just balance it
				let payment_tx =
					self.build_dust_spend_tx(missing_dust, &network_id, now, ttl, false)?;
				return Ok(tx.merge(&payment_tx)?);
			}
		}
		Err("Could not balance TX".into())
	}

	async fn prove_tx(
		&mut self,
		tx: Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Result<Transaction<Signature, ProofMarker, PedersenRandomness, D>> {
		let resolver = self.context.resolver().await;
		let parameters = self
			.context
			.ledger_state
			.lock()
			.map_err(|_| "ledger state lock was poisoned".to_string())?
			.parameters
			.clone();
		Ok(self
			.prover
			.prove(tx, self.rng.split(), resolver, &parameters.cost_model.runtime_cost_model)
			.await)
	}

	fn compute_missing_dust<
		P: ProofKind<D>,
		B: Storable<D> + PedersenDowngradeable<D> + Serializable,
	>(
		&self,
		tx: &Transaction<Signature, P, B, D>,
	) -> Result<Option<u128>> {
		let fees = {
			let ledger_state = self
				.context
				.ledger_state
				.lock()
				.map_err(|_| "ledger state lock was poisoned".to_string())?;
			tx.fees(&ledger_state.parameters)?
		};
		let imbalances = tx.balance(Some(fees))?;
		let dust_imbalance = imbalances
			.get(&(TokenType::Dust, Segment::Guaranteed.into()))
			.copied()
			.unwrap_or_default();
		if dust_imbalance < 0 { Ok(Some(dust_imbalance.unsigned_abs())) } else { Ok(None) }
	}

	// Builds a transaction which spends exactly `required_amount` DUST from the configured funding seeds.
	// Fails if the seeds do not have enough DUST available.
	fn build_dust_spend_tx(
		&mut self,
		required_amount: u128,
		network_id: &str,
		now: Timestamp,
		ttl: Timestamp,
		speculative: bool,
	) -> Result<Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D>> {
		let spends = self.gather_dust_spends(required_amount, now, speculative)?;
		let mut intent = Intent::empty(&mut self.rng, ttl);
		intent.dust_actions = Some(Sp::new(DustActions {
			spends: spends.into(),
			registrations: vec![].into(),
			ctime: now,
		}));
		let intents = HashMapStorage::new().insert(Segment::FeePayments.into(), intent);
		Ok(Transaction::from_intents(network_id, intents))
	}

	fn gather_dust_spends(
		&self,
		required_amount: u128,
		ctime: Timestamp,
		speculative: bool,
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
			let new_spends = if speculative {
				wallet.dust.speculative_spend(remaining, ctime, params)?
			} else {
				wallet.dust.spend(remaining, ctime, params)?
			};
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

	#[cfg(not(feature = "erase-proof"))]
	pub async fn prove(
		mut self,
	) -> Result<Transaction<Signature, ProofMarker, PureGeneratorPedersen, D>> {
		let tx_unproven = self.build().await?;
		let tx_proven = self.prove_tx(tx_unproven).await?;
		let tx_sealed = tx_proven.seal(self.rng.clone());
		Self::validate(self.context, self.now, tx_sealed)
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

	#[cfg(feature = "erase-proof")]
	pub async fn prove(self) -> Result<Transaction<D>> {
		Ok(self.erase_proof().await)
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

	pub fn build(&mut self) -> Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D> {
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

	pub async fn erase_proof(mut self) -> Transaction<(), (), Pedersen, D> {
		let tx_unproven = self.build();
		let tx_erased_proof = tx_unproven.erase_proofs();
		tx_erased_proof.erase_signatures()
	}

	#[cfg(not(feature = "erase-proof"))]
	pub async fn prove(mut self) -> Transaction<Signature, ProofMarker, PureGeneratorPedersen, D> {
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
	pub async fn prove(self) -> Transaction<D> {
		self.erase_proof().await
	}
}
