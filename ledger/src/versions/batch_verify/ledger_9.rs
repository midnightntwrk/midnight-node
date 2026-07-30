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

//! Ledger-9 cross-transaction proof batching.
//!
//! This is the only ledger version that exposes the batch-verification primitives
//! (`Transaction::collect_proof_evidence` and `ProofKind::batch_proof_verify`). The shared
//! validation code in `common/mod.rs` delegates the ZK-crypto step of
//! `Bridge::batch_verify_transactions` here so that the v9-only types never leak into the
//! version-agnostic module (which must also compile against ledger 7 and 8).

#![cfg(feature = "std")]

use super::{
	LOG_TARGET,
	ledger_storage_local::db::DB,
	midnight_serialize_local::Serializable,
	mn_ledger_local::{
		structure::{ProofKind, ProofMarker, SignatureKind, Transaction},
		verify::{StateReference, WellFormedStrictness},
	},
	transient_crypto_local::commitment::PureGeneratorPedersen,
};

/// Collects the proof evidence of every transaction in `txs` and verifies all of it in a single
/// aggregate `batch_proof_verify` call against `ref_state`.
///
/// The transactions must already have passed the non-crypto `well_formed` checks (the caller runs
/// `well_formed` with proofs deferred first); this only performs the ZK-crypto step. Returns
/// `Ok(())` when every proof verifies and `Err(())` when evidence collection or the aggregate
/// verification fails (the caller then either isolates the offender or rejects the whole batch).
pub fn batch_verify_proofs<S, D>(
	txs: &[&Transaction<S, ProofMarker, PureGeneratorPedersen, D>],
	ref_state: &impl StateReference<D>,
) -> Result<(), ()>
where
	S: SignatureKind<D>,
	D: DB,
	Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Serializable,
{
	// `defer_proofs()` leaves `proof_verification_mode` at its default (`Real`); reuse it so the
	// batch step verifies proofs exactly as an inline `well_formed` would.
	let mode = WellFormedStrictness::default().proof_verification_mode;

	let mut all_evidence = Vec::new();
	for tx in txs {
		match tx.collect_proof_evidence(ref_state) {
			Ok(evidence) => all_evidence.extend(evidence),
			Err(e) => {
				log::warn!(
					target: LOG_TARGET,
					"batch proof verification: failed to collect proof evidence: {e}",
				);
				return Err(());
			},
		}
	}

	// `linear_revalidation: false` — we don't use the ledger's own failed-index localization;
	// on aggregate failure the caller (`batch_verify_transactions`) isolates the offender(s)
	// itself by re-verifying individually (see `common/mod.rs`'s fallback path).
	match <ProofMarker as ProofKind<D>>::batch_proof_verify(&all_evidence, mode, false) {
		Ok(()) => Ok(()),
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"batch proof verification failed for {} transaction(s): {e}",
				txs.len(),
			);
			Err(())
		},
	}
}
