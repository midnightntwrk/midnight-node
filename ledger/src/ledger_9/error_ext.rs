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

//! Ledger-9-only mappings for variants that don't exist in ledgers 7 and 8. Each
//! helper handles only the variants we know are ledger-9-specific and returns
//! `Err` for anything else, so the shared common conversion can fall back to
//! its `UnknownError + log` arm rather than misclassifying future additions.

#![cfg(feature = "std")]

use crate::ledger_9::{
	ledger_storage_local::db::DB,
	mn_ledger_local::error::{
		FeeCalculationError, MalformedContractDeploy, MalformedTransaction,
		SystemTransactionError as LedgerSystemTransactionError, TransactionInvalid,
	},
	types::{
		FeeCalculationErrorCode, InvalidError, MalformedContractDeployErrorCode, MalformedError,
		SystemTransactionError, ZswapInvalidErrorCode,
	},
	zswap_local::error::TransactionInvalid as ZswapTransactionInvalid,
};

pub fn try_convert_extra_invalid<D: DB>(
	err: TransactionInvalid<D>,
) -> Result<InvalidError, TransactionInvalid<D>> {
	match err {
		TransactionInvalid::MerkleTreeError(_) => Ok(InvalidError::MerkleTreeError),
		TransactionInvalid::InitialNonceAlreadyPresent(_) => {
			Ok(InvalidError::GenerationInfoAlreadyPresent)
		},
		TransactionInvalid::BridgeClaimTooSmall { .. } => Ok(InvalidError::BridgeClaimTooSmall),
		TransactionInvalid::ContractTooManyOperations { .. } => {
			Ok(InvalidError::ContractTooManyOperations)
		},
		other => Err(other),
	}
}

/// `MalformedTransaction` variants added in ledger 9.1.0.1.
pub fn try_convert_extra_malformed<D: DB>(
	err: MalformedTransaction<D>,
) -> Result<MalformedError, MalformedTransaction<D>> {
	match err {
		MalformedTransaction::TooManyPublicInputs { .. } => Ok(MalformedError::TooManyPublicInputs),
		MalformedTransaction::TranscriptGasExceedsBlockLimits { .. } => {
			Ok(MalformedError::TranscriptGasExceedsBlockLimits)
		},
		MalformedTransaction::ContractDustTransfer { .. } => {
			Ok(MalformedError::ContractDustTransfer)
		},
		MalformedTransaction::RepeatedCommitteeSignature { .. } => {
			Ok(MalformedError::RepeatedCommitteeSignature)
		},
		MalformedTransaction::RepeatedDustRegistration { .. } => {
			Ok(MalformedError::RepeatedDustRegistration)
		},
		other => Err(other),
	}
}

/// `FeeCalculationError` is `#[non_exhaustive]` from ledger 9.1.0.1.
pub fn try_convert_extra_fee_calculation(
	err: FeeCalculationError,
) -> Result<FeeCalculationErrorCode, FeeCalculationError> {
	match err {
		FeeCalculationError::RepeatedEntries => Ok(FeeCalculationErrorCode::RepeatedEntries),
		other => Err(other),
	}
}

pub fn try_convert_extra_contract_deploy(
	err: MalformedContractDeploy,
) -> Result<MalformedContractDeployErrorCode, MalformedContractDeploy> {
	match err {
		MalformedContractDeploy::TooManyOperations { .. } => {
			Ok(MalformedContractDeployErrorCode::TooManyOperations)
		},
		other => Err(other),
	}
}

pub fn try_convert_extra_zswap_invalid(
	err: ZswapTransactionInvalid,
) -> Result<ZswapInvalidErrorCode, ZswapTransactionInvalid> {
	match err {
		ZswapTransactionInvalid::MerkleTreeError(_) => Ok(ZswapInvalidErrorCode::MerkleTreeError),
		other => Err(other),
	}
}

pub fn try_convert_extra_system_tx(
	err: LedgerSystemTransactionError,
) -> Result<SystemTransactionError, LedgerSystemTransactionError> {
	match err {
		LedgerSystemTransactionError::MerkleTreeError(_) => {
			Ok(SystemTransactionError::MerkleTreeError)
		},
		LedgerSystemTransactionError::InitialNonceAlreadyPresent(_) => {
			Ok(SystemTransactionError::GenerationInfoAlreadyPresent)
		},
		other => Err(other),
	}
}

#[cfg(test)]
mod tests {
	use crate::ledger_9::{
		ledger_storage_local::db::InMemoryDB,
		mn_ledger_local::error::{
			FeeCalculationError, MalformedContractDeploy, MalformedTransaction, TransactionInvalid,
		},
		onchain_runtime_local::state::EntryPointBuf,
		types::{
			FeeCalculationErrorCode, InvalidError, MalformedContractDeployErrorCode, MalformedError,
		},
	};
	use base_crypto::{cost_model::CostDuration, hash::HashOutput};
	use coin_structure_ledger_9::contract::ContractAddress;

	type Mt = MalformedTransaction<InMemoryDB>;
	type Ti = TransactionInvalid<InMemoryDB>;

	fn addr() -> ContractAddress {
		ContractAddress(HashOutput([7; 32]))
	}

	/// Every `MalformedTransaction` variant this module maps gets its own code, not the
	/// `UnknownError` catch-all.
	#[test]
	fn malformed_variants_are_not_unknown_error() {
		let ps = CostDuration::from_picoseconds;
		let cases: Vec<(Mt, MalformedError)> = vec![
			(
				Mt::FeeCalculation(FeeCalculationError::RepeatedEntries),
				MalformedError::FeeCalculation(FeeCalculationErrorCode::RepeatedEntries),
			),
			(
				Mt::MalformedContractDeploy(MalformedContractDeploy::TooManyOperations {
					operations: 257,
					limit: 256,
				}),
				MalformedError::MalformedContractDeploy(
					MalformedContractDeployErrorCode::TooManyOperations,
				),
			),
			(
				Mt::TooManyPublicInputs {
					address: addr(),
					operation: EntryPointBuf(b"op".to_vec()),
					public_inputs: 2,
					limit: 1,
				},
				MalformedError::TooManyPublicInputs,
			),
			(
				Mt::TranscriptGasExceedsBlockLimits {
					address: addr(),
					declared_compute_time: ps(2),
					declared_read_time: ps(2),
					compute_time_limit: ps(1),
					read_time_limit: ps(1),
				},
				MalformedError::TranscriptGasExceedsBlockLimits,
			),
			(Mt::ContractDustTransfer { address: addr() }, MalformedError::ContractDustTransfer),
		];
		for (upstream, expected) in cases {
			assert_eq!(MalformedError::from(upstream), expected);
		}
	}

	/// Same for `TransactionInvalid`.
	#[test]
	fn invalid_variants_are_not_unknown_error() {
		let cases: Vec<(Ti, InvalidError)> = vec![
			(
				Ti::BridgeClaimTooSmall { claimed: 1, minimum: 2, claimable: 3 },
				InvalidError::BridgeClaimTooSmall,
			),
			(
				Ti::ContractTooManyOperations { address: addr(), operations: 257, limit: 256 },
				InvalidError::ContractTooManyOperations,
			),
		];
		for (upstream, expected) in cases {
			assert_eq!(InvalidError::from(upstream), expected);
		}
	}
}
