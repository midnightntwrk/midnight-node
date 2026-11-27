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

//! Ledger 9's side of the [`TxValidationCache`](ledger::TxValidationCache) property tests: the
//! proofs and the parts of the ledger's interface that differ between versions. The tests
//! themselves are shared: the properties and every test of them are in
//! `src/revalidation_suite/properties.rs`.

mod proofs;

/// Declares a test `NAME::run` for each suite function `NAME`.
macro_rules! suite_tests {
	($($name:ident),* $(,)?) => {$(
		mod $name {
			#[test]
			fn run() {
				super::$name()
			}
		}
	)*};
}

// Each ledger version compiles it once.
#[allow(clippy::duplicate_mod)]
#[path = "../../revalidation_suite/mod.rs"]
mod suite;

pub use crate::ledger_9 as ledger;
pub use midnight_ledger_unsafe_helpers::ledger_9 as helpers;

use crate::boundary::types::{Hash, TransactionAppliedStateRoot, TransactionDetails};
use helpers::{LedgerParameters, Timestamp};
use ledger::{
	BlockContext, Bridge, TransactionSignature, TxValidationCache,
	ledger_storage_local::{DefaultDB, db::DB},
	midnight_serialize_local::Tagged,
	mn_ledger_local::structure::{LedgerState, ProofKind, Transaction},
	transient_crypto_local::commitment::PureGeneratorPedersen,
	types::LedgerApiError,
	zswap_local::ledger::State as ZswapState,
};
use sp_externalities::Externalities;
use storage_sha2::digest::{OutputSizeUser, consts::U32};

/// Whether a call `well_formed` rejects at the corrected tblock is retried at the block's own.
pub const RETRY_AT_BLOCK_TBLOCK: bool = false;

/// Below the size of an entry point with a verifier key, so the deploy and key inserts fail.
pub const SHRINK_CONTRACT_METADATA_LIMIT: Option<fn(&mut LedgerParameters)> =
	Some(|params| params.limits.max_contract_metadata_size = 1_000);

/// The zswap state after a block end, which records its Merkle root.
pub fn zswap_post_block_update(
	state: &LedgerState<DefaultDB>,
	tblock: Timestamp,
) -> ZswapState<DefaultDB> {
	state.zswap.post_block_update(tblock, state.parameters.global_ttl)
}

#[allow(clippy::too_many_arguments)]
pub fn apply_transaction<D: DB, P: ProofKind<D>>(
	ext: &mut dyn Externalities,
	state_key: &[u8],
	tx: &[u8],
	block_context: BlockContext,
	filter_only_failed_segments: bool,
	net_intra_tx_utxos: bool,
	runtime_version: u32,
	skew_tblock: bool,
	cache: &TxValidationCache,
) -> Result<TransactionAppliedStateRoot, LedgerApiError>
where
	Transaction<TransactionSignature, P, PureGeneratorPedersen, D>: Tagged,
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	Bridge::<TransactionSignature, D, P>::apply_transaction(
		ext,
		state_key,
		tx,
		block_context,
		filter_only_failed_segments,
		net_intra_tx_utxos,
		runtime_version,
		skew_tblock,
		cache,
	)
}

#[allow(clippy::too_many_arguments)]
pub fn validate_transaction<D: DB, P: ProofKind<D>>(
	ext: &mut dyn Externalities,
	state_key: &[u8],
	tx: &[u8],
	block_context: BlockContext,
	runtime_version: u32,
	max_weight: u64,
	get_tx_details: bool,
	check_utxos: bool,
	cache: &TxValidationCache,
) -> Result<(Hash, Option<TransactionDetails>), LedgerApiError>
where
	Transaction<TransactionSignature, P, PureGeneratorPedersen, D>: Tagged,
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	Bridge::<TransactionSignature, D, P>::validate_transaction(
		ext,
		state_key,
		tx,
		block_context,
		runtime_version,
		max_weight,
		get_tx_details,
		check_utxos,
		cache,
	)
}

#[allow(clippy::too_many_arguments)]
pub fn validate_guaranteed_execution<D: DB, P: ProofKind<D>>(
	ext: &mut dyn Externalities,
	state_key: &[u8],
	tx: &[u8],
	block_context: BlockContext,
	runtime_version: u32,
	skew_tblock: bool,
	check_utxos: bool,
	cache: &TxValidationCache,
) -> Result<(), LedgerApiError>
where
	Transaction<TransactionSignature, P, PureGeneratorPedersen, D>: Tagged,
	D::Hasher: OutputSizeUser<OutputSize = U32>,
{
	Bridge::<TransactionSignature, D, P>::validate_guaranteed_execution(
		ext,
		state_key,
		tx,
		block_context,
		runtime_version,
		skew_tblock,
		check_utxos,
		cache,
	)
}
