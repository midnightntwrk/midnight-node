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

use crate::{
	BuildIntent, ClaimMintTransaction, DB, HashMapStorage, Intent, LedgerContext, NetworkId, Nonce,
	Offer, OfferInfo, Pedersen, PedersenRandomness, ProofMarker, ProofPreimage,
	ProofPreimageMarker, ProofProvider, SeedableRng, SegmentId, Signature, StdRng, Transaction,
	UnshieldedTokenType, WalletSeed, serialize,
};
use std::{collections::HashMap, fs, fs::File, io::Write, sync::Arc};

pub trait FromContext<D: DB + Clone> {
	fn new_from_context(
		context: Arc<LedgerContext<D>>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
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
}

impl<D: DB + Clone> FromContext<D> for StandardTrasactionInfo<D> {
	fn new_from_context(
		context: Arc<LedgerContext<D>>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
	) -> Self {
		let rng = Self::rng(maybe_rng_seed);

		Self {
			context,
			intents: HashMap::new(),
			guaranteed_coins: None,
			fallible_coins: HashMap::new(),
			rng,
			prover,
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

	pub async fn build(
		&mut self,
	) -> Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D> {
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
			let intent = intent_info.build(&mut self.rng, self.context.clone(), *segment_id).await;
			intents = intents.insert(*segment_id, intent);
		}

		Transaction::new(intents, guaranteed_coins, fallible_coins)
	}

	pub async fn save_intents_to_file(
		mut self,
		network: NetworkId,
		parent_dir: &str,
		file_name: &str,
	) {
		// make sure that the dir is created, if it does not exist
		fs::create_dir_all(parent_dir).expect("failed to create directory");

		for (segment_id, intent_info) in self.intents.iter_mut() {
			let intent = intent_info.build(&mut self.rng, self.context.clone(), *segment_id).await;
			println!("Serializing intent...");
			match serialize(&intent, network) {
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

	pub async fn erase_proof(mut self) -> Transaction<(), (), Pedersen, D> {
		let tx_unproven = self.build().await;
		let tx_erased_proof = tx_unproven.erase_proofs();
		tx_erased_proof.erase_signatures()
	}

	#[cfg(not(feature = "erase-proof"))]
	pub async fn prove(mut self) -> Transaction<Signature, ProofMarker, PedersenRandomness, D> {
		let tx_unproven = self.build().await;
		let resolver = self.context.resolver().await;
		self.prover.prove(tx_unproven, self.rng, resolver).await
	}

	#[cfg(feature = "erase-proof")]
	pub async fn prove(self) -> Transaction<D> {
		self.erase_proof().await
	}
}

#[derive(Default)]
pub struct MintCoinInfo {
	pub origin: WalletSeed,
	pub token_type: UnshieldedTokenType,
	pub value: u128,
	pub nonce: Nonce,
}

pub struct ClaimMintInfo<D: DB + Clone> {
	pub context: Arc<LedgerContext<D>>,
	pub coin: MintCoinInfo,
	pub rng: StdRng,
	pub prover: Arc<dyn ProofProvider<D>>,
}

impl<D: DB + Clone> FromContext<D> for ClaimMintInfo<D> {
	fn new_from_context(
		context: Arc<LedgerContext<D>>,
		prover: Arc<dyn ProofProvider<D>>,
		maybe_rng_seed: Option<[u8; 32]>,
	) -> Self {
		let rng = Self::rng(maybe_rng_seed);

		Self { context, coin: MintCoinInfo::default(), rng, prover }
	}
}

impl<D: DB + Clone> ClaimMintInfo<D> {
	pub fn set_coin(&mut self, mint_coin: MintCoinInfo) {
		self.coin = mint_coin
	}

	pub fn build(&mut self) -> Transaction<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		self.context.with_ledger_state(|ledger_state| {
			let mint_cost = ledger_state.parameters.cost_model.mint_cost as u128;

			let amount = self.coin.value - mint_cost;

			let claim_mint = self.context.with_wallet_from_seed(self.coin.origin, |wallet| {
				let unsigend_claim_mint: ClaimMintTransaction<(), D> = ClaimMintTransaction {
					value: amount,
					type_: self.coin.token_type,
					owner: wallet.unshielded.signing_key().verifying_key(),
					nonce: self.coin.nonce,
					signature: (),
				};

				let data_to_sign = unsigend_claim_mint.data_to_sign();
				let signature = wallet.unshielded.signing_key().sign(&mut self.rng, &data_to_sign);

				ClaimMintTransaction {
					value: amount,
					type_: self.coin.token_type,
					owner: wallet.unshielded.signing_key().verifying_key(),
					nonce: self.coin.nonce,
					signature,
				}
			});

			Transaction::ClaimMint(claim_mint)
		})
	}

	pub async fn erase_proof(mut self) -> Transaction<(), (), Pedersen, D> {
		let tx_unproven = self.build();
		let tx_erased_proof = tx_unproven.erase_proofs();
		tx_erased_proof.erase_signatures()
	}

	#[cfg(not(feature = "erase-proof"))]
	pub async fn prove(mut self) -> Transaction<Signature, ProofMarker, PedersenRandomness, D> {
		let tx_unproven = self.build();
		let resolver = self.context.resolver().await;
		self.prover.prove(tx_unproven, self.rng, resolver).await
	}

	#[cfg(feature = "erase-proof")]
	pub async fn prove(self) -> Transaction<D> {
		self.erase_proof().await
	}
}
