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

//! Hand-written estimates from the storage each call touches, until the pallet is benchmarked.

use core::marker::PhantomData;
use frame_support::{
	traits::Get,
	weights::{RuntimeDbWeight, Weight, constants::RocksDbWeight},
};

pub trait WeightInfo {
	fn activate() -> Weight;
	fn clear_halt() -> Weight;
	fn accrue() -> Weight;
	fn settle(updates: u32) -> Weight;
}

const BASE: Weight = Weight::from_parts(20_000_000, 0);

/// Priced with the runtime's own database weights.
pub struct SubstrateWeight<T>(PhantomData<T>);

impl<T: frame_system::Config> WeightInfo for SubstrateWeight<T> {
	fn activate() -> Weight {
		activate(T::DbWeight::get())
	}
	fn clear_halt() -> Weight {
		clear_halt(T::DbWeight::get())
	}
	fn accrue() -> Weight {
		accrue(T::DbWeight::get())
	}
	fn settle(updates: u32) -> Weight {
		settle(T::DbWeight::get(), updates)
	}
}

impl WeightInfo for () {
	fn activate() -> Weight {
		activate(RocksDbWeight::get())
	}
	fn clear_halt() -> Weight {
		clear_halt(RocksDbWeight::get())
	}
	fn accrue() -> Weight {
		accrue(RocksDbWeight::get())
	}
	fn settle(updates: u32) -> Weight {
		settle(RocksDbWeight::get(), updates)
	}
}

/// Reads: `Activation`, `CurrentParametersVersion`, `ParameterSets`, the ledger's two pools and the
/// two prerequisites. Writes: `Activation`, `ReserveOutstanding`, `Drawn`, `Settled`, and the
/// block's `on_finalize` takes of `BlockUtilization` and `SettlementUsage`.
fn activate(db: RuntimeDbWeight) -> Weight {
	BASE.saturating_add(db.reads_writes(7, 6))
		.saturating_add(Weight::from_parts(0, 1_024))
}

fn clear_halt(db: RuntimeDbWeight) -> Weight {
	BASE.saturating_add(db.reads_writes(1, 1))
}

/// Reads: `Activation`, the timestamp, the digest, the current committee, the snapshot's id,
/// `OpenEpoch`, `ParameterSets`, `ReserveOutstanding`, `Drawn`, the bucket, and `Settled` and `L`
/// for the counters' check. Writes: `OpenEpoch`, `ReserveOutstanding`, `Drawn`, the bucket and the
/// two utilization takes; when an epoch closes, also the snapshot's id read again, `QueuedEpochs`
/// read, and `ClosedEpochs`, `NextClosed`, `LastClosed`, `QueuedEpochs` and `Settlement` written. The proof size covers
/// decoding a committee of up to 1 000 seats.
fn accrue(db: RuntimeDbWeight) -> Weight {
	BASE.saturating_add(db.reads_writes(14, 11))
		.saturating_add(Weight::from_parts(0, 200_000))
}

/// Fixed reads: `ParameterSets`, `SettlementHalted`, `Settlement`, `Settled`, `Activation`,
/// `EscrowTotal`, the ledger's pool, `ClosedEpochs`, `NextClosed`, `SettlementUsage`,
/// `SettlementBudget`, `QueuedEpochs`; fixed writes: `SettlementBudget`, `QueuedEpochs`. Per update,
/// at most: the cursor, the closed epoch and the snapshot's id, pool and a row or a payout, the
/// bucket, and `Payouts`, `Escrow`, `EscrowTotal` and the cursor written. `updates` is the work done, not the limit. The
/// ledger's own work is added by the caller.
fn settle(db: RuntimeDbWeight, updates: u32) -> Weight {
	BASE.saturating_add(db.reads_writes(12, 6))
		.saturating_add(db.reads_writes(6, 4).saturating_mul(updates.into()))
		.saturating_add(Weight::from_parts(0, 1_024).saturating_mul(updates.into()))
}
