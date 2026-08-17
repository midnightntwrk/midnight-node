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
