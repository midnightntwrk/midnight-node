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
	base_crypto_local, helpers_local, ledger_storage_local, midnight_serialize_local,
	mn_ledger_local, onchain_runtime_local, transient_crypto_local, zswap_local,
};
use base_crypto_local::{
	cost_model::SyntheticCost, hash::HashOutput as HashOutputLedger, time::Timestamp,
};
use derive_where::derive_where;
use ledger_storage_local::{
	self as storage, Storable,
	arena::{ArenaKey, Sp},
	db::DB,
	storable::Loader,
	storage::default_storage,
};

use helpers_local::StorableSyntheticCost;
use midnight_serialize_local::{self as serialize, Tagged};
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

#[derive(Debug, Storable)]
#[derive_where(Clone)]
#[storable(db = D)]
pub struct Ledger<D: DB> {
	pub state: LedgerState<D>,
	block_fullness: StorableSyntheticCost<D>,
}

impl<D: DB> Tagged for Ledger<D> {
	fn tag() -> std::borrow::Cow<'static, str> {
		<LedgerState<D> as Tagged>::tag()
	}

	fn tag_unique_factor() -> String {
		<LedgerState<D> as Tagged>::tag_unique_factor()
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
	pub fn new(state: LedgerState<D>) -> Self {
		Self { state, block_fullness: SyntheticCost::ZERO.into() }
	}

	pub(crate) fn get_zswap_state(
		&self,
		maybe_contract_address: Option<ContractAddress>,
	) -> ZswapState<D> {
		let mut state = ZswapLedgerState::new();

		state.coin_coms = if let Some(contract_address) = maybe_contract_address {
			self.state.zswap.filter(&[contract_address])
		} else {
			self.state.zswap.coin_coms.clone()
		};

		state
	}

	pub(crate) fn get_zswap_state_root(&self) -> MerkleTreeDigest {
		let state = Self::get_zswap_state(self, None);
		// TODO: is this rehash necessary?
		state.coin_coms.rehash().root().unwrap()
	}

	// grcov-excl-stop
	pub(crate) fn get_contract_state(
		&self,
		contract_address: ContractAddress,
	) -> Option<ContractState<D>> {
		self.state.index(contract_address)
	}

	pub(crate) fn apply_transaction<S: SignatureKind<D>>(
		sp: Sp<Self, D>,
		api: &Api,
		tx: &Transaction<S, D>,
		ctx: &TransactionContext<D>,
	) -> Result<(Sp<Self, D>, AppliedStage<D>), LedgerApiError> {
		let tx_cost =
			tx.0.cost(&sp.state.parameters, true)
				.map_err(|_| LedgerApiError::FeeCalculationError)?;
		let valid_tx =
			tx.0.well_formed(
				&ctx.ref_state,
				mn_ledger_local::verify::WellFormedStrictness::default(),
				ctx.block_context.tblock,
			)
			.map_err(|e| LedgerApiError::Transaction(TransactionError::Malformed(e.into())))?;
		let (next_state, result) = sp.state.apply(&valid_tx, ctx);
		let next_block_fullness = tx_cost + sp.block_fullness.clone().into();
		let new_sp = default_storage::<D>()
			.arena
			.alloc(Ledger { state: next_state, block_fullness: next_block_fullness.into() });

		match result {
			TransactionResult::Success(_) => Ok((new_sp, AppliedStage::AllApplied)),
			TransactionResult::PartialSuccess(segments, _) => {
				log::warn!(
					target: LOG_TARGET,
					"Non guaranteed part of the transaction failed tx_hash = {:?}, segments = {:?}",
					tx.identifiers().map(|i| api.tagged_serialize(&i)).collect::<Vec<_>>(),
					segments
				);
				Ok((new_sp, AppliedStage::PartialSuccess(segments)))
			},
			TransactionResult::Failure(reason) => {
				log::warn!(target: LOG_TARGET, "Error applying Transaction: {reason:?}");
				Err(LedgerApiError::Transaction(TransactionError::Invalid(reason.into())))
			},
		}
	}

	pub(crate) fn post_block_update(
		sp: Sp<Self, D>,
		block_context: BlockContext,
	) -> Result<Sp<Self, D>, LedgerApiError> {
		let block_fullness = sp.block_fullness.clone().into();
		let next_state = sp
			.state
			.post_block_update(Timestamp::from_secs(block_context.tblock), block_fullness)
			.map_err(|_| LedgerApiError::BlockLimitExceededError)?;
		let new_sp = default_storage::<D>()
			.arena
			.alloc(Ledger { state: next_state, block_fullness: SyntheticCost::ZERO.into() });
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
		tblock: Timestamp,
	) -> Result<Sp<Self, D>, LedgerApiError> {
		let tx_cost = tx.cost(&sp.state.parameters);
		let (next_state, _) = sp.state.apply_system_tx(tx, tblock).map_err(|e| {
			log::error!(target: LOG_TARGET, "Error applying System Transaction: {e:?}");
			LedgerApiError::Transaction(TransactionError::SystemTransaction(e.into()))
		})?;
		let next_block_fullness = tx_cost + sp.block_fullness.clone().into();
		Ok(default_storage::<D>()
			.arena
			.alloc(Ledger { state: next_state, block_fullness: next_block_fullness.into() }))
	}

	pub(crate) fn get_unclaimed_amount(&self, beneficiary: UserAddress) -> Option<&u128> {
		self.state.unclaimed_block_rewards.get(&beneficiary)
	}

	pub(crate) fn get_parameters(&self) -> LedgerParameters {
		(*self.state.parameters).clone()
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
			ref_state: self.state.clone(),
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
		&self.state
	}
}

// grcov-excl-start
#[cfg(test)]
mod tests {
	use super::super::super::super::{
		CRATE_NAME, helpers_local::extract_info_from_tx_with_context,
	};
	use super::super::Api;
	use super::*;
	use base_crypto_local::signatures::Signature;
	use ledger_storage_local::DefaultDB;
	use midnight_node_res::{
		networks::{MidnightNetwork, UndeployedNetwork},
		undeployed::transactions::{CHECK_TX, CONTRACT_ADDR, DEPLOY_TX, MAINTENANCE_TX, STORE_TX},
	};
	use midnight_serialize_local::tagged_deserialize;
	use mn_ledger_local::structure::LedgerState;

	fn prepare_ledger() -> Sp<Ledger<DefaultDB>> {
		sp_tracing::try_init_simple();

		let genesis = UndeployedNetwork.genesis_state();

		let state: LedgerState<DefaultDB> = tagged_deserialize(genesis)
			.unwrap_or_else(|err| panic!("Can't deserialize ledger from genesis: {err}"));
		let ledger = Ledger::new(state);

		Sp::new(ledger)
	}

	fn assert_apply_transaction(
		api: &Api,
		ledger: &mut Sp<Ledger<DefaultDB>>,
		bytes: &[u8],
		block_context: &BlockContext,
	) {
		let tx = api.tagged_deserialize::<Transaction<Signature, DefaultDB>>(bytes);
		assert!(tx.is_ok(), "Can't deserialize transaction: {}", tx.unwrap_err());
		let tx_ctx = ledger.get_transaction_context(block_context.clone());
		let (mut new_ledger_state, _applied_stage) =
			Ledger::<DefaultDB>::apply_transaction(ledger.clone(), api, &tx.unwrap(), &tx_ctx)
				.unwrap_or_else(|err| panic!("Can't apply transaction: {err}"));

		new_ledger_state =
			Ledger::<DefaultDB>::post_block_update(new_ledger_state, block_context.clone())
				.expect("Post block update failed");

		*ledger = new_ledger_state;
	}

	#[test]
	fn should_convert_to_and_from_bytes() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let ledger: LedgerState<DefaultDB> = LedgerState::new("undeployed");
		let mut bytes = vec![];
		assert!(midnight_serialize_local::tagged_serialize(&ledger, &mut bytes).is_ok());
		let _: LedgerState<DefaultDB> =
			midnight_serialize_local::tagged_deserialize(&bytes[..]).unwrap();
	}

	#[test]
	fn should_apply_transaction() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = Api::new();
		let mut ledger = prepare_ledger();
		let (serialized_tx, block_context) = extract_info_from_tx_with_context(DEPLOY_TX);
		assert_apply_transaction(&api, &mut ledger, &serialized_tx, &block_context.into());
	}

	#[test]
	fn should_get_contract_state() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let api = Api::new();
		let mut ledger = prepare_ledger();

		let (deploy_tx, deploy_tx_block_context) = extract_info_from_tx_with_context(DEPLOY_TX);
		let (store_tx, store_tx_block_context) = extract_info_from_tx_with_context(STORE_TX);
		let (check_tx, check_tx_block_context) = extract_info_from_tx_with_context(CHECK_TX);
		let (maintenance_tx, maintenance_tx_block_context) =
			extract_info_from_tx_with_context(MAINTENANCE_TX);

		assert_apply_transaction(&api, &mut ledger, &deploy_tx, &deploy_tx_block_context.into());
		assert_apply_transaction(&api, &mut ledger, &store_tx, &store_tx_block_context.into());
		assert_apply_transaction(&api, &mut ledger, &check_tx, &check_tx_block_context.into());
		assert_apply_transaction(
			&api,
			&mut ledger,
			&maintenance_tx,
			&maintenance_tx_block_context.into(),
		);

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
