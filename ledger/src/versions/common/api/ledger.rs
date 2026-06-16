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

use super::{
	base_crypto_local, coin_structure_local, helpers_local, ledger_storage_local,
	midnight_serialize_local, mn_ledger_local, transient_crypto_local, zswap_local,
};
use base_crypto_local::{cost_model::SyntheticCost, time::Timestamp};
use coin_structure_local::coin::{NIGHT, TokenType};
use derive_where::derive_where;
use ledger_storage_local::{
	self as storage, Storable,
	arena::{ArenaKey, Sp},
	db::DB,
	storable::Loader,
	storage::default_storage,
};

use helpers_local::{StorableSyntheticCost, compute_overall_fullness};
use midnight_serialize_local::{self as serialize, Tagged};
use mn_ledger_local::{
	semantics::{TransactionContext, TransactionResult},
	structure::{LedgerParameters, LedgerState, SignatureKind},
};
use std::{borrow::Borrow, collections::BTreeMap};
use transient_crypto_local::merkle_tree::MerkleTreeDigest;
use zswap_local::ledger::State as ZswapLedgerState;

use super::{
	super::super::BlockContext,
	Api, ContractAddress, ContractState, DeserializableError, LOG_TARGET, SerializableError,
	SystemTransaction, Transaction, TransactionInvalid, UserAddress, ZswapState,
	types::{DeserializationError, LedgerApiError, SerializationError, TransactionError},
};

#[derive(Debug)]
pub enum AppliedStage<D: DB> {
	AllApplied,
	PartialSuccess(BTreeMap<u16, Result<(), TransactionInvalid<D>>>),
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

	/// Applies a pre-verified transaction to the ledger.
	///
	/// This is used when a `VerifiedTransaction` has been cached from a prior
	/// validation step, avoiding redundant ZK proof verification.
	pub(crate) fn apply_verified_transaction<S: SignatureKind<D>>(
		sp: Sp<Self, D>,
		api: &Api,
		tx: &Transaction<S, D>,
		verified_tx: &mn_ledger_local::structure::VerifiedTransaction<D>,
		ctx: &TransactionContext<D>,
	) -> Result<(Sp<Self, D>, AppliedStage<D>), LedgerApiError> {
		let tx_cost =
			tx.0.cost(&sp.state.parameters, true)
				.map_err(|_| LedgerApiError::FeeCalculationError)?;
		let (next_state, result) = sp.state.apply(verified_tx, ctx);
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
				Ok((new_sp, AppliedStage::PartialSuccess(segments.into_iter().collect())))
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
		let block_fullness: SyntheticCost = sp.block_fullness.clone().into();
		let block_limits = sp.state.parameters.limits.block_limits;
		let normalized_fullness =
			helpers_local::clamp_and_normalize(&block_fullness, &block_limits, "post_block_update");
		let overall_fullness = compute_overall_fullness(&normalized_fullness);
		let next_state = sp
			.state
			.post_block_update(
				Timestamp::from_secs(block_context.tblock),
				normalized_fullness,
				overall_fullness,
			)
			.map_err(|_| LedgerApiError::BlockLimitExceededError)?;
		let new_sp = default_storage::<D>()
			.arena
			.alloc(Ledger { state: next_state, block_fullness: SyntheticCost::ZERO.into() });
		Ok(new_sp)
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

	/// Move `to_locked + to_treasury` out of the reserve pool, crediting the locked pool and the
	/// NIGHT treasury respectively — the exact, supply-preserving database effect needed to
	/// correct an under-funded locked pool in place (Preview's #1674 condition), without a chain
	/// reset and without a new ledger `SystemTransaction` variant.
	///
	/// This is a plain edit of three already-`pub` `LedgerState` fields rather than a privileged
	/// transaction, so it requires only the stock `midnight-ledger`. It is safe by construction:
	///
	/// * **Supply-preserving** — the move only shuffles value *between* the three pools whose sum
	///   the ledger's NIGHT invariant tracks, so that sum (and therefore the invariant) is
	///   unchanged; no other state is touched.
	/// * **Reserve-guarded** — it refuses (leaving state untouched) when the reserve cannot cover
	///   the move, mirroring the ledger's own `IllegalReserveDistribution` guard.
	/// * **Side-effect-free** — `block_fullness` (the per-block weight accumulator, reset each
	///   block by `post_block_update`) is carried over verbatim.
	///
	/// `versions/common` is compiled once per ledger version; only the ledger-8 host fn
	/// (`seed_pools_to_target`) calls this, so the ledger-7 monomorphization is unused.
	#[allow(dead_code)]
	pub(crate) fn seed_pools_from_reserve(
		sp: Sp<Self, D>,
		to_locked: u128,
		to_treasury: u128,
	) -> Result<Sp<Self, D>, LedgerApiError> {
		let total = to_locked.saturating_add(to_treasury);
		if total > sp.state.reserve_pool {
			log::error!(
				target: LOG_TARGET,
				"seed_pools_from_reserve rejected: move {total} (locked {to_locked} + treasury {to_treasury}) exceeds reserve {}",
				sp.state.reserve_pool
			);
			return Err(LedgerApiError::HostApiError);
		}

		let mut state = sp.state.clone();
		state.reserve_pool -= total;
		state.locked_pool = state.locked_pool.saturating_add(to_locked);
		let night = state
			.treasury
			.get(&TokenType::Unshielded(NIGHT))
			.copied()
			.unwrap_or(0)
			.saturating_add(to_treasury);
		state.treasury = state.treasury.insert(TokenType::Unshielded(NIGHT), night);

		Ok(default_storage::<D>()
			.arena
			.alloc(Ledger { state, block_fullness: sp.block_fullness.clone() }))
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
	) -> Result<TransactionContext<D>, LedgerApiError> {
		Ok(TransactionContext {
			ref_state: self.state.clone(),
			block_context: block_context.try_into().map_err(|e| {
				log::error!(target: LOG_TARGET, "failed to convert block_context: {}", hex::encode(e));
				LedgerApiError::GetTransactionContextError
			})?,
			whitelist: None,
		})
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
	use super::super::super::super::{CRATE_NAME, helpers_local::extract_tx_with_context};
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
		let tx = api
			.tagged_deserialize::<Transaction<Signature, DefaultDB>>(bytes)
			.expect("failed to deserialize tx");
		let tx_ctx = ledger.get_transaction_context(block_context.clone()).unwrap();
		let verified_tx =
			tx.0.well_formed(
				&tx_ctx.ref_state,
				mn_ledger_local::verify::WellFormedStrictness::default(),
				tx_ctx.block_context.tblock,
			)
			.unwrap_or_else(|err| panic!("Transaction not well-formed: {err:?}"));
		let (mut new_ledger_state, _applied_stage) =
			Ledger::<DefaultDB>::apply_verified_transaction(
				ledger.clone(),
				api,
				&tx,
				&verified_tx,
				&tx_ctx,
			)
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
		let (serialized_tx, block_context) = extract_tx_with_context(DEPLOY_TX);
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

		let (deploy_tx, deploy_tx_block_context) = extract_tx_with_context(DEPLOY_TX);
		let (store_tx, store_tx_block_context) = extract_tx_with_context(STORE_TX);
		let (check_tx, check_tx_block_context) = extract_tx_with_context(CHECK_TX);
		let (maintenance_tx, maintenance_tx_block_context) =
			extract_tx_with_context(MAINTENANCE_TX);

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

	/// `(locked_pool, reserve_pool, treasury[NIGHT])` for a ledger state.
	fn pools(state: &LedgerState<DefaultDB>) -> (u128, u128, u128) {
		let treasury_night =
			state.treasury.get(&TokenType::Unshielded(NIGHT)).copied().unwrap_or(0);
		(state.locked_pool, state.reserve_pool, treasury_night)
	}

	/// The reserve → locked/treasury move must apply exactly the requested amounts and conserve the
	/// summed NIGHT supply across the three pools (the correctness guarantee that lets the migration
	/// run without a new ledger SystemTransaction).
	#[test]
	fn seed_pools_from_reserve_moves_amounts_and_conserves_supply() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let ledger = prepare_ledger();
		let (l0, r0, t0) = pools(&ledger.state);
		assert!(r0 > 0, "undeployed genesis should have a non-empty reserve to move from");

		let to_locked = r0 / 2;
		let to_treasury = r0 / 4;
		let moved = Ledger::<DefaultDB>::seed_pools_from_reserve(ledger, to_locked, to_treasury)
			.expect("reserve covers the move");
		let (l1, r1, t1) = pools(&moved.state);

		assert_eq!(l1, l0 + to_locked, "locked += to_locked");
		assert_eq!(t1, t0 + to_treasury, "treasury[NIGHT] += to_treasury");
		assert_eq!(r1, r0 - (to_locked + to_treasury), "reserve -= (to_locked + to_treasury)");
		assert_eq!(l0 + r0 + t0, l1 + r1 + t1, "three-pool NIGHT supply conserved");
	}

	/// The reserve guard mirrors the ledger's own `IllegalReserveDistribution`: a move larger than
	/// the reserve is rejected, never silently saturated.
	#[test]
	fn seed_pools_from_reserve_rejects_when_reserve_insufficient() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let ledger = prepare_ledger();
		let reserve = ledger.state.reserve_pool;
		let res =
			Ledger::<DefaultDB>::seed_pools_from_reserve(ledger, reserve.saturating_add(1), 0);
		assert!(
			matches!(res, Err(LedgerApiError::HostApiError)),
			"a move exceeding the reserve must be rejected"
		);
	}

	/// Headline equivalence test: on Preview's **real**, shipped genesis (`locked_pool == 0`,
	/// reserve 23e15 — the #1674 defect), moving the gap to the healthy-testnet targets reproduces
	/// the *exact* post-state recorded in `docs/upgrade-proof/after-pools.txt` — i.e. the identical
	/// database effect the removed `SystemTransaction::SeedPoolsFromReserve` produced, now achieved
	/// against the stock ledger.
	#[test]
	fn seed_pools_from_reserve_reaches_preview_target_from_zero_locked() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		const PREVIEW_GENESIS: &[u8] =
			include_bytes!("../../../../../res/genesis/genesis_state_preview.mn");
		// Targets must match `runtime/src/migrations.rs`.
		const TARGET_LOCKED: u128 = 16_799_999_999_126_012;
		const TARGET_TREASURY: u128 = 1_200_000_000_000_000;

		let state: LedgerState<DefaultDB> =
			tagged_deserialize(PREVIEW_GENESIS).expect("deserialize preview genesis");
		let ledger = Sp::new(Ledger::new(state));
		let (l0, r0, t0) = pools(&ledger.state);
		assert_eq!(l0, 0, "preview genesis ships with locked_pool == 0 (#1674)");

		// The migration moves only the gap to target.
		let to_locked = TARGET_LOCKED.saturating_sub(l0);
		let to_treasury = TARGET_TREASURY.saturating_sub(t0);
		let moved = Ledger::<DefaultDB>::seed_pools_from_reserve(ledger, to_locked, to_treasury)
			.expect("preview reserve covers the move");
		let (l1, r1, t1) = pools(&moved.state);

		assert_eq!(l1, TARGET_LOCKED, "locked reaches target");
		assert_eq!(t1, TARGET_TREASURY, "treasury[NIGHT] reaches target");
		assert_eq!(l0 + r0 + t0, l1 + r1 + t1, "supply conserved");
		// Exact post-state from docs/upgrade-proof/after-pools.txt.
		assert_eq!(
			(l1, r1, t1),
			(16_799_999_999_126_012, 5_000_000_000_873_988, 1_200_000_000_000_000),
			"exact reproduction of the SeedPoolsFromReserve post-state"
		);
	}
}
// grcov-excl-stop
