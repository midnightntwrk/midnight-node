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

use super::{
	base_crypto_local, coin_structure_local, ledger_storage_local, midnight_serialize_local,
	mn_ledger_local, onchain_runtime_local, transient_crypto_local, zswap_local,
};
use base_crypto_local::{hash::HashOutput as HashOutputLedger, time::Timestamp};
use coin_structure_local::coin::{NIGHT, TokenType};
use derive_where::derive_where;
use ledger_storage_local::db::DB;
use ledger_storage_local::{
	Storable,
	arena::{ArenaKey, Sp},
	storable::Loader,
	storage::default_storage,
};
use midnight_serialize_local::{Deserializable, Serializable, Version, Versioned};
use mn_ledger_local::{
	semantics::{TransactionContext, TransactionResult},
	structure::{LedgerParameters, LedgerState, SignatureKind},
};
use onchain_runtime_local::context::BlockContext as LedgerBlockContext;
use std::{borrow::Borrow, collections::HashMap};
use transient_crypto_local::merkle_tree::MerkleTreeDigest;
use zswap_local::ledger::State as ZswapLedgerState;

use super::{
	Api, ContractAddress, ContractState, DeserializableError, LOG_TARGET, SerializableError,
	SystemTransaction, Transaction, TransactionInvalid, UserAddress, ZswapState,
	types::{DeserializationError, LedgerApiError, SerializationError, TransactionError},
};

use crate::common::types::BlockContext;

#[derive(Debug)]
pub enum AppliedStage<D: DB> {
	AllApplied,
	PartialSuccess(HashMap<u16, Result<(), TransactionInvalid<D>>>),
}

#[derive(Debug, Default, Storable)]
#[derive_where(Clone)]
#[storable(db = D)]
pub struct Ledger<D: DB>(pub LedgerState<D>);

// NOTE: We only use Serialize/Deserialize on LedgerState for debugging purposes
impl<D: DB> Serializable for Ledger<D> {
	fn unversioned_serialize<W: std::io::Write>(
		value: &Self,
		writer: &mut W,
	) -> Result<(), std::io::Error> {
		Serializable::serialize(&value.0, writer)
	}

	fn unversioned_serialized_size(value: &Self) -> usize {
		Serializable::serialized_size(&value.0)
	}
}

impl<D: DB> Versioned for Ledger<D> {
	const VERSION: Option<Version> = None;
	const NETWORK_SPECIFIC: bool = <LedgerState<D> as Versioned>::NETWORK_SPECIFIC;
}

impl<D: DB> Deserializable for Ledger<D> {
	fn versioned_deserialize<R: std::io::Read>(
		reader: &mut R,
		_version: Option<&Version>,
		recursion_level: u32,
	) -> Result<Self, std::io::Error> {
		let ledger = LedgerState::deserialize(reader, recursion_level)?;
		Ok(Ledger(ledger))
	}
}

impl<D: DB> SerializableError for Ledger<D> {
	fn error() -> SerializationError {
		SerializationError::LedgerState
	}
}

impl<D: DB> DeserializableError for Ledger<D> {
	fn error() -> DeserializationError {
		DeserializationError::LedgerState
	}
}

impl SerializableError for LedgerParameters {
	fn error() -> SerializationError {
		SerializationError::LedgerParameters
	}
}

impl SerializableError for MerkleTreeDigest {
	fn error() -> SerializationError {
		SerializationError::MerkleTreeDigest
	}
}

impl<D: DB> Ledger<D> {
	// grcov-excl-start
	pub(crate) fn get_zswap_state(
		&self,
		maybe_contract_address: Option<ContractAddress>,
	) -> ZswapState<D> {
		let mut state = ZswapLedgerState::new();

		state.coin_coms = if let Some(contract_address) = maybe_contract_address {
			self.0.zswap.filter(&[contract_address])
		} else {
			self.0.zswap.coin_coms.clone()
		};

		state
	}

	pub(crate) fn get_zswap_state_root(&self) -> MerkleTreeDigest {
		let state = Self::get_zswap_state(self, None);
		state.coin_coms.root()
	}

	// grcov-excl-stop
	pub(crate) fn get_contract_state(
		&self,
		contract_address: ContractAddress,
	) -> Option<ContractState<D>> {
		self.0.index(contract_address)
	}

	pub(crate) fn apply_transaction<S: SignatureKind<D>>(
		sp: Sp<Self, D>,
		api: &Api,
		tx: &Transaction<S, D>,
		ctx: &TransactionContext<D>,
	) -> Result<(Sp<Self, D>, AppliedStage<D>), LedgerApiError> {
		let (next_state, result) = sp.0.apply(tx.borrow(), ctx);
		let new_sp = default_storage::<D>().arena.alloc(Ledger(next_state));

		match result {
			TransactionResult::Success => Ok((new_sp, AppliedStage::AllApplied)),
			TransactionResult::PartialSuccess(segments) => {
				log::warn!(
					target: LOG_TARGET,
					"Non guaranteed part of the transaction failed tx_hash = {:?}, segments = {:?}",
					tx.identifiers().map(|i|api.serialize(&i)).collect::<Vec<_>>(),
					segments
				);
				Ok((new_sp, AppliedStage::PartialSuccess(segments)))
			},
			TransactionResult::Failure(reason) => {
				log::warn!(target: LOG_TARGET, "Error applying Transaction: {:?}", reason);
				Err(LedgerApiError::Transaction(TransactionError::Invalid(reason.into())))
			},
		}
	}

	pub(crate) fn post_block_update(
		sp: Sp<Self, D>,
		block_context: BlockContext,
	) -> Result<Sp<Self, D>, LedgerApiError> {
		let next_state = sp.0.post_block_update(Timestamp::from_secs(block_context.tblock));
		let new_sp = default_storage::<D>().arena.alloc(Ledger(next_state));
		Ok(new_sp)
	}

	pub(crate) fn validate_transaction<S: SignatureKind<D>>(
		&self,
		tx: &Transaction<S, D>,
		block_context: &BlockContext,
	) -> Result<(), LedgerApiError> {
		tx.validate(self, block_context)
	}

	pub(crate) fn apply_system_tx(
		sp: Sp<Self, D>,
		tx: &SystemTransaction,
	) -> Result<Sp<Self, D>, LedgerApiError> {
		let next_state = sp.0.apply_system_tx(tx, Timestamp::from_secs(0)).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error applying System Transaction: {:?}", e);
			LedgerApiError::Transaction(TransactionError::SystemTransaction(e.into()))
		})?;
		Ok(default_storage::<D>().arena.alloc(Ledger(next_state)))
	}

	pub(crate) fn get_unclaimed_amount(&self, beneficiary: UserAddress) -> Option<&u128> {
		match self.0.unclaimed_mints.get(&beneficiary) {
			Some(coins) => coins.get(&TokenType::Unshielded(NIGHT)),
			None => None,
		}
	}

	pub(crate) fn get_parameters(&self) -> LedgerParameters {
		self.0.parameters.clone()
	}

	pub(crate) fn get_transaction_context(
		&self,
		block_context: BlockContext,
	) -> TransactionContext<D> {
		let block_hash: [u8; 32] = block_context
			.parent_block_hash
			.try_into()
			.expect("Runtime is using `sp_core:H256` which is 32 bytes");

		TransactionContext {
			ref_state: self.0.clone(),
			block_context: LedgerBlockContext {
				tblock: Timestamp::from_secs(block_context.tblock),
				tblock_err: block_context.tblock_err,
				parent_block_hash: HashOutputLedger(block_hash),
			},
			whitelist: None,
		}
	}
}

impl<D: DB> Borrow<LedgerState<D>> for Ledger<D> {
	fn borrow(&self) -> &LedgerState<D> {
		&self.0
	}
}

// grcov-excl-start
#[cfg(test)]
mod tests {
	use super::super::super::super::CRATE_NAME;
	use super::super::super::json::contract_state_json;
	use super::super::Api;
	use super::*;
	use base_crypto_local::signatures::Signature;
	use ledger_storage_local::DefaultDB;
	use midnight_node_res::{
		networks::{MidnightNetwork, UndeployedNetwork},
		undeployed::transactions::{CHECK_TX, CONTRACT_ADDR, DEPLOY_TX, MAINTENANCE_TX, STORE_TX},
	};
	use midnight_serialize_local::NetworkId;

	const NETWORK_ID: NetworkId = NetworkId::Undeployed;

	fn prepare_ledger(api: &Api) -> Sp<Ledger<DefaultDB>> {
		let genesis = UndeployedNetwork.genesis_state();

		let ledger: Result<Ledger<DefaultDB>, LedgerApiError> = api.deserialize(genesis);
		assert!(ledger.is_ok(), "Can't deserialize ledger from genesis: {}", ledger.unwrap_err());
		Sp::new(ledger.unwrap())
	}

	fn assert_apply_transaction(
		api: &Api,
		ledger: &mut Sp<Ledger<DefaultDB>>,
		bytes: &[u8],
		block_context: &BlockContext,
	) -> AppliedStage<DefaultDB> {
		let tx =
			api.deserialize::<Transaction<Signature, DefaultDB>>(hex::encode(bytes).as_bytes());
		assert!(tx.is_ok(), "Can't deserialize transaction: {}", tx.unwrap_err());
		let tx_ctx = ledger.get_transaction_context(block_context.clone());
		let result =
			Ledger::<DefaultDB>::apply_transaction(ledger.clone(), api, &tx.unwrap(), &tx_ctx);
		assert!(result.is_ok(), "Can't apply transaction: {}", result.unwrap_err());
		let applied_stage;
		(*ledger, applied_stage) = result.unwrap();
		applied_stage
	}

	#[test]
	fn should_convert_to_and_from_bytes() {
		let ledger: LedgerState<DefaultDB> = LedgerState::default();
		let mut bytes = vec![];
		assert!(midnight_serialize_local::serialize(&ledger, &mut bytes, NETWORK_ID).is_ok());
		let _: LedgerState<DefaultDB> =
			midnight_serialize_local::deserialize(&bytes[..], NETWORK_ID).unwrap();
	}

	#[test]
	#[ignore = "TODO UNSHIELDED"]
	fn should_apply_transaction() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = Api::new(NETWORK_ID);
		let mut ledger = prepare_ledger(&api);
		let block_context = Default::default();
		assert_apply_transaction(&api, &mut ledger, DEPLOY_TX, &block_context);
	}

	#[test]
	#[ignore = "TODO UNSHIELDED"]
	fn should_get_contract_state_json() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = Api::new(NETWORK_ID);
		let mut ledger = prepare_ledger(&api);

		let block_context = Default::default();
		assert_apply_transaction(&api, &mut ledger, DEPLOY_TX, &block_context);
		assert_apply_transaction(&api, &mut ledger, STORE_TX, &block_context);
		assert_apply_transaction(&api, &mut ledger, CHECK_TX, &block_context);
		assert_apply_transaction(&api, &mut ledger, MAINTENANCE_TX, &block_context);

		let a = CONTRACT_ADDR;
		let addr = hex::decode(a).unwrap();
		let addr = api.deserialize::<ContractAddress>(&addr).unwrap();
		let state = ledger.get_contract_state(addr);
		assert!(
			state.is_some(),
			"Contract state not found for address {}",
			String::from_utf8_lossy(a)
		);
		let result = contract_state_json(state.unwrap());
		assert!(
			result.is_ok(),
			"Can't transform the contract state into JSON: {}",
			result.unwrap_err()
		);
	}

	#[test]
	#[ignore = "TODO UNSHIELDED"]
	fn should_get_contract_state() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = Api::new(NETWORK_ID);
		let mut ledger = prepare_ledger(&api);
		let block_context = Default::default();
		assert_apply_transaction(&api, &mut ledger, DEPLOY_TX, &block_context);
		assert_apply_transaction(&api, &mut ledger, STORE_TX, &block_context);
		assert_apply_transaction(&api, &mut ledger, CHECK_TX, &block_context);
		assert_apply_transaction(&api, &mut ledger, MAINTENANCE_TX, &block_context);

		let a = CONTRACT_ADDR;
		let addr = hex::decode(a).unwrap();
		let addr = api.deserialize::<ContractAddress>(&addr).unwrap();
		let state = ledger.get_contract_state(addr);
		assert!(
			state.is_some(),
			"Contract state not found for address {}",
			String::from_utf8_lossy(a)
		);
	}
}
// grcov-excl-stop
