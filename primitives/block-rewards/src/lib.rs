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

//! Native NIGHT block rewards: the parameters, the per-block draw, the record a block leaves and
//! the split of a pool's epoch total.
//!
//! Pure arithmetic only; storage and hooks live in the pallet. Every encoding here is consensus
//! visible: never reorder fields or enum variants.
//!
//! # Symbols
//!
//! Amounts are in STAR, stakes in lovelace. `B` and `B_o` are unrelated: a parameter and a
//! counter. Lower-case `s` is always a stake; upper-case `S` is the settled counter.
//!
//! ```text
//! Parameters (RewardParametersV1)
//!   π       initial_expansion
//!   B       reserve_allocation
//!   T       treasury_allocation
//!   γ       expected_blocks_per_year      blocks in YEAR_MS at the chain's slot duration
//!   R       distribution_rate             π (1 − B − T) / B / γ: share of B_o one block draws
//!
//! Reserve counters (pallet storage, and the Released source)
//!   B_o     ReserveOutstanding            reserve not drawn yet
//!   D       Drawn                         drawn by blocks
//!   S       Settled                       paid to the ledger, or held in escrow
//!   L       Config::Released              released by the Cardano Reserve
//!
//! One block's draw (BlockDraw)
//!   N_b     base                          floor(B_o × R)
//!   N_f                                   floor(N_b × fixed_subsidy)
//!   N_v                                   floor((N_b − N_f) × used / max)
//!   N_a     producer                      N_f + N_v
//!   N_t     treasury                      N_b − N_a
//!
//! One pool's split for an epoch (PoolSplit)
//!   P_pool      pool_total                what the pool's blocks drew for producers
//!   s           total_stake               s_operator + Σ s_i
//!   s_operator  SnapshotPool::operator_stake
//!   s_i         SnapshotPayee::stake      one delegator or lock
//!   share_i                               floor(shared × s_i / s)
//! ```
//!
//! The pallet keeps `B_o + D` equal to the reserve at activation, `S ≤ D` and `S ≤ L`.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

mod draw;
mod parameters;
mod record;
mod snapshot;
mod split;
mod usage;

pub use draw::{BlockDraw, DrawError, Utilization, block_draw};
pub use parameters::{
	MAX_UPDATES_PER_BLOCK, ParameterError, Ratio, RewardParameters, RewardParametersV1, YEAR_MS,
};
pub use record::{BlockRewardRecord, CardanoEpoch};
pub use snapshot::{
	LockOwner, RewardAddress, RewardPayee, SnapshotPayee, SnapshotPool, StakeCredential, UtxoId,
};
pub use split::{PoolSplit, PoolTerms, SplitError, split_pool};
pub use usage::{BlockUsage, UsageDimension};
