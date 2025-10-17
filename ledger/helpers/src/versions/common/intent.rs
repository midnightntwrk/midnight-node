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

use super::{
	Array, BuildContractAction, ContractAction, ContractEffects, DB, DUST_EXPECTED_FILES,
	DustResolver, FetchMode, Intent, KeyLocation, LedgerContext, MidnightDataProvider, OutputMode,
	PUBLIC_PARAMS, PedersenRandomness, ProofPreimageMarker, ProvingKeyMaterial, Resolver,
	Signature, StdRng, Timestamp, UnshieldedOfferInfo, deserialize,
};
use async_trait::async_trait;
use rand::{CryptoRng, Rng};
use std::{
	io,
	path::Path,
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

pub type SegmentId = u16;

type IntentOf<D> = Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>;
#[async_trait]
pub trait BuildIntent<D: DB + Clone>: Send + Sync {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<LedgerContext<D>>,
		segment_id: SegmentId,
	) -> IntentOf<D>;
}

pub struct IntentInfo<D: DB + Clone> {
	pub guaranteed_unshielded_offer: Option<UnshieldedOfferInfo<D>>,
	pub fallible_unshielded_offer: Option<UnshieldedOfferInfo<D>>,
	pub actions: Vec<Box<dyn BuildContractAction<D>>>,
	// TODO: Add TTL Option here
}

#[async_trait]
impl<D: DB + Clone> BuildIntent<D> for IntentInfo<D> {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<LedgerContext<D>>,
		segment_id: SegmentId,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let mut intent = Intent::<Signature, _, _, _>::empty(rng, ttl);

		for action in self.actions.iter_mut() {
			intent = action.build(rng, context.clone(), &intent).await;
		}

		let mut guaranteed_signing_keys = Vec::default();
		let mut fallible_signing_keys = Vec::default();
		let dust_registration_signing_keys = Vec::default();

		if let Some((unshielded_offer, signing_keys)) =
			self.guaranteed_unshielded_offer.as_ref().map(|guo| {
				(
					guo.build(context.clone()),
					guo.inputs
						.iter()
						.map(|input| input.signing_key(context.clone()))
						.collect::<Vec<_>>(),
				)
			}) {
			intent.guaranteed_unshielded_offer = Some(unshielded_offer);
			guaranteed_signing_keys = signing_keys;
		}

		if let Some((unshielded_offer, signing_keys)) =
			self.fallible_unshielded_offer.as_ref().map(|guo| {
				(
					guo.build(context.clone()),
					guo.inputs
						.iter()
						.map(|input| input.signing_key(context.clone()))
						.collect::<Vec<_>>(),
				)
			}) {
			intent.fallible_unshielded_offer = Some(unshielded_offer);
			fallible_signing_keys = signing_keys;
		}

		intent
			.sign(
				rng,
				segment_id,
				guaranteed_signing_keys.as_slice(),
				fallible_signing_keys.as_slice(),
				dust_registration_signing_keys.as_slice(),
			)
			.unwrap_or_else(|_| panic!("Intent signing with segment_id {segment_id:?} failed"))
	}
}

pub struct IntentCustom<D: DB + Clone> {
	pub intent: IntentOf<D>,
	pub resolver: &'static Resolver,
}

impl<D: DB + Clone> IntentCustom<D> {
	pub fn new_from_file(
		path: impl AsRef<Path>,
		resolver: &'static Resolver,
	) -> Result<Self, std::io::Error> {
		let bytes = std::fs::read(path)?;
		let intent: IntentOf<D> = deserialize(bytes.as_slice()).expect("failed to deserialize");
		Ok(Self { intent, resolver })
	}

	pub fn new_from_actions<R: Rng + CryptoRng + ?Sized>(
		rng: &mut R,
		actions: &[ContractAction<ProofPreimageMarker, D>],
		resolver: &'static Resolver,
	) -> Self {
		let now = Timestamp::from_secs(
			SystemTime::now()
				.duration_since(UNIX_EPOCH)
				.expect("time has run backwards")
				.as_secs(),
		);
		let intent = Intent {
			guaranteed_unshielded_offer: None,
			fallible_unshielded_offer: None,
			actions: Array::new_from_slice(actions),
			dust_actions: None,
			ttl: now,
			binding_commitment: rng.r#gen(),
		};
		Self { intent, resolver }
	}

	pub fn find_effects(&self) -> (Option<ContractEffects<D>>, Option<ContractEffects<D>>) {
		let mut guaranteed_effects: Option<ContractEffects<D>> = None;
		let mut fallible_effects: Option<ContractEffects<D>> = None;
		for action in self.intent.actions.iter() {
			if let ContractAction::Call(ref c) = *action.clone() {
				if let Some(ref t) = c.guaranteed_transcript {
					guaranteed_effects = Some(t.effects.clone());
				}
				if let Some(ref t) = c.fallible_transcript {
					fallible_effects = Some(t.effects.clone());
				}
			}
		}
		(guaranteed_effects, fallible_effects)
	}

	pub fn get_resolver(parent_dir: String) -> Result<Resolver, std::io::Error> {
		Ok(Resolver::new(
			PUBLIC_PARAMS.clone(),
			DustResolver(MidnightDataProvider::new(
				FetchMode::OnDemand,
				OutputMode::Log,
				DUST_EXPECTED_FILES.to_owned(),
			)?),
			Box::new(move |KeyLocation(loc)| {
				let sync_block = || {
					let read_file = |dir, ext| {
						let path = format!("{parent_dir}/{dir}/{loc}.{ext}");
						match std::fs::read(&path) {
							Err(e) if e.kind() == io::ErrorKind::NotFound => {
								println!("Resolver: missing key at path {path}");
								Ok(None)
							},
							Err(e) => {
								println!("Resolver: error reading key at path {path}: {e}");
								Err(e)
							},
							Ok(v) => {
								println!("Resolver: found key at path {path}");
								Ok(Some(v))
							},
						}
					};
					let Some(prover_key) = read_file("keys", "prover")? else {
						println!("WARN: prover key not created");
						return Ok(None);
					};
					let Some(verifier_key) = read_file("keys", "verifier")? else {
						println!("WARN: verifier key not created");
						return Ok(None);
					};
					let Some(ir_source) = read_file("zkir", "bzkir")? else {
						println!("WARN:  ir source not created");
						return Ok(None);
					};

					println!("Creating Proving Key Material...");

					Ok(Some(ProvingKeyMaterial { prover_key, verifier_key, ir_source }))
				};
				let res = sync_block();
				Box::pin(std::future::ready(res))
			}),
		))
	}
}

#[async_trait]
impl<D: DB + Clone> BuildIntent<D> for IntentCustom<D> {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<LedgerContext<D>>,
		_segment_id: SegmentId,
	) -> IntentOf<D> {
		println!("Updating the resolver...");
		context.update_resolver(self.resolver).await;
		let mut intent = self.intent.clone();
		intent.ttl = ttl;
		println!("custom intent: {intent:#?}");
		intent
	}
}

#[async_trait]
impl<D: DB + Clone> BuildContractAction<D> for IntentCustom<D> {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let mut actions = intent.actions.clone();

		for action in self.intent.actions.iter() {
			actions = actions.push((*action).clone());
		}

		context.update_resolver(self.resolver).await;

		IntentOf::<D> {
			guaranteed_unshielded_offer: intent.guaranteed_unshielded_offer.clone(),
			fallible_unshielded_offer: intent.fallible_unshielded_offer.clone(),
			actions,
			dust_actions: intent.dust_actions.clone(),
			ttl: intent.ttl,
			binding_commitment: intent.binding_commitment,
		}
	}
}
