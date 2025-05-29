// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use crate::{
	common::types::{
		BlockContext, GasCost, Hash, StorageCost, TransactionAppliedStateRoot, TransactionDetails,
		Tx,
	},
	hard_fork_test, latest,
};
use sp_runtime_interface::runtime_interface;
use sp_std::vec::Vec;

#[cfg(feature = "runtime-benchmarks")]
use crate::types::{BenchmarkClaimMintTxBuilder, BenchmarkStandardTxBuilder};

#[cfg(feature = "std")]
type Database = ledger_storage::db::ParityDb;

#[cfg(feature = "std")]
type DatabaseHF = ledger_storage_hf::db::ParityDb;

#[cfg(feature = "std")]
type Signature = base_crypto::signatures::Signature;

#[cfg(feature = "std")]
type SignatureHF = base_crypto_hf::signatures::Signature;

#[runtime_interface]
pub trait LedgerBridge {
	fn flush_storage(&mut self) {
		latest::Bridge::<Signature, Database>::flush_storage(*self)
	}

	fn pre_fetch_storage(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
	) -> Result<(), latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::pre_fetch_storage(*self, network_id, state_key)
	}

	fn post_block_update(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		block_context: BlockContext,
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::post_block_update(
			*self,
			network_id,
			state_key,
			block_context,
		)
	}

	// Current Enabled Version
	fn get_version() -> Vec<u8> {
		latest::Bridge::<Signature, Database>::get_version()
	}

	/*
	 * apply_transaction()
	 */
	fn apply_transaction(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		tx: &[u8],
		block_context: BlockContext,
		_runtime_version: u32,
	) -> Result<TransactionAppliedStateRoot, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::apply_transaction(
			*self,
			network_id,
			state_key,
			tx,
			block_context,
		)
	}

	/*
	 * validate_transaction()
	 */
	// Current Enabled Version
	fn validate_transaction(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		tx: &[u8],
		block_context: BlockContext,
		runtime_version: u32,
	) -> Result<(Hash, TransactionDetails), latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::validate_transaction(
			*self,
			network_id,
			state_key,
			tx,
			block_context,
			runtime_version,
		)
	}

	/*
	 * get_contract_state_json()
	 */
	// Current Enabled Version
	fn get_contract_state_json(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_contract_state_json(
			network_id,
			state_key,
			contract_address,
		)
	}

	/*
	 * get_contract_state()
	 */
	// Current Enabled Version
	fn get_contract_state(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_contract_state(
			network_id,
			state_key,
			contract_address,
		)
	}

	/*
	 * get_decoded_transaction()
	 */
	// Current Enabled Version
	fn get_decoded_transaction(
		network_id: &[u8],
		transaction_bytes: &[u8],
	) -> Result<Tx, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_decoded_transaction(
			network_id,
			transaction_bytes,
		)
	}

	/*
	 * get_zswap_chain_state()
	 */
	// Current Enabled Version
	fn get_zswap_chain_state(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_zswap_chain_state(
			network_id,
			state_key,
			contract_address,
		)
	}

	/*
	 * Mints system coins for block rewards
	 */
	// Current Enabled Version
	fn mint_coins(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		amount: u128,
		receiver: &[u8],
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::mint_coins(network_id, state_key, amount, receiver)
	}

	/*
	 * Returns the unclaimed amount for a provided beneficiary address
	 */
	// Current Enabled Version
	fn get_unclaimed_amount(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		beneficiary: &[u8],
	) -> Result<u128, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_unclaimed_amount(
			network_id,
			state_key,
			beneficiary,
		)
	}

	/*
	 * Returns the Ledger Parameters
	 */
	// Current Enabled Version
	fn get_ledger_parameters(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_ledger_parameters(network_id, state_key)
	}

	/*
	 * Returns the expected fee to pay for a submitting a transaction
	 */
	fn get_transaction_cost(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		tx: &[u8],
		block_context: BlockContext,
	) -> Result<(StorageCost, GasCost), latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_transaction_cost(
			network_id,
			state_key,
			tx,
			&block_context,
		)
	}

	/*
	 * Returns the Zsawp state root
	 */
	// Current Enabled Version
	fn get_zswap_state_root(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::get_zswap_state_root(network_id, state_key)
	}

	/*
	 * Helper host_api method to generate Standard transactions:
	 * Needed as `mn-ledger` does not support `no_std` and it'd leak into the becnchmarks otherwise
	 */
	// Current Enabled Version
	#[cfg(feature = "runtime-benchmarks")]
	fn build_standard_transactions(
		network_id: &[u8],
		ledger_state: &[u8],
		args: BenchmarkStandardTxBuilder,
	) -> Result<(Vec<u8>, Vec<u8>), latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::build_standard_transactions(
			network_id,
			ledger_state,
			args,
		)
	}

	/*
	 * Helper host_api method to generate ClaimMint transactions:
	 * Needed as `mn-ledger` does not support `no_std` and it'd leak into the becnchmarks otherwise
	 */
	// Current Enabled Version
	#[cfg(feature = "runtime-benchmarks")]
	fn build_claim_mint_transactions(
		network_id: &[u8],
		ledger_state: &[u8],
		args: BenchmarkClaimMintTxBuilder,
	) -> Result<Vec<u8>, latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::build_claim_mint_transactions(
			network_id,
			ledger_state,
			args,
		)
	}

	/*
	 * Helper host_api method to execute a contract call for the purpose of
	 * benchmarking its execution time in relation to its `gas_cost`
	 * Needed as `mn-ledger` does not support `no_std` and it'd leak into the becnchmarks otherwise
	 */
	// Current Enabled Version
	#[cfg(feature = "runtime-benchmarks")]
	fn execute_contract_call(
		network_id: &[u8],
		ledger_state: &[u8],
		tx: &[u8],
	) -> Result<(), latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::execute_contract_call(network_id, ledger_state, tx)
	}

	/*
	 * Helper host_api method to benchmark transaction deserealization
	 * Needed as `mn-ledger` does not support `no_std` and it'd leak into the becnchmarks otherwise
	 */
	// Current Enabled Version
	#[cfg(feature = "runtime-benchmarks")]
	fn deserialize_transaction(
		network_id: &[u8],
		tx: &[u8],
	) -> Result<(), latest::types::LedgerApiError> {
		latest::Bridge::<Signature, Database>::deserialize_transaction(network_id, tx)
	}
}

#[runtime_interface]
pub trait LedgerBridgeHf {
	fn flush_storage(&mut self) {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::flush_storage(*self)
	}

	fn pre_fetch_storage(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
	) -> Result<(), hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::pre_fetch_storage(
			*self, network_id, state_key,
		)
	}

	fn post_block_update(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		block_context: BlockContext,
	) -> Result<Vec<u8>, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::post_block_update(
			*self,
			network_id,
			state_key,
			block_context,
		)
	}

	// Version for hard-fork
	fn get_version() -> Vec<u8> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_version()
	}

	// Hard-fork Version
	fn apply_transaction(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		tx: &[u8],
		block_context: BlockContext,
		_runtime_version: u32,
	) -> Result<TransactionAppliedStateRoot, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::apply_transaction(
			*self,
			network_id,
			state_key,
			tx,
			block_context,
		)
	}

	// Hard-fork Version
	fn validate_transaction(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		tx: &[u8],
		block_context: BlockContext,
		runtime_version: u32,
	) -> Result<(Hash, TransactionDetails), hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::validate_transaction(
			*self,
			network_id,
			state_key,
			tx,
			block_context,
			runtime_version,
		)
	}

	// Hard-fork Version
	fn get_contract_state_json(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_contract_state_json(
			network_id,
			state_key,
			contract_address,
		)
	}

	// Hard-fork Version
	fn get_contract_state(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_contract_state(
			network_id,
			state_key,
			contract_address,
		)
	}

	// Hard-fork Version
	fn get_decoded_transaction(
		network_id: &[u8],
		transaction_bytes: &[u8],
	) -> Result<Tx, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_decoded_transaction(
			network_id,
			transaction_bytes,
		)
	}

	// Hard-fork Version
	fn get_zswap_chain_state(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		contract_address: &[u8],
	) -> Result<Vec<u8>, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_zswap_chain_state(
			network_id,
			state_key,
			contract_address,
		)
	}

	// Hard-fork Version
	fn mint_coins(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		amount: u128,
		receiver: &[u8],
	) -> Result<Vec<u8>, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::mint_coins(
			network_id, state_key, amount, receiver,
		)
	}

	// Hard-fork Version
	fn get_unclaimed_amount(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		beneficiary: &[u8],
	) -> Result<u128, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_unclaimed_amount(
			network_id,
			state_key,
			beneficiary,
		)
	}

	// Hard-fork Version
	fn get_ledger_parameters(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
	) -> Result<Vec<u8>, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_ledger_parameters(
			network_id, state_key,
		)
	}

	// Hard-fork Version
	fn get_transaction_cost(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
		tx: &[u8],
		block_context: BlockContext,
	) -> Result<(StorageCost, GasCost), hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_transaction_cost(
			network_id,
			state_key,
			tx,
			&block_context,
		)
	}

	// Hard-fork Version
	fn get_zswap_state_root(
		&mut self,
		network_id: &[u8],
		state_key: &[u8],
	) -> Result<Vec<u8>, hard_fork_test::types::LedgerApiError> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_zswap_state_root(
			network_id, state_key,
		)
	}
}
