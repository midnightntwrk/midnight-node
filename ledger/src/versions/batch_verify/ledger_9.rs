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
		error::MalformedTransaction,
		structure::{ProofKind, ProofMarker, SignatureKind, Transaction},
		verify::{StateReference, WellFormedStrictness},
	},
	transient_crypto_local::commitment::PureGeneratorPedersen,
};
use crate::common::batch::BatchVerifyFailure;

/// Proof evidence for a set of transactions with the expensive, per-proof half of batch
/// verification already done. Accumulated with [`merge_prepared`], decided by [`finalize_prepared`].
pub type PreparedProofs = super::mn_ledger_local::structure::PreparedContractProofs;

/// Collects one transaction's proof evidence and runs the per-proof half of batch verification
/// over it, returning the prepared evidence and how many evidence items it contributed.
///
/// This is the part of [`batch_verify_proofs`] whose cost grows with the number of proofs, and it
/// does not depend on which other transactions end up in the batch — so a caller may run it as
/// each transaction arrives and only pay [`finalize_prepared`] when it decides the batch.
///
/// The transaction must already have passed the non-crypto `well_formed` checks (run `well_formed`
/// with proofs deferred first); this only does proof work.
pub fn prepare_tx_proofs<S, D>(
	tx: &Transaction<S, ProofMarker, PureGeneratorPedersen, D>,
	ref_state: &impl StateReference<D>,
) -> Result<(PreparedProofs, usize), BatchVerifyFailure>
where
	S: SignatureKind<D>,
	D: DB,
	Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Serializable,
{
	let evidence = tx.collect_proof_evidence(ref_state).map_err(|e| {
		log::warn!(
			target: LOG_TARGET,
			"batch proof preparation: failed to collect proof evidence: {e}",
		);
		BatchVerifyFailure::Unlocalized
	})?;
	let count = evidence.len();
	let mode = WellFormedStrictness::default().proof_verification_mode;
	let prepared = <ProofMarker as ProofKind<D>>::prepare_proof_evidence(&evidence, mode).map_err(
		|e| {
			log::warn!(target: LOG_TARGET, "batch proof preparation failed: {e}");
			BatchVerifyFailure::Unlocalized
		},
	)?;
	Ok((prepared, count))
}

/// Appends `more` to `acc`, keeping evidence order so indices reported by [`finalize_prepared`]
/// stay relative to the whole accumulated batch.
pub fn merge_prepared<D: DB>(acc: &mut PreparedProofs, more: PreparedProofs) {
	<ProofMarker as ProofKind<D>>::merge_prepared_evidence(acc, more)
}

/// Decides a batch accumulated by [`prepare_tx_proofs`] / [`merge_prepared`]: one fold and a single
/// pairing check, at a cost essentially independent of the batch size.
///
/// `linear_revalidation` behaves exactly as in [`batch_verify_proofs`], and the indices reported in
/// [`BatchVerifyFailure::Localized`] are positions in the accumulated *evidence* sequence — the
/// caller maps them back to transactions with its own prefix-sum table.
pub fn finalize_prepared<D: DB>(
	prepared: &PreparedProofs,
	linear_revalidation: bool,
) -> Result<(), BatchVerifyFailure> {
	let mode = WellFormedStrictness::default().proof_verification_mode;
	match <ProofMarker as ProofKind<D>>::verify_prepared_evidence(
		prepared,
		mode,
		linear_revalidation,
	) {
		Ok(()) => Ok(()),
		Err(MalformedTransaction::InvalidProofBatch { failed_indices }) => {
			log::warn!(
				target: LOG_TARGET,
				"prepared batch verification failed at evidence index(es) {failed_indices:?}",
			);
			Err(BatchVerifyFailure::LocalizedEvidence(failed_indices))
		},
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"prepared batch verification failed, not localized: {e}",
			);
			Err(BatchVerifyFailure::Unlocalized)
		},
	}
}

/// Collects the proof evidence of every transaction in `txs` and verifies all of it in a single
/// aggregate `batch_proof_verify` call against `ref_state`.
///
/// The transactions must already have passed the non-crypto `well_formed` checks (the caller runs
/// `well_formed` with proofs deferred first); this only performs the ZK-crypto step.
///
/// `linear_revalidation` is passed straight through to the ledger: with it set, a rejected batch is
/// searched for the offending proofs and this returns [`BatchVerifyFailure::Localized`] naming the
/// transactions that carry them (every other transaction in the batch verified); with it clear, the
/// ledger spends no effort on attribution and the failure is [`BatchVerifyFailure::Unlocalized`].
/// Callers that only need an accept/reject verdict for the whole batch (block import) should pass
/// `false`.
pub fn batch_verify_proofs<S, D>(
	txs: &[&Transaction<S, ProofMarker, PureGeneratorPedersen, D>],
	ref_state: &impl StateReference<D>,
	linear_revalidation: bool,
) -> Result<(), BatchVerifyFailure>
where
	S: SignatureKind<D>,
	D: DB,
	Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Serializable,
{
	// `defer_proofs()` leaves `proof_verification_mode` at its default (`Real`); reuse it so the
	// batch step verifies proofs exactly as an inline `well_formed` would.
	let mode = WellFormedStrictness::default().proof_verification_mode;

	// `evidence_ends[i]` is the total evidence count contributed by `txs[..=i]`, i.e. the
	// per-transaction prefix sum. Non-decreasing, with equal neighbours for proofless transactions
	// (`ClaimRewards`). It's what maps the ledger's evidence-space failure indices back to
	// transactions below.
	let mut evidence_ends = Vec::with_capacity(txs.len());
	let mut all_evidence = Vec::new();
	for tx in txs {
		match tx.collect_proof_evidence(ref_state) {
			Ok(evidence) => all_evidence.extend(evidence),
			Err(e) => {
				log::warn!(
					target: LOG_TARGET,
					"batch proof verification: failed to collect proof evidence: {e}",
				);
				return Err(BatchVerifyFailure::Unlocalized);
			},
		}
		evidence_ends.push(all_evidence.len());
	}

	match <ProofMarker as ProofKind<D>>::batch_proof_verify(
		&all_evidence,
		mode,
		linear_revalidation,
	) {
		Ok(()) => Ok(()),
		// The ledger pinpointed the bad proofs (only possible with `linear_revalidation`): translate
		// its evidence-space indices into transaction-space ones.
		Err(MalformedTransaction::InvalidProofBatch { failed_indices }) => {
			let tx_indices = evidence_to_tx_indices(&evidence_ends, &failed_indices);
			if tx_indices.is_empty() {
				// Defensive: a localized failure we can't attribute (indices outside our evidence
				// table). Report it as unlocalized — an empty `Localized` would read as "no
				// offender", letting the caller accept the batch the ledger just rejected.
				log::warn!(
					target: LOG_TARGET,
					"batch proof verification failed for {} transaction(s); could not attribute \
					 evidence index(es) {failed_indices:?}",
					txs.len(),
				);
				return Err(BatchVerifyFailure::Unlocalized);
			}
			log::warn!(
				target: LOG_TARGET,
				"batch proof verification failed for {} of {} transaction(s), at batch index(es) {tx_indices:?}",
				tx_indices.len(),
				txs.len(),
			);
			Err(BatchVerifyFailure::Localized(tx_indices))
		},
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"batch proof verification failed for {} transaction(s), not localized: {e}",
				txs.len(),
			);
			Err(BatchVerifyFailure::Unlocalized)
		},
	}
}

/// Maps the ledger's proof-evidence failure indices to the indices of the transactions that
/// contributed them (ascending, deduplicated — several bad proofs can belong to one transaction).
///
/// `evidence_ends` is the per-transaction prefix sum of evidence counts, so the owner of evidence
/// index `e` is the first transaction whose end is strictly greater than `e`. Indices past the end
/// of the evidence are dropped rather than blaming a nonexistent transaction; the caller treats an
/// empty result as an unlocalized failure.
pub fn evidence_to_tx_indices(evidence_ends: &[usize], failed_evidence: &[usize]) -> Vec<usize> {
	let mut tx_indices: Vec<usize> = failed_evidence
		.iter()
		.map(|&e| evidence_ends.partition_point(|&end| end <= e))
		.filter(|&i| i < evidence_ends.len())
		.collect();
	tx_indices.sort_unstable();
	tx_indices.dedup();
	tx_indices
}

#[cfg(test)]
mod tests {
	use super::evidence_to_tx_indices;

	#[test]
	fn maps_evidence_indices_to_transaction_indices() {
		// Three transactions contributing 2, 0 (proofless) and 3 evidence items respectively.
		let ends = [2, 2, 5];

		assert_eq!(evidence_to_tx_indices(&ends, &[0]), vec![0]);
		assert_eq!(evidence_to_tx_indices(&ends, &[1]), vec![0]);
		assert_eq!(evidence_to_tx_indices(&ends, &[2]), vec![2]);
		assert_eq!(evidence_to_tx_indices(&ends, &[4]), vec![2]);

		// Several bad proofs in the same transaction collapse to one index; the proofless
		// transaction 1 owns no evidence and can never be blamed.
		assert_eq!(evidence_to_tx_indices(&ends, &[0, 1]), vec![0]);
		assert_eq!(evidence_to_tx_indices(&ends, &[2, 3, 4]), vec![2]);
		assert_eq!(evidence_to_tx_indices(&ends, &[1, 3]), vec![0, 2]);

		// Out-of-range indices are dropped, leaving an empty (⇒ unlocalized) result.
		assert_eq!(evidence_to_tx_indices(&ends, &[5]), Vec::<usize>::new());
		assert_eq!(evidence_to_tx_indices(&[], &[0]), Vec::<usize>::new());
	}
}
