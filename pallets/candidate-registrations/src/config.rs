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

//! Chain-spec shape for the pallet's genesis.

use alloc::string::String;
use midnight_primitives_candidate_registrations::{
	CardanoTxPosition, DEFAULT_REGISTRATION_EVENTS_PER_BLOCK,
};
use serde::{Deserialize, Serialize};

/// Where on Cardano registrations live, and where observation starts.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[cfg_attr(feature = "std", derive(serde_valid::Validate))]
pub struct CandidateRegistrationsGenesis {
	/// Bech32 address of the candidates validator.
	#[cfg_attr(feature = "std", validate(pattern = r"^(addr|addr_test)1[0-9a-z]{1,108}$"))]
	pub validator_address: String,

	/// Policy id of the Registration Token. An output at `validator_address`
	/// without this token is not a registration, and is filtered out before
	/// anything else looks at it.
	#[serde(with = "hex")]
	pub registration_policy_id: [u8; 28],

	/// First Cardano transaction to observe.
	pub next_cardano_position: CardanoTxPosition,

	/// Registration events the runtime accepts per block.
	pub registration_events_per_block: u32,
}

impl Default for CandidateRegistrationsGenesis {
	fn default() -> Self {
		Self {
			validator_address: String::new(),
			registration_policy_id: [0u8; 28],
			next_cardano_position: CardanoTxPosition::default(),
			registration_events_per_block: DEFAULT_REGISTRATION_EVENTS_PER_BLOCK,
		}
	}
}
