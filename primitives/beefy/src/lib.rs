#![cfg_attr(not(feature = "std"), no_std)]
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

extern crate alloc;

use alloc::vec::Vec;
use parity_scale_codec::{Codec, Decode};
use sp_consensus_beefy::mmr::{BeefyAuthoritySet, BeefyNextAuthoritySet};
use sp_runtime::RuntimeAppPublic;

/// The key type for inserting Beefy keys into the keystore
pub const BEEFY_KEY_TYPE: &str = "beef";

pub const BEEFY_LOG_TARGET: &str = "midnight-beefy";

/// The StakeDelegation
pub type Stake = u64;
pub type BeefyAuthoritySetOf<Hash> = BeefyAuthoritySet<Hash>;

pub type BeefyStake<AuthorityId> = (AuthorityId, Stake);

/// A List of tuple (Beefy Ids, stake)
pub type BeefyStakes<AuthorityId> = Vec<BeefyStake<AuthorityId>>;

/// Ids to identify Beefy stakes
pub mod known_payloads {
	use sp_consensus_beefy::BeefyPayloadId;

	pub const CURRENT_BEEFY_STAKES_ID: BeefyPayloadId = *b"cs";
	pub const CURRENT_BEEFY_AUTHORITY_SET: BeefyPayloadId = *b"cb";
	pub const NEXT_BEEFY_STAKES_ID: BeefyPayloadId = *b"ns";
	pub const NEXT_BEEFY_AUTHORITY_SET: BeefyPayloadId = *b"nb";
}

// An api to be used and accessed by the Node
sp_api::decl_runtime_apis! {
	pub trait BeefyStakesApi<Hash, AuthorityId>
	where
		BeefyAuthoritySet<Hash>: Decode,
		AuthorityId: Codec + RuntimeAppPublic
	{
		/// Gets the current beefy stakes
		fn current_beefy_stakes() -> BeefyStakes<AuthorityId>;

		/// Gets the next beefy stakes
		fn next_beefy_stakes() -> Option<BeefyStakes<AuthorityId>>;

		/// Returns the authority set based on the current beef stakes
		fn compute_current_authority_set(
			beefy_stakes: BeefyStakes<AuthorityId>,
		) ->  BeefyAuthoritySet<Hash>;

		/// Returns the authority set based on the next beef stakes
		fn compute_next_authority_set(
			beefy_stakes: BeefyStakes<AuthorityId>,
		) -> BeefyNextAuthoritySet<Hash> ;
	}
}
