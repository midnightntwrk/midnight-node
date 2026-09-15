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

//! Shared batch-verification failure type.
//!
//! Lives here rather than in the per-version `versions/batch_verify/ledger_*.rs` modules (which are
//! module-parameterized, one instantiation per ledger version) because it carries no
//! version-dependent types — just transaction indices.

#![cfg(feature = "std")]

use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};

/// Whether the mempool batch-verification ingress point is enabled on this node.
///
/// Set once during node startup via [`set_batch_verify_enabled`]; defaults to `false` so any
/// embedder that never calls it (tests, the toolkit, node subcommands) stays quiet.
static BATCH_VERIFY_MEMPOOL_ENABLED: AtomicBool = AtomicBool::new(false);

/// Whether the block-import batch-verification ingress point is enabled on this node. See
/// [`BATCH_VERIFY_MEMPOOL_ENABLED`].
static BATCH_VERIFY_BLOCK_IMPORT_ENABLED: AtomicBool = AtomicBool::new(false);

/// Records which batch-verification ingress points are enabled for this process. Called once from
/// node startup.
pub fn set_batch_verify_enabled(mempool: bool, block_import: bool) {
	BATCH_VERIFY_MEMPOOL_ENABLED.store(mempool, Ordering::Relaxed);
	BATCH_VERIFY_BLOCK_IMPORT_ENABLED.store(block_import, Ordering::Relaxed);
}

/// Whether a transaction reaching **mempool** validation should already carry a batch-verified
/// proof result. Only the mempool ingress point warms the cache before that happens.
pub fn batch_verify_mempool_enabled() -> bool {
	BATCH_VERIFY_MEMPOOL_ENABLED.load(Ordering::Relaxed)
}

/// Whether a transaction reaching **block execution** (`pre_dispatch`, then dispatch) should
/// already carry a batch-verified proof result. Either ingress point can have warmed it: the
/// mempool worker pool on the node that authored the block, or the block-import wrapper on a node
/// importing someone else's.
pub fn batch_verify_block_enabled() -> bool {
	BATCH_VERIFY_MEMPOOL_ENABLED.load(Ordering::Relaxed)
		|| BATCH_VERIFY_BLOCK_IMPORT_ENABLED.load(Ordering::Relaxed)
}

/// Why an aggregate batch proof verification failed.
///
/// The ledger's `ProofKind::batch_proof_verify` takes a `linear_revalidation` flag: when set, a
/// rejected batch is searched for the offending proofs (one cheap pairing per proof, reusing the
/// already-prepared guards) and their indices are reported; when clear, the batch is rejected as a
/// unit without spending that effort. These are the two outcomes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BatchVerifyFailure {
	/// The ledger localized the invalid proofs: ascending, deduplicated indices into the
	/// transaction slice passed to `batch_verify_proofs`. Every transaction *not* listed verified
	/// as part of the same aggregate check, so the caller can reject exactly the offenders and keep
	/// the rest of the batch.
	Localized(Vec<usize>),
	/// The failure could not be attributed to individual transactions: `linear_revalidation` was
	/// `false`, proof-evidence collection failed, or the rejection came from a path the ledger does
	/// not localize (the legacy v2 proof batch, verifier-key initialization). Nothing may be
	/// concluded about any individual transaction in the batch.
	Unlocalized,
}

#[cfg(test)]
mod tests {
	use super::{
		batch_verify_block_enabled, batch_verify_mempool_enabled, set_batch_verify_enabled,
	};

	/// Both flags must default to `false` so an embedder that never sets them (tests, the toolkit,
	/// node subcommands) does not get the batching-on error logging.
	///
	/// The block-execution flag is deliberately the OR of the two ingress points, while the
	/// mempool flag tracks only its own: with block-import batching on but the mempool path off,
	/// a transaction entering the pool has legitimately not been batch-verified, and treating that
	/// as an error would log once per transaction in the configuration the presets recommend.
	#[test]
	fn batch_verify_flags_default_off_and_track_their_own_ingress() {
		assert!(!batch_verify_mempool_enabled(), "mempool default must be off");
		assert!(!batch_verify_block_enabled(), "block default must be off");

		set_batch_verify_enabled(false, true);
		assert!(!batch_verify_mempool_enabled(), "block-import alone must not arm the mempool");
		assert!(batch_verify_block_enabled(), "block-import alone arms block execution");

		set_batch_verify_enabled(true, false);
		assert!(batch_verify_mempool_enabled());
		assert!(
			batch_verify_block_enabled(),
			"the mempool warms the cache for block execution too"
		);

		// Restore: the flags are process-global, and leaving them set would make every later
		// `get_verified_transaction` miss in this process log at ERROR.
		set_batch_verify_enabled(false, false);
		assert!(!batch_verify_mempool_enabled());
		assert!(!batch_verify_block_enabled());
	}
}
