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

//! Shared consensus-engine primitives and the runtime API exposing which
//! block-production engine is currently active.

#![cfg_attr(not(feature = "std"), no_std)]

use parity_scale_codec::{Decode, Encode, MaxEncodedLen};
use scale_info::TypeInfo;

/// `spec_version` of the first runtime that contains `pallet-consensus-engine`.
///
/// From the block executed by that runtime onward the pallet enforces the layout of the AURA and
/// BABE pre-runtime digests, so a node may interpret them. Blocks executed by an older runtime
/// (their `pallet-version` digest reports a lower `spec_version`) were never checked, so a node
/// must treat them as plain AURA blocks whatever their header carries. `pallet-version` has
/// deposited its digest in every block since genesis, so a block *without* one can only come from
/// a future runtime that dropped the pallet, well after activation, and is trusted.
pub const ACTIVATION_SPEC_VERSION: u32 = 3_000_000;

/// The block-production consensus engine currently active on the chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, MaxEncodedLen, TypeInfo)]
pub enum ActiveEngine {
	/// AURA block production is active.
	Aura,
	/// BABE block production is active.
	Babe,
}

sp_api::decl_runtime_apis! {
	/// Runtime API reporting the consensus engine currently producing blocks.
	pub trait ConsensusEngineApi {
		/// Returns the consensus engine that is currently active.
		fn active_engine() -> ActiveEngine;
	}
}
