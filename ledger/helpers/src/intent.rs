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
	BuildContractAction, DB, Intent, LedgerContext, PedersenRandomness, ProofPreimageMarker,
	Signature, StdRng, Timestamp, UnshieldedOfferInfo,
};
use std::{
	sync::Arc,
	time::{SystemTime, UNIX_EPOCH},
};

pub type SegmentId = u16;

pub struct IntentInfo<D: DB + Clone> {
	pub guaranteed_unshielded_offer: Option<UnshieldedOfferInfo<D>>,
	pub fallible_unshielded_offer: Option<UnshieldedOfferInfo<D>>,
	pub actions: Vec<Box<dyn BuildContractAction<D> + Send>>,
}

impl<D: DB + Clone> IntentInfo<D> {
	pub async fn build(
		&mut self,
		rng: &mut StdRng,
		context: Arc<LedgerContext<D>>,
		segment_id: SegmentId,
	) -> Intent<Signature, ProofPreimageMarker, PedersenRandomness, D> {
		let now = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.expect("Time went backwards")
			.as_secs();
		let delay: u64 = 3600; // Max lifetime

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
			.unwrap_or_else(|_| panic!("Intent signing with segment_id {:?} failed", segment_id))
	}
}
