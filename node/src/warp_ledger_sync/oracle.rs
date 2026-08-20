// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Recovery gate + wrapping [`SyncOracle`].
//!
//! The shared [`RecoveryGate`] is the single source of truth for "is the warp-recovered ledger
//! arena ready yet". It gates two things until recovery is verified:
//! - **block authoring**, via [`MidnightSyncOracle`] passed to both AURA and BABE, and
//! - **block import**, via [`super::block_import::GatedBlockImport`] on both the AURA and BABE
//!   import queues — so the node does not execute post-warp blocks against an empty arena (which
//!   would hit `NoLedgerState`) or race the recovery writer. After the consensus flip, incoming
//!   blocks are dispatched to the BABE queue, so gating AURA alone is not enough.
//!
//! The gate is armed **only** when the monitor detects the warp path. On a full sync it stays
//! disarmed, so both gates are pure passthroughs and full-sync nodes are never affected.

use std::sync::{
	Arc,
	atomic::{AtomicBool, Ordering},
};

use sp_consensus::SyncOracle;

/// Shared recovery flags, flipped by the monitor task and read by the authoring oracle and the
/// block-import gate. Cloneable `Arc` handle so all observe the same state.
#[derive(Debug, Default)]
pub struct RecoveryGate {
	/// Set true once the warp path is detected; while false the gate is a pure passthrough.
	recovery_pending: AtomicBool,
	/// Set true once the ledger arena is recovered and verified.
	ledger_verified: AtomicBool,
}

impl RecoveryGate {
	/// A fresh gate: nothing pending (full-sync default).
	pub fn new() -> Arc<Self> {
		Arc::new(Self::default())
	}

	/// Arm the gate: the warp path was taken, so authoring + import must wait for verification.
	pub fn arm(&self) {
		self.recovery_pending.store(true, Ordering::Release);
	}

	/// Mark the ledger arena verified + imported. Opens both gates.
	pub fn mark_ledger_verified(&self) {
		self.ledger_verified.store(true, Ordering::Release);
	}

	/// Whether warp recovery is in progress: armed but not yet verified. Both the authoring oracle
	/// and the import gate hold while this is true.
	pub fn ledger_recovery_in_progress(&self) -> bool {
		self.recovery_pending.load(Ordering::Acquire)
			&& !self.ledger_verified.load(Ordering::Acquire)
	}

	/// Wait until recovery is no longer in progress (poll-based; recovery takes seconds-to-minutes,
	/// so a sub-second poll adds no meaningful latency and keeps the gate free of async machinery).
	pub async fn wait_until_released(&self) {
		while self.ledger_recovery_in_progress() {
			tokio::time::sleep(std::time::Duration::from_millis(250)).await;
		}
	}
}

/// Wraps the node's inner [`SyncOracle`] (the `SyncingService`) so AURA reports "still syncing"
/// (and therefore does not author) while the warp-recovered ledger arena is being recovered.
#[derive(Clone)]
pub struct MidnightSyncOracle<Inner> {
	inner: Inner,
	gate: Arc<RecoveryGate>,
}

impl<Inner> MidnightSyncOracle<Inner> {
	pub fn new(inner: Inner, gate: Arc<RecoveryGate>) -> Self {
		Self { inner, gate }
	}
}

impl<Inner: SyncOracle> SyncOracle for MidnightSyncOracle<Inner> {
	fn is_major_syncing(&self) -> bool {
		self.inner.is_major_syncing() || self.gate.ledger_recovery_in_progress()
	}

	fn is_offline(&self) -> bool {
		self.inner.is_offline()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Minimal inner oracle whose state we control.
	#[derive(Clone)]
	struct MockOracle(Arc<AtomicBool>);
	impl MockOracle {
		fn new(major: bool) -> Self {
			Self(Arc::new(AtomicBool::new(major)))
		}
		fn set(&self, v: bool) {
			self.0.store(v, Ordering::Release);
		}
	}
	impl SyncOracle for MockOracle {
		fn is_major_syncing(&self) -> bool {
			self.0.load(Ordering::Acquire)
		}
		fn is_offline(&self) -> bool {
			false
		}
	}

	#[test]
	fn full_sync_is_pure_passthrough() {
		// Gate never armed (recovery_pending == false): behavior == inner.
		let inner = MockOracle::new(true);
		let oracle = MidnightSyncOracle::new(inner.clone(), RecoveryGate::new());
		assert!(oracle.is_major_syncing(), "delegates to inner while inner is syncing");
		inner.set(false);
		assert!(!oracle.is_major_syncing(), "not gated on full sync once inner is done");
	}

	#[test]
	fn warp_node_gated_until_ledger_verified() {
		let inner = MockOracle::new(false); // inner already finished warp+state-sync
		let gate = RecoveryGate::new();
		let oracle = MidnightSyncOracle::new(inner, gate.clone());

		gate.arm();
		assert!(oracle.is_major_syncing(), "armed + not verified -> gated");
		assert!(gate.ledger_recovery_in_progress());

		gate.mark_ledger_verified();
		assert!(!oracle.is_major_syncing(), "verified -> released");
		assert!(!gate.ledger_recovery_in_progress());
	}

	#[test]
	fn is_offline_always_delegates() {
		let oracle = MidnightSyncOracle::new(MockOracle::new(true), RecoveryGate::new());
		assert!(!oracle.is_offline());
	}
}
