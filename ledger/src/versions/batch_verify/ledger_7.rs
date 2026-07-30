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

//! Ledger-7 batch-verification stub.
//!
//! Ledger 7 predates the cross-transaction proof-batching primitives
//! (`collect_proof_evidence` / `batch_proof_verify`), so it cannot batch. The node only ever
//! dispatches the batch entry point to the active (ledger-9) version, so this is never reached at
//! runtime; it exists solely to keep the version-agnostic `Bridge::batch_verify_transactions`
//! compiling for every ledger version. Returning an (unlocalized) error forces the caller to fall
//! back to per-transaction inline verification rather than silently trusting unverified proofs.

#![cfg(feature = "std")]

use super::{
	ledger_storage_local::db::DB,
	midnight_serialize_local::Serializable,
	mn_ledger_local::{
		structure::{ProofMarker, SignatureKind, Transaction},
		verify::StateReference,
	},
	transient_crypto_local::commitment::PureGeneratorPedersen,
};
use crate::common::batch::BatchVerifyFailure;

/// Ledger 7 cannot batch-verify proofs; always signals "unsupported" so the caller falls back.
pub fn batch_verify_proofs<S, D>(
	_txs: &[&Transaction<S, ProofMarker, PureGeneratorPedersen, D>],
	_ref_state: &impl StateReference<D>,
	_linear_revalidation: bool,
) -> Result<(), BatchVerifyFailure>
where
	S: SignatureKind<D>,
	D: DB,
	Transaction<S, ProofMarker, PureGeneratorPedersen, D>: Serializable,
{
	Err(BatchVerifyFailure::Unlocalized)
}
