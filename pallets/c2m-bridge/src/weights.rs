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

//! Weight definitions for `pallet_c2m_bridge`.

#![cfg_attr(rustfmt, rustfmt_skip)]
#![allow(unused_parens)]
#![allow(unused_imports)]
#![allow(missing_docs)]

use frame_support::{traits::Get, weights::{Weight, constants::ParityDbWeight}};
use core::marker::PhantomData;

/// Weight functions needed for `pallet_c2m_bridge`.
pub trait WeightInfo {
	fn set_subminimal_transfers_config() -> Weight;
	fn add_approved_mc_tx_hashes(n: u32) -> Weight;
}

#[cfg(test)]
pub struct SubstrateWeight<T>(PhantomData<T>);

#[cfg(test)]
impl WeightInfo for () {
	fn set_subminimal_transfers_config() -> Weight {
	    Weight::zero()
	}
	fn add_approved_mc_tx_hashes(_n: u32) -> Weight {
	    Weight::zero()
	}
}
