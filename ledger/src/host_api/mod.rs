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
		BlockContext, GasCost, Hash, StorageCost, SystemTransactionAppliedStateRoot,
		TransactionAppliedStateRoot, TransactionDetails, Tx,
	},
	hard_fork_test, latest,
};
use sp_runtime_interface::pass_by::{
	AllocateAndReturnByCodec, AllocateAndReturnFatPointer, PassFatPointerAndDecode,
	PassFatPointerAndRead,
};
use sp_runtime_interface::runtime_interface;
use sp_std::vec::Vec;

#[cfg(feature = "std")]
type Database = ledger_storage::db::ParityDb;

// TODO: hard fork
#[cfg(feature = "std")]
// type DatabaseHF = ledger_storage::db::ParityDb;
type DatabaseHF = ledger_storage_hf::db::ParityDb;

#[cfg(feature = "std")]
type Signature = base_crypto::signatures::Signature;

// TODO: hard fork
#[cfg(feature = "std")]
// type SignatureHF = base_crypto::signatures::Signature;
type SignatureHF = base_crypto_hf::signatures::Signature;

#[runtime_interface]
pub trait LedgerBridge {
	fn set_default_storage(&mut self) {
		latest::Bridge::<Signature, Database>::set_default_storage(*self)
	}

	fn drop_default_storage(&mut self) {
		// Do nothing. No DB exists prior this version.
		// Method should exist though to easiy reuse runtimes between
		// hard-fork and no hard-fork versions.
	}

	fn flush_storage(&mut self) {
		latest::Bridge::<Signature, Database>::flush_storage(*self)
	}

	fn pre_fetch_storage(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<(), latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::pre_fetch_storage(*self, state_key)
	}

	fn post_block_update(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::post_block_update(*self, state_key, block_context)
	}

	// Current Enabled Version
	fn get_version() -> AllocateAndReturnFatPointer<Vec<u8>> {
		latest::Bridge::<Signature, Database>::get_version()
	}

	/*
	 * apply_transaction()
	 */
	fn apply_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<TransactionAppliedStateRoot, latest::types::LedgerApiError>>
	{
		latest::Bridge::<Signature, Database>::apply_transaction(
			*self,
			state_key,
			tx,
			block_context,
		)
	}

	fn apply_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<
		Result<SystemTransactionAppliedStateRoot, latest::types::LedgerApiError>,
	> {
		latest::Bridge::<Signature, Database>::apply_system_transaction(
			*self,
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
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		runtime_version: u32,
	) -> AllocateAndReturnByCodec<Result<(Hash, TransactionDetails), latest::types::LedgerApiError>>
	{
		latest::Bridge::<Signature, Database>::validate_transaction(
			*self,
			state_key,
			tx,
			block_context,
			runtime_version,
		)
	}

	/*
	 * get_contract_state()
	 */
	// Current Enabled Version
	fn get_contract_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::get_contract_state(state_key, contract_address)
	}

	/*
	 * get_decoded_transaction()
	 */
	// Current Enabled Version
	fn get_decoded_transaction(
		transaction_bytes: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Tx, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::get_decoded_transaction(transaction_bytes)
	}

	/*
	 * get_zswap_chain_state()
	 */
	// Current Enabled Version
	fn get_zswap_chain_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::get_zswap_chain_state(state_key, contract_address)
	}

	/*
	 * Mints system coins for block rewards
	 */
	// Current Enabled Version
	fn mint_coins(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		amount: PassFatPointerAndDecode<u128>,
		receiver: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::mint_coins(
			state_key,
			amount,
			receiver,
			block_context,
		)
	}

	/*
	 * Returns the unclaimed amount for a provided beneficiary address
	 */
	// Current Enabled Version
	fn get_unclaimed_amount(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		beneficiary: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<u128, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::get_unclaimed_amount(state_key, beneficiary)
	}

	/*
	 * Returns the Ledger Parameters
	 */
	// Current Enabled Version
	fn get_ledger_parameters(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::get_ledger_parameters(state_key)
	}

	/*
	 * Returns the expected fee to pay for a submitting a transaction
	 */
	fn get_transaction_cost(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<(StorageCost, GasCost), latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::get_transaction_cost(state_key, tx, &block_context)
	}

	/*
	 * Returns the Zsawp state root
	 */
	// Current Enabled Version
	fn get_zswap_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::get_zswap_state_root(state_key)
	}

	fn construct_cnight_generates_dust_event(
		value: PassFatPointerAndDecode<u128>,
		owner: PassFatPointerAndRead<&[u8]>,
		time: u64,
		action: u8,
		nonce: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::construct_cnight_generates_dust_event(
			value, owner, time, action, nonce,
		)
	}

	fn construct_cnight_generates_dust_system_tx(
		events: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, latest::types::LedgerApiError>> {
		latest::Bridge::<Signature, Database>::construct_cnight_generates_dust_system_tx(events)
	}
}

#[runtime_interface]
pub trait LedgerBridgeHf {
	fn set_default_storage(&mut self) {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::set_default_storage(*self)
	}

	fn drop_default_storage(&mut self) {
		use ledger_storage::{
			db::ParityDb,
			storage::{try_get_default_storage, unsafe_drop_default_storage},
		};
		unsafe_drop_default_storage::<ParityDb>();

		match try_get_default_storage::<ParityDb>() {
			Some(_) => {
				log::error!(
					target: hard_fork_test::LOG_TARGET,
					"Pre Hard-fork Default Storage wasn't successfully dropped, still exists",
				);
			},
			None => {
				log::info!(
					target: hard_fork_test::LOG_TARGET,
					"Pre Hard-fork Default Storage was successfully dropped",
				);
			},
		};
	}

	fn flush_storage(&mut self) {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::flush_storage(*self)
	}

	fn pre_fetch_storage(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<(), hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::pre_fetch_storage(*self, state_key)
	}

	fn post_block_update(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::post_block_update(
			*self,
			state_key,
			block_context,
		)
	}

	// Version for hard-fork
	fn get_version() -> AllocateAndReturnFatPointer<Vec<u8>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_version()
	}

	// Hard-fork Version
	fn apply_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<
		Result<TransactionAppliedStateRoot, hard_fork_test::types::LedgerApiError>,
	> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::apply_transaction(
			*self,
			state_key,
			tx,
			block_context,
		)
	}

	fn apply_system_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		_runtime_version: u32,
	) -> AllocateAndReturnByCodec<
		Result<SystemTransactionAppliedStateRoot, hard_fork_test::types::LedgerApiError>,
	> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::apply_system_transaction(
			*self,
			state_key,
			tx,
			block_context,
		)
	}

	// Hard-fork Version
	fn validate_transaction(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
		runtime_version: u32,
	) -> AllocateAndReturnByCodec<
		Result<(Hash, TransactionDetails), hard_fork_test::types::LedgerApiError>,
	> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::validate_transaction(
			*self,
			state_key,
			tx,
			block_context,
			runtime_version,
		)
	}

	// Hard-fork Version
	fn get_contract_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_contract_state(
			state_key,
			contract_address,
		)
	}

	// Hard-fork Version
	fn get_decoded_transaction(
		transaction_bytes: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Tx, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_decoded_transaction(
			transaction_bytes,
		)
	}

	// Hard-fork Version
	fn get_zswap_chain_state(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		contract_address: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_zswap_chain_state(
			state_key,
			contract_address,
		)
	}

	fn construct_cnight_generates_dust_event(
		value: PassFatPointerAndDecode<u128>,
		owner: PassFatPointerAndRead<&[u8]>,
		time: u64,
		action: u8,
		nonce: PassFatPointerAndDecode<[u8; 32]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::construct_cnight_generates_dust_event(
			value, owner, time, action, nonce,
		)
	}

	fn construct_cnight_generates_dust_system_tx(
		events: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::construct_cnight_generates_dust_system_tx(
			events,
		)
	}

	// Hard-fork Version
	fn mint_coins(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		amount: PassFatPointerAndDecode<u128>, //TODO can we be more efficient?
		receiver: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::mint_coins(
			state_key,
			amount,
			receiver,
			block_context,
		)
	}

	// Hard-fork Version
	fn get_unclaimed_amount(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		beneficiary: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<u128, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_unclaimed_amount(
			state_key,
			beneficiary,
		)
	}

	// Hard-fork Version
	fn get_ledger_parameters(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_ledger_parameters(state_key)
	}

	// Hard-fork Version
	fn get_transaction_cost(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
		tx: PassFatPointerAndRead<&[u8]>,
		block_context: PassFatPointerAndDecode<BlockContext>,
	) -> AllocateAndReturnByCodec<
		Result<(StorageCost, GasCost), hard_fork_test::types::LedgerApiError>,
	> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_transaction_cost(
			state_key,
			tx,
			&block_context,
		)
	}

	// Hard-fork Version
	fn get_zswap_state_root(
		&mut self,
		state_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, hard_fork_test::types::LedgerApiError>> {
		hard_fork_test::Bridge::<SignatureHF, DatabaseHF>::get_zswap_state_root(state_key)
	}
}
