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
	BuildContractAction, DB, Deserializable, Intent, KeyLocation, LedgerContext, NetworkId,
	PUBLIC_PARAMS, PedersenRandomness, ProofPreimageMarker, ProvingData, Resolver, Signature,
	StdRng, Timestamp, UnshieldedOfferInfo, deserialize,
};
use async_trait::async_trait;
use std::{
	fs::File,
	io::{self, BufReader, Read},
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

pub type SegmentId = u16;

type IntentOf<D> = Intent<Signature, ProofPreimageMarker, PedersenRandomness, D>;
#[async_trait]
pub trait BuildIntent<D: DB + Clone> {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
		segment_id: SegmentId,
	) -> IntentOf<D>;
}

pub struct IntentInfo<D: DB + Clone> {
	pub guaranteed_unshielded_offer: Option<UnshieldedOfferInfo<D>>,
	pub fallible_unshielded_offer: Option<UnshieldedOfferInfo<D>>,
	pub actions: Vec<Box<dyn BuildContractAction<D> + Send>>,
}

#[async_trait]
impl<D: DB + Clone> BuildIntent<D> for IntentInfo<D> {
	async fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
		segment_id: SegmentId,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_secs();
		// (10 min) max_ttl/6 - enough to produce 6 txs for a chain that starts
		// with the `Timestamp` of the first tx to be sent
		let delay: u64 = 600;

		let ttl = now + delay;

		let mut intent = Intent::<Signature, _, _, _>::empty(rng, Timestamp::from_secs(ttl));

		for action in self.actions.iter_mut() {
			intent = action.build(rng, context.clone(), &intent).await;
		}

		intent.guaranteed_unshielded_offer =
			self.guaranteed_unshielded_offer.as_ref().map(|guo| guo.build(context.clone()));
		intent.fallible_unshielded_offer =
			self.fallible_unshielded_offer.as_ref().map(|fuo| fuo.build(context.clone()));

		let guaranteed_signing_keys = &[];
		let fallible_signing_keys = &[];

		intent
			.sign::<Signature, StdRng>(
				rng,
				segment_id,
				guaranteed_signing_keys,
				fallible_signing_keys,
			)
			.unwrap_or_else(|_| panic!("Intent signing with segment_id {segment_id:?} failed"))
	}
}

pub struct IntentCustom {
	pub intent_path: String,
	pub network: NetworkId,
	pub resolver: &'static Resolver,
}

impl IntentCustom {
	pub fn get_resolver(parent_dir: String) -> Resolver {
		Resolver::new(
			PUBLIC_PARAMS.clone(),
			Box::new(move |KeyLocation(loc)| {
				let files = [("keys", "prover"), ("keys", "verifier"), ("zkir", "bzkir")]
					.iter()
					.map(|(dir, ext)| format!("{parent_dir}/{dir}/{loc}.{ext}"))
					.map(|path| {
						println!("Resolver: reading {path}...");
						Ok::<_, io::Error>(BufReader::new(File::open(path)?))
					})
					.collect::<Result<Vec<_>, _>>();
				let mut files = match files {
					Ok(f) => f,
					Err(e) => {
						return Box::pin(std::future::ready(
							if io::ErrorKind::NotFound == e.kind() { Ok(None) } else { Err(e) },
						));
					},
				};
				let pk = Deserializable::deserialize(&mut files[0], 0).expect("cannot deserialize");
				let vk = Deserializable::deserialize(&mut files[1], 0).expect("cannot deserialize");
				let ir = Deserializable::deserialize(&mut files[2], 0).expect("cannot deserialize");
				Box::pin(std::future::ready(Ok(Some(ProvingData::V4(pk, vk, ir)))))
			}),
		)
	}
}

#[async_trait]
impl<D: DB + Clone> BuildIntent<D> for IntentCustom {
	async fn build(
		&mut self,
		_rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
		_segment_id: SegmentId,
	) -> IntentOf<D> {
		println!("Updating the resolver...");
		context.update_resolver(self.resolver).await;

		let mut bytes = vec![];

		let mut file = File::open(&self.intent_path).expect("Could not open file");
		file.read_to_end(&mut bytes).expect("Failed to read file");

		deserialize(bytes.as_slice(), self.network).expect("failed to deserialize")
	}
}
