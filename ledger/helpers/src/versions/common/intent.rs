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

use super::{
	Array, BuildContractAction, BuilderContext, ContractAction, ContractAddress, ContractEffects,
	DB, DUST_EXPECTED_FILES, DustResolver, FetchMode, Intent, KeyLocation, MidnightDataProvider,
	OutputMode, PUBLIC_PARAMS, PedersenRandomness, ProofPreimageMarker, ProvingKeyMaterial,
	Resolver, Signature, SigningKey, StdRng, Timestamp, UnshieldedOfferInfo, deserialize,
	transaction_signing_key,
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
pub trait BuildIntent<D: DB + Clone, C: BuilderContext<D>>: Send + Sync {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<C>,
		segment_id: SegmentId,
	) -> IntentOf<D>;

	/// Signing keys for the unshielded offers this intent carries, returned as
	/// `(guaranteed, fallible)` in the same order the offer inputs are built.
	///
	/// `StandardTrasactionInfo::apply_dust` uses these to re-sign the offers after it
	/// attaches `dust_actions`: since ledger 9.1.0-rc.3, the dust fields are folded into the
	/// intent's `data_to_sign`, so the signatures produced by [`Self::build`] (before the
	/// dust existed) no longer match. Intents with no unshielded offer (the default) return
	/// empty vectors.
	fn unshielded_signing_keys(&self, _context: Arc<C>) -> (Vec<SigningKey>, Vec<SigningKey>) {
		(Vec::new(), Vec::new())
	}
}

pub struct IntentInfo<D: DB + Clone, C: BuilderContext<D>> {
	pub guaranteed_unshielded_offer: Option<UnshieldedOfferInfo<D, C>>,
	pub fallible_unshielded_offer: Option<UnshieldedOfferInfo<D, C>>,
	pub actions: Vec<Box<dyn BuildContractAction<D, C>>>,
	// TODO: Add TTL Option here
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildIntent<D, C> for IntentInfo<D, C> {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<C>,
		segment_id: SegmentId,
	) -> IntentOf<D> {
		let mut intent = Intent::<Signature, _, _, _>::empty(rng, ttl);

		for action in self.actions.iter_mut() {
			let next = action.build(rng, context.clone(), &intent).await;
			intent = next;
		}

		let mut guaranteed_signing_keys = Vec::default();
		let mut fallible_signing_keys = Vec::default();
		let dust_registration_signing_keys = Vec::default();

		if let Some(ref guaranteed_unshielded_offer) = self.guaranteed_unshielded_offer {
			let unshielded_offer = guaranteed_unshielded_offer.build(context.clone()).await;
			let signing_keys = guaranteed_unshielded_offer
				.inputs
				.iter()
				.map(|input| input.signing_key(context.clone()))
				.collect::<Vec<_>>();
			intent.guaranteed_unshielded_offer = Some(unshielded_offer);
			guaranteed_signing_keys = signing_keys;
		}

		if let Some(ref fallible_unshielded_offer) = self.fallible_unshielded_offer {
			let unshielded_offer = fallible_unshielded_offer.build(context.clone()).await;
			let signing_keys = fallible_unshielded_offer
				.inputs
				.iter()
				.map(|input| input.signing_key(context.clone()))
				.collect::<Vec<_>>();
			intent.fallible_unshielded_offer = Some(unshielded_offer);
			fallible_signing_keys = signing_keys;
		}

		let guaranteed_signing_keys =
			guaranteed_signing_keys.iter().map(transaction_signing_key).collect::<Vec<_>>();
		let fallible_signing_keys =
			fallible_signing_keys.iter().map(transaction_signing_key).collect::<Vec<_>>();

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

	fn unshielded_signing_keys(&self, context: Arc<C>) -> (Vec<SigningKey>, Vec<SigningKey>) {
		let signing_keys = |offer: &Option<UnshieldedOfferInfo<D, C>>| {
			offer
				.as_ref()
				.map(|offer| {
					offer.inputs.iter().map(|input| input.signing_key(context.clone())).collect()
				})
				.unwrap_or_default()
		};

		(
			signing_keys(&self.guaranteed_unshielded_offer),
			signing_keys(&self.fallible_unshielded_offer),
		)
	}
}

#[derive(Clone)]
pub struct IntentCustom<D: DB + Clone> {
	pub intent: IntentOf<D>,
	pub resolver: &'static Resolver,
}

impl<D: DB + Clone> IntentCustom<D> {
	/// Maximum file size for intent files (64 MB)
	const MAX_INTENT_FILE_SIZE: u64 = 64 * 1024 * 1024;

	pub fn new_from_file(
		path: impl AsRef<Path>,
		resolver: &'static Resolver,
	) -> Result<Self, std::io::Error> {
		let metadata = std::fs::metadata(path.as_ref())?;
		if metadata.len() > Self::MAX_INTENT_FILE_SIZE {
			return Err(std::io::Error::new(
				std::io::ErrorKind::InvalidData,
				format!("intent file exceeds maximum size of {} bytes", Self::MAX_INTENT_FILE_SIZE),
			));
		}
		let bytes = std::fs::read(path)?;
		let intent: IntentOf<D> = deserialize(bytes.as_slice())?;
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

	pub fn find_effects(&self) -> (Vec<ContractEffects<D>>, Vec<ContractEffects<D>>) {
		let mut guaranteed_effects = vec![];
		let mut fallible_effects = vec![];
		for action in self.intent.actions.iter() {
			if let ContractAction::Call(ref c) = *action.clone() {
				if let Some(ref t) = c.guaranteed_transcript {
					guaranteed_effects.push(t.effects.clone());
				}
				if let Some(ref t) = c.fallible_transcript {
					fallible_effects.push(t.effects.clone());
				}
			}
		}
		(guaranteed_effects, fallible_effects)
	}

	pub fn find_contract_address(&self) -> Option<ContractAddress> {
		self.intent.actions.iter().find_map(|action| match *action {
			ContractAction::Call(ref c) => Some(c.address),
			ContractAction::Maintain(ref c) => Some(c.address),
			_ => None,
		})
	}

	pub fn get_resolver(artifact_dirs: &[String]) -> Result<Resolver, std::io::Error> {
		let artifact_dirs = artifact_dirs.to_vec();
		Ok(Resolver::new(
			PUBLIC_PARAMS.clone(),
			DustResolver(MidnightDataProvider::new(
				FetchMode::OnDemand,
				OutputMode::Log,
				DUST_EXPECTED_FILES.to_owned(),
			)?),
			Box::new(move |KeyLocation(loc)| {
				let artifact_dirs = artifact_dirs.to_vec();
				let sync_block = move || {
					let read_file = |dir, ext| {
						for parent_dir in &artifact_dirs {
							let path = format!("{parent_dir}/{dir}/{loc}.{ext}");
							match std::fs::read(&path) {
								Err(e) if e.kind() == io::ErrorKind::NotFound => {
									log::debug!("Resolver: missing key at path {path}");
									continue;
								},
								Err(e) => {
									log::error!("Resolver: error reading key at path {path}: {e}");
									return Err(e);
								},
								Ok(v) => {
									log::debug!("Resolver: found key at path {path}");
									return Ok(Some(v));
								},
							}
						}
						Ok(None)
					};
					let Some(prover_key) = read_file("keys", "prover")? else {
						log::warn!("prover key not created");
						return Ok(None);
					};
					let Some(verifier_key) = read_file("keys", "verifier")? else {
						log::warn!("verifier key not created");
						return Ok(None);
					};
					let Some(ir_source) = read_file("zkir", "bzkir")? else {
						log::warn!("IR source not created");
						return Ok(None);
					};

					log::info!("Creating Proving Key Material...");

					Ok(Some(ProvingKeyMaterial { prover_key, verifier_key, ir_source }))
				};
				let res = sync_block();
				Box::pin(std::future::ready(res))
			}),
		))
	}
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildIntent<D, C> for IntentCustom<D> {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		ttl: Timestamp,
		context: Arc<C>,
		_segment_id: SegmentId,
	) -> IntentOf<D> {
		log::debug!("Updating the resolver...");
		context.update_resolver(self.resolver).await;
		let mut intent = self.intent.clone();
		intent.ttl = ttl;
		intent
	}
}

#[async_trait]
impl<D: DB + Clone, C: BuilderContext<D>> BuildContractAction<D, C> for IntentCustom<D> {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		context: Arc<C>,
		intent: &Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let mut actions = intent.actions.clone();

		for action in self.intent.actions.iter() {
			actions = actions.push((*action).clone());
		}

		let result = IntentOf::<D> {
			guaranteed_unshielded_offer: intent.guaranteed_unshielded_offer.clone(),
			fallible_unshielded_offer: intent.fallible_unshielded_offer.clone(),
			actions,
			dust_actions: intent.dust_actions.clone(),
			ttl: intent.ttl,
			binding_commitment: intent.binding_commitment,
		};

		context.update_resolver(self.resolver).await;
		result
	}
}
