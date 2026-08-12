// This file is part of midnight-node.
// Copyright (C) 2025-2026 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! State translation from ledger v8 to ledger v9.
//!
//! Ported from `midnight-ledger` PR #539 (`v8-to-v9-state-translation`). The
//! only changes from the upstream crate are the import aliases below, which map
//! the translation's `ledger_v8` / `ledger_v9` / `onchain_state_v8` /
//! `onchain_state_v9` / `storage` / `serialize` crate names onto this
//! workspace's package aliases. The [`StateTranslationTable`] is consumed by the
//! v8->v9 storage migration ([`crate::host_api::migration_8_to_9`]).
//!
//! ## State shape differences (only stored types listed)
//!
//! | type                         | v8 tag                               | v9 tag                               | change |
//! | ---------------------------- | ------------------------------------ | ------------------------------------ | ------ |
//! | LedgerState                  | `ledger-state[v13]`                  | `ledger-state[v18]`                  | `bridge_receiving` map gains `NightAnn` |
//! | LedgerParameters             | `ledger-parameters[v5]`              | `ledger-parameters[v8]`              | adds `min_block_price`; `TransactionLimits` adds `max_contract_metadata_size`; `TransactionCostModel` drops `parallelism_factor`, adds `validation`/`guaranteed`/`fallible` factors |
//! | ContractState                | `contract-state[v6]`                 | `contract-state[v8]`                 | reflows `ContractOperation` + `ContractMaintenanceAuthority` changes |
//! | ContractOperation            | `contract-operation[v4]`             | `contract-operation[v6]`             | single `v2` key -> `{ v2, v3, ir }`; v8 key maps to `v2`, new `v3`/`ir` empty |
//! | ContractMaintenanceAuthority | `contract-maintenance-authority[v1]` | `contract-maintenance-authority[v2]` | `committee: Vec<VerifyingKey>` -> `Vec<ContractMaintenanceVerifyingKey>` (Schnorr/ECDSA sum) |
//!
//! Everything else (zswap, utxo, replay_protection, treasury,
//! unclaimed_block_rewards) is tag-stable and passes through `recast`. `dust`
//! is the exception: it is tag-stable but deliberately *wiped* rather than
//! carried over (see [`LedgerStateTl::finalize`]).

// Map the upstream translation crate names onto the node workspace's package
// aliases. `mn-ledger-8`/`mn-ledger-9` are the two `midnight-ledger` majors;
// `onchain-state-ledger-8`/`-9` are the exact `midnight-onchain-state` instances
// that each ledger's `ContractState` resolves to; `ledger-storage-ledger-8`
// (midnight-storage 2.0.1, `state-translation` feature) backs both.
use ledger_storage_ledger_8 as storage;
use midnight_serialize as serialize;
use mn_ledger_8 as ledger_v8;
use mn_ledger_9 as ledger_v9;
use onchain_state_ledger_8 as onchain_state_v8;
use onchain_state_ledger_9 as onchain_state_v9;

use base_crypto::cost_model::CostDuration;
use serialize::Tagged;
use std::ops::Deref;
use std::{any::Any, borrow::Cow, io, marker::PhantomData};
use storage::{
	Storable,
	arena::Sp,
	db::DB,
	merkle_patricia_trie::{self, Annotation, MerklePatriciaTrie},
	state_translation::*,
	storable::SizeAnn,
	storage::{HashMap, Map, default_storage},
};

// ---------- Generic helpers (copied from the v6->v7 reference) ----------

/// Recast a stored object from one type to another, requiring matching tags.
/// Used for subtrees whose tag is unchanged between v8 and v9.
fn recast<A: Storable<D> + Tagged, B: Storable<D> + Tagged, D: DB>(
	a: &Sp<A, D>,
) -> io::Result<Sp<B, D>> {
	if A::tag() != B::tag() {
		return io::Result::Err(io::Error::other("tags do not match"));
	}
	default_storage::<D>().get_lazy(&a.as_child().into())
}

/// Generic MPT translation: walks the trie, translating each entry via the
/// table-registered translation for `A->B`, and recomputes annotations under
/// `AnnB` from the new values.
struct MptTl<A, B, AnnA, AnnB>(PhantomData<(A, B, AnnA, AnnB)>);

impl<
	A: Storable<D> + Tagged,
	B: Storable<D> + Tagged,
	AnnA: Annotation<A> + Storable<D> + Tagged,
	AnnB: Annotation<B> + Storable<D> + Tagged,
	D: DB,
> DirectTranslation<MerklePatriciaTrie<A, D, AnnA>, MerklePatriciaTrie<B, D, AnnB>, D>
	for MptTl<A, B, AnnA, AnnB>
{
	fn required_translations() -> Vec<TranslationId> {
		vec![TranslationId(
			merkle_patricia_trie::Node::<A, D, AnnA>::tag(),
			merkle_patricia_trie::Node::<B, D, AnnB>::tag(),
		)]
	}
	fn child_translations(
		source: &MerklePatriciaTrie<A, D, AnnA>,
	) -> Vec<(TranslationId, Sp<dyn Any + Send + Sync, D>)> {
		let tlids = <Self as DirectTranslation<MerklePatriciaTrie<A, D, AnnA>, _, D>>::required_translations();
		vec![(tlids[0].clone(), source.0.upcast())]
	}
	fn finalize(
		source: &MerklePatriciaTrie<A, D, AnnA>,
		_limit: &mut CostDuration,
		cache: &TranslationCache<D>,
	) -> io::Result<Option<MerklePatriciaTrie<B, D, AnnB>>> {
		let tls = Self::child_translations(source);
		Ok(Some(MerklePatriciaTrie(try_resopt!(cache.resolve(&tls[0].0, tls[0].1.as_child())))))
	}
}

impl<
	A: Storable<D> + Tagged,
	B: Storable<D> + Tagged,
	AnnA: Storable<D> + Tagged + Annotation<A>,
	AnnB: Storable<D> + Tagged + Annotation<B>,
	D: DB,
>
	DirectTranslation<
		merkle_patricia_trie::Node<A, D, AnnA>,
		merkle_patricia_trie::Node<B, D, AnnB>,
		D,
	> for MptTl<A, B, AnnA, AnnB>
{
	fn required_translations() -> Vec<TranslationId> {
		let entry_tl = TranslationId(A::tag(), B::tag());
		let self_tl = TranslationId(
			merkle_patricia_trie::Node::<A, D, AnnA>::tag(),
			merkle_patricia_trie::Node::<B, D, AnnB>::tag(),
		);
		vec![entry_tl, self_tl]
	}
	fn child_translations(
		source: &merkle_patricia_trie::Node<A, D, AnnA>,
	) -> Vec<(TranslationId, Sp<dyn Any + Send + Sync, D>)> {
		let tls = <Self as DirectTranslation<merkle_patricia_trie::Node::<A, D, AnnA>, _, D>>::required_translations();
		let entry_tl = tls[0].clone();
		let self_tl = tls[1].clone();
		match source {
			merkle_patricia_trie::Node::Empty => vec![],
			merkle_patricia_trie::Node::Branch { children, .. } => {
				children.iter().map(|child| (self_tl.clone(), child.upcast())).collect()
			},
			merkle_patricia_trie::Node::Extension { child, .. } => {
				vec![(self_tl, child.upcast())]
			},
			merkle_patricia_trie::Node::MidBranchLeaf { value, child, .. } => {
				vec![(entry_tl, value.upcast()), (self_tl, child.upcast())]
			},
			merkle_patricia_trie::Node::Leaf { value, .. } => vec![(entry_tl, value.upcast())],
		}
	}
	fn finalize(
		source: &merkle_patricia_trie::Node<A, D, AnnA>,
		_limit: &mut CostDuration,
		cache: &TranslationCache<D>,
	) -> io::Result<Option<merkle_patricia_trie::Node<B, D, AnnB>>> {
		let tls = Self::child_translations(source);
		Ok(Some(match source {
			merkle_patricia_trie::Node::Empty => merkle_patricia_trie::Node::Empty,
			merkle_patricia_trie::Node::Branch { .. } => {
				let mut new_children =
					core::array::from_fn(|_| Sp::new(merkle_patricia_trie::Node::Empty));
				for (child, new_child) in tls.iter().zip(new_children.iter_mut()) {
					*new_child = try_resopt!(cache.resolve(&child.0, child.1.as_child()));
				}
				let ann = new_children.iter().fold(AnnB::empty(), |acc, x| {
					acc.append(&merkle_patricia_trie::Node::<B, D, AnnB>::ann(x))
				});
				merkle_patricia_trie::Node::Branch { ann, children: Box::new(new_children) }
			},
			merkle_patricia_trie::Node::Extension { compressed_path, .. } => {
				let child: Sp<merkle_patricia_trie::Node<B, D, AnnB>, D> =
					try_resopt!(cache.resolve(&tls[0].0, tls[0].1.as_child()));
				let ann = merkle_patricia_trie::Node::<B, D, AnnB>::ann(&child);
				merkle_patricia_trie::Node::Extension {
					ann,
					compressed_path: compressed_path.clone(),
					child,
				}
			},
			merkle_patricia_trie::Node::Leaf { .. } => {
				let value = try_resopt!(cache.resolve(&tls[0].0, tls[0].1.as_child()));
				let ann = AnnB::from_value(&value);
				merkle_patricia_trie::Node::Leaf { ann, value }
			},
			merkle_patricia_trie::Node::MidBranchLeaf { .. } => {
				let value = try_resopt!(cache.resolve(&tls[0].0, tls[0].1.as_child()));
				let child: Sp<merkle_patricia_trie::Node<B, D, AnnB>, D> =
					try_resopt!(cache.resolve(&tls[1].0, tls[1].1.as_child()));
				let ann = AnnB::from_value(&value)
					.append(&merkle_patricia_trie::Node::<B, D, AnnB>::ann(&child));
				merkle_patricia_trie::Node::MidBranchLeaf { ann, value, child }
			},
		}))
	}
}

/// Identity translation for a type whose serialization is unchanged across
/// versions. Needed when an MPT's entries are tag-stable but its annotation
/// changes (e.g. `bridge_receiving`).
struct IdentityTl<T>(PhantomData<T>);

impl<T: Storable<D> + Clone, D: DB> DirectTranslation<T, T, D> for IdentityTl<T> {
	fn required_translations() -> Vec<TranslationId> {
		Vec::new()
	}
	fn child_translations(_: &T) -> Vec<(TranslationId, Sp<dyn Any + Send + Sync, D>)> {
		Vec::new()
	}
	fn finalize(
		source: &T,
		_limit: &mut CostDuration,
		_cache: &TranslationCache<D>,
	) -> io::Result<Option<T>> {
		Ok(Some(source.clone()))
	}
}

// ---------- Translation IDs (shorthand) ----------

struct Ids;

impl Ids {
	fn contract_mpt<D: DB>() -> TranslationId {
		TranslationId(
			MerklePatriciaTrie::<
				onchain_state_v8::state::ContractState<D>,
				D,
				ledger_v8::annotation::NightAnn,
			>::tag(),
			MerklePatriciaTrie::<
				onchain_state_v9::state::ContractState<D>,
				D,
				ledger_v9::annotation::NightAnn,
			>::tag(),
		)
	}

	fn bridge_receiving_mpt<D: DB>() -> TranslationId {
		TranslationId(
			MerklePatriciaTrie::<u128, D, SizeAnn>::tag(),
			MerklePatriciaTrie::<u128, D, ledger_v9::annotation::NightAnn>::tag(),
		)
	}

	fn parameters() -> TranslationId {
		TranslationId(
			ledger_v8::structure::LedgerParameters::tag(),
			ledger_v9::structure::LedgerParameters::tag(),
		)
	}
}

// ---------- Top-level: LedgerState v8 -> v9 ----------

struct LedgerStateTl;

impl<D: DB>
	DirectTranslation<ledger_v8::structure::LedgerState<D>, ledger_v9::structure::LedgerState<D>, D>
	for LedgerStateTl
{
	fn required_translations() -> Vec<TranslationId> {
		vec![Ids::parameters(), Ids::bridge_receiving_mpt::<D>(), Ids::contract_mpt::<D>()]
	}

	fn child_translations(
		source: &ledger_v8::structure::LedgerState<D>,
	) -> Vec<(TranslationId, Sp<dyn Any + Send + Sync, D>)> {
		vec![
			(Ids::parameters(), source.parameters.upcast()),
			(Ids::bridge_receiving_mpt::<D>(), source.bridge_receiving.mpt.upcast()),
			(Ids::contract_mpt::<D>(), source.contract.mpt.upcast()),
		]
	}

	fn finalize(
		source: &ledger_v8::structure::LedgerState<D>,
		_limit: &mut CostDuration,
		cache: &TranslationCache<D>,
	) -> io::Result<Option<ledger_v9::structure::LedgerState<D>>> {
		let Some(parameters) = cache.lookup(&Ids::parameters(), source.parameters.as_child())
		else {
			return Ok(None);
		};
		let Some(bridge_recv_mpt) =
			cache.lookup(&Ids::bridge_receiving_mpt::<D>(), source.bridge_receiving.mpt.as_child())
		else {
			return Ok(None);
		};
		let Some(contract_mpt) =
			cache.lookup(&Ids::contract_mpt::<D>(), source.contract.mpt.as_child())
		else {
			return Ok(None);
		};

		Ok(Some(ledger_v9::structure::LedgerState {
			network_id: source.network_id.clone(),
			parameters: parameters.force_downcast(),
			locked_pool: source.locked_pool,
			bridge_receiving: Map { mpt: bridge_recv_mpt.force_downcast(), key_type: PhantomData },
			reserve_pool: source.reserve_pool,
			block_reward_pool: source.block_reward_pool,
			unclaimed_block_rewards: Map {
				mpt: recast(&source.unclaimed_block_rewards.mpt)?,
				key_type: PhantomData,
			},
			treasury: Map { mpt: recast(&source.treasury.mpt)?, key_type: PhantomData },
			zswap: recast(&source.zswap)?,
			contract: Map { mpt: contract_mpt.force_downcast(), key_type: PhantomData },
			utxo: recast(&source.utxo)?,
			replay_protection: recast(&source.replay_protection)?,
			// The hardfork wipes dust: the v8 dust state is dropped and replaced
			// with the same empty state genesis starts from. Dust generation for
			// still-locked cNIGHT is re-applied afterwards by
			// `pallet_cnight_observation::migrations::v2`; dust UTxOs (balances)
			// are not restored — they regenerate from the re-applied generation
			// entries.
			dust: Sp::new(ledger_v9::dust::DustState::default()),
		}))
	}
}

// ---------- LedgerParameters v8 -> v9 ----------

struct LedgerParametersTl;

impl<D: DB>
	DirectTranslation<
		ledger_v8::structure::LedgerParameters,
		ledger_v9::structure::LedgerParameters,
		D,
	> for LedgerParametersTl
{
	fn required_translations() -> Vec<TranslationId> {
		Vec::new()
	}
	fn child_translations(
		_: &ledger_v8::structure::LedgerParameters,
	) -> Vec<(TranslationId, Sp<dyn Any + Send + Sync, D>)> {
		Vec::new()
	}
	fn finalize(
		source: &ledger_v8::structure::LedgerParameters,
		_limit: &mut CostDuration,
		_cache: &TranslationCache<D>,
	) -> io::Result<Option<ledger_v9::structure::LedgerParameters>> {
		// Base-crypto-backed fields (Duration, FixedPoint, primitives) are
		// assignable directly because `midnight-base-crypto` is unified across
		// v8 and v9 by workspace patches. Composite types defined in `ledger`
		// (TransactionCostModel, dust parameters, etc.) are tag-stable but not
		// identical types, so we go through the (de)serializer.
		//
		// `TransactionLimits` is the exception: v9 bumped it to
		// `transaction-limits[v3]` by adding `max_contract_metadata_size`, so
		// it is no longer tag-stable and is rebuilt field-by-field (its other
		// fields are unified base-crypto types).
		Ok(Some(ledger_v9::structure::LedgerParameters {
			// `TransactionCostModel` bumped `transaction-cost-model[v4]`->`[v5]`:
			// v9 drops `parallelism_factor` and adds three `FixedPoint` factors.
			// The two surviving fields are tag-stable and recast through; the new
			// factors get the v9 INITIAL_PARAMETERS defaults.
			cost_model: ledger_v9::structure::TransactionCostModel {
				runtime_cost_model: recast_base(&source.cost_model.runtime_cost_model)?,
				baseline_cost: recast_base(&source.cost_model.baseline_cost)?,
				// NEW IN v9 — placeholder; the production value should match the
				// value chosen for the hardfork.
				validation_factor: ledger_v9::structure::INITIAL_PARAMETERS
					.cost_model
					.validation_factor,
				guaranteed_factor: ledger_v9::structure::INITIAL_PARAMETERS
					.cost_model
					.guaranteed_factor,
				fallible_factor: ledger_v9::structure::INITIAL_PARAMETERS
					.cost_model
					.fallible_factor,
			},
			limits: ledger_v9::structure::TransactionLimits {
				transaction_byte_limit: source.limits.transaction_byte_limit,
				time_to_dismiss_per_byte: source.limits.time_to_dismiss_per_byte,
				min_time_to_dismiss: source.limits.min_time_to_dismiss,
				block_limits: source.limits.block_limits,
				block_withdrawal_minimum_multiple: source.limits.block_withdrawal_minimum_multiple,
				// NEW IN v9 — placeholder; the production value should match
				// the value chosen for the hardfork.
				max_contract_metadata_size: ledger_v9::structure::INITIAL_PARAMETERS
					.limits
					.max_contract_metadata_size,
			},
			dust: recast_base(&source.dust)?,
			fee_prices: recast_base(&source.fee_prices)?,
			global_ttl: source.global_ttl,
			cost_dimension_min_ratio: source.cost_dimension_min_ratio,
			price_adjustment_a_parameter: source.price_adjustment_a_parameter,
			cardano_to_midnight_bridge_fee_basis_points: source
				.cardano_to_midnight_bridge_fee_basis_points,
			c_to_m_bridge_min_amount: source.c_to_m_bridge_min_amount,
			// NEW IN v9 — placeholder; the production value should match the
			// value chosen for the hardfork.
			min_block_price: ledger_v9::structure::INITIAL_PARAMETERS.min_block_price,
		}))
	}
}

/// Recast for tag-stable base types passed by value (cost model, limits, etc.).
/// Not the same as `recast` above which only works for `Sp`.
fn recast_base<A: Tagged + serialize::Serializable, B: Tagged + serialize::Deserializable>(
	a: &A,
) -> io::Result<B> {
	if A::tag() != B::tag() {
		return Err(io::Error::other("tags do not match"));
	}
	let mut buf = Vec::new();
	a.serialize(&mut buf)?;
	B::deserialize(&mut &buf[..], 0)
}

// ---------- ContractOperation v8 -> v9 ----------

/// Translate a single contract operation. v9 grew `ContractOperation` from a
/// single `v2` verifier key (`contract-operation[v4]`) to `{ v2, v3, ir }`
/// (`contract-operation[v6]`). v8's only key is a zk-stdlib-v1 key
/// (`verifier-key[v6]`), which v9 keeps in its `v2` slot: that slot is backed by
/// the same `transient-crypto` 2.x crate (`transient_crypto_old`), so it is the
/// identical type and assigns directly. The new zk-stdlib-v2 `v3` key
/// (`verifier-key[v7]`, transient-crypto 3.x) and the `ir` slot have no v8
/// equivalent and stay empty — v9 keys are *not* synthesized from v8 keys.
/// (Note `ContractOperation::new(vk, ir)` sets `v3`, not `v2`, so the struct is
/// built field-wise here.)
fn translate_contract_operation(
	source: &onchain_state_v8::state::ContractOperation,
) -> onchain_state_v9::state::ContractOperation {
	// `ContractOperation` is `#[non_exhaustive]`; `new` seeds `v3`/`ir`, and the
	// v8 key goes into the `v2` slot field-wise.
	let mut op = onchain_state_v9::state::ContractOperation::new(None, None);
	op.v2 = source.v2.clone();
	op
}

// ---------- ContractState v8 -> v9 ----------

struct ContractStateTl;

impl<D: DB>
	DirectTranslation<
		onchain_state_v8::state::ContractState<D>,
		onchain_state_v9::state::ContractState<D>,
		D,
	> for ContractStateTl
{
	fn required_translations() -> Vec<TranslationId> {
		Vec::new()
	}
	fn child_translations(
		_: &onchain_state_v8::state::ContractState<D>,
	) -> Vec<(TranslationId, Sp<dyn Any + Send + Sync, D>)> {
		Vec::new()
	}
	fn finalize(
		source: &onchain_state_v8::state::ContractState<D>,
		_limit: &mut CostDuration,
		_cache: &TranslationCache<D>,
	) -> io::Result<Option<onchain_state_v9::state::ContractState<D>>> {
		// `operations` entries (ContractOperation) changed shape, so the map
		// is rebuilt entry-by-entry. The translation machinery can't walk these
		// base-storable leaves nested under a contract, but a contract's
		// operation set is small, so an in-place rebuild is fine. ChargedState
		// and the balance map (keyed u128) are tag-stable and recast through.
		let mut operations = HashMap::new();
		for entry in source.operations.iter() {
			let (key, op) = &*entry;
			let key_v9: onchain_state_v9::state::EntryPointBuf = key[..].into();
			operations = operations.insert(key_v9, translate_contract_operation(op));
		}
		let committee_v9 = source
			.maintenance_authority
			.committee
			.iter()
			.map(|vk| onchain_state_v9::state::ContractMaintenanceVerifyingKey::Schnorr(vk.clone()))
			.collect();
		let maintenance_authority = onchain_state_v9::state::ContractMaintenanceAuthority {
			committee: committee_v9,
			threshold: source.maintenance_authority.threshold,
			counter: source.maintenance_authority.counter,
		};
		Ok(Some(onchain_state_v9::state::ContractState::<D> {
			data: recast::<
				onchain_state_v8::state::ChargedState<D>,
				onchain_state_v9::state::ChargedState<D>,
				D,
			>(&Sp::new(source.data.clone()))?
			.deref()
			.clone(),
			operations,
			maintenance_authority,
			balance: HashMap(Map { mpt: recast(&source.balance.0.mpt)?, key_type: PhantomData }),
		}))
	}
}

// ---------- Translation table ----------

pub struct StateTranslationTable;

impl<D: DB> TranslationTable<D> for StateTranslationTable {
	const TABLE: &[(TranslationId, &dyn TypelessTranslation<D>)] = &[
		// Top-level
		(
			TranslationId(Cow::Borrowed("ledger-state[v13]"), Cow::Borrowed("ledger-state[v18]")),
			&DirectSpTranslation::<_, _, LedgerStateTl, _>(PhantomData),
		),
		// LedgerParameters
		(
			TranslationId(
				Cow::Borrowed("ledger-parameters[v5]"),
				Cow::Borrowed("ledger-parameters[v8]"),
			),
			&DirectSpTranslation::<_, _, LedgerParametersTl, _>(PhantomData),
		),
		// ContractState
		(
			TranslationId(Cow::Borrowed("contract-state[v6]"), Cow::Borrowed("contract-state[v8]")),
			&DirectSpTranslation::<_, _, ContractStateTl, _>(PhantomData),
		),
		// `contract` MPT in LedgerState — entries are ContractState
		(
			TranslationId(
				Cow::Borrowed("mpt(contract-state[v6],night-annotation)"),
				Cow::Borrowed("mpt(contract-state[v8],night-annotation)"),
			),
			&DirectSpTranslation::<
				MerklePatriciaTrie<
					onchain_state_v8::state::ContractState<D>,
					D,
					ledger_v8::annotation::NightAnn,
				>,
				MerklePatriciaTrie<
					onchain_state_v9::state::ContractState<D>,
					D,
					ledger_v9::annotation::NightAnn,
				>,
				MptTl<
					onchain_state_v8::state::ContractState<D>,
					onchain_state_v9::state::ContractState<D>,
					ledger_v8::annotation::NightAnn,
					ledger_v9::annotation::NightAnn,
				>,
				_,
			>(PhantomData),
		),
		(
			TranslationId(
				Cow::Borrowed("mpt-node(contract-state[v6],night-annotation)"),
				Cow::Borrowed("mpt-node(contract-state[v8],night-annotation)"),
			),
			&DirectSpTranslation::<
				merkle_patricia_trie::Node<
					onchain_state_v8::state::ContractState<D>,
					D,
					ledger_v8::annotation::NightAnn,
				>,
				merkle_patricia_trie::Node<
					onchain_state_v9::state::ContractState<D>,
					D,
					ledger_v9::annotation::NightAnn,
				>,
				MptTl<
					onchain_state_v8::state::ContractState<D>,
					onchain_state_v9::state::ContractState<D>,
					ledger_v8::annotation::NightAnn,
					ledger_v9::annotation::NightAnn,
				>,
				_,
			>(PhantomData),
		),
		// `bridge_receiving` MPT — entries unchanged (u128), annotation changes
		// from SizeAnn to NightAnn. Needs an identity entry translation and an
		// MptTl that re-annotates.
		(
			TranslationId(Cow::Borrowed("u128"), Cow::Borrowed("u128")),
			&DirectSpTranslation::<u128, u128, IdentityTl<u128>, _>(PhantomData),
		),
		(
			TranslationId(
				Cow::Borrowed("mpt(u128,size-annotation)"),
				Cow::Borrowed("mpt(u128,night-annotation)"),
			),
			&DirectSpTranslation::<
				MerklePatriciaTrie<u128, D, SizeAnn>,
				MerklePatriciaTrie<u128, D, ledger_v9::annotation::NightAnn>,
				MptTl<u128, u128, SizeAnn, ledger_v9::annotation::NightAnn>,
				_,
			>(PhantomData),
		),
		(
			TranslationId(
				Cow::Borrowed("mpt-node(u128,size-annotation)"),
				Cow::Borrowed("mpt-node(u128,night-annotation)"),
			),
			&DirectSpTranslation::<
				merkle_patricia_trie::Node<u128, D, SizeAnn>,
				merkle_patricia_trie::Node<u128, D, ledger_v9::annotation::NightAnn>,
				MptTl<u128, u128, SizeAnn, ledger_v9::annotation::NightAnn>,
				_,
			>(PhantomData),
		),
	];
}

#[cfg(test)]
mod tests {
	use super::*;
	use storage::db::InMemoryDB;

	fn translate_to_completion(
		v8: ledger_v8::structure::LedgerState<InMemoryDB>,
	) -> ledger_v9::structure::LedgerState<InMemoryDB> {
		let tl_state = TypedTranslationState::<
			ledger_v8::structure::LedgerState<InMemoryDB>,
			ledger_v9::structure::LedgerState<InMemoryDB>,
			StateTranslationTable,
			InMemoryDB,
		>::start(Sp::new(v8))
		.expect("Failed to start translation");

		let cost = CostDuration::from_picoseconds(1_000_000_000_000);
		let finished = tl_state.run(cost).expect("Translation failed");

		finished
			.result()
			.expect("Failed to get result")
			.expect("Translation did not complete")
			.deref()
			.clone()
	}

	/// Every `TranslationId` a table entry requires must itself be in the table,
	/// or translation errors at runtime the first time the entry is needed.
	#[test]
	fn table_is_closed() {
		<StateTranslationTable as TranslationTable<InMemoryDB>>::assert_closure();
	}

	/// The `TABLE` hardcodes tag string literals. If a tag on either the v8 or v9
	/// side drifts (e.g. an rc bump changes a `#[tag]`), the literal no longer
	/// matches what `T::tag()` produces and the migration silently mis-dispatches.
	/// Rebuild every expected ID from the node's actual crate types and compare.
	#[test]
	fn table_tags_match_types() {
		use storage::merkle_patricia_trie::{MerklePatriciaTrie, Node};
		use storage::storable::SizeAnn;

		type V8Ann = ledger_v8::annotation::NightAnn;
		type V9Ann = ledger_v9::annotation::NightAnn;
		type V8Contract = onchain_state_v8::state::ContractState<InMemoryDB>;
		type V9Contract = onchain_state_v9::state::ContractState<InMemoryDB>;

		let expected: Vec<(Cow<'static, str>, Cow<'static, str>)> = vec![
			(
				ledger_v8::structure::LedgerState::<InMemoryDB>::tag(),
				ledger_v9::structure::LedgerState::<InMemoryDB>::tag(),
			),
			(
				ledger_v8::structure::LedgerParameters::tag(),
				ledger_v9::structure::LedgerParameters::tag(),
			),
			(V8Contract::tag(), V9Contract::tag()),
			(
				MerklePatriciaTrie::<V8Contract, InMemoryDB, V8Ann>::tag(),
				MerklePatriciaTrie::<V9Contract, InMemoryDB, V9Ann>::tag(),
			),
			(
				Node::<V8Contract, InMemoryDB, V8Ann>::tag(),
				Node::<V9Contract, InMemoryDB, V9Ann>::tag(),
			),
			(u128::tag(), u128::tag()),
			(
				MerklePatriciaTrie::<u128, InMemoryDB, SizeAnn>::tag(),
				MerklePatriciaTrie::<u128, InMemoryDB, V9Ann>::tag(),
			),
			(Node::<u128, InMemoryDB, SizeAnn>::tag(), Node::<u128, InMemoryDB, V9Ann>::tag()),
		];

		let actual: Vec<_> = <StateTranslationTable as TranslationTable<InMemoryDB>>::TABLE
			.iter()
			.map(|(id, _)| (id.0.clone(), id.1.clone()))
			.collect();

		assert_eq!(actual, expected);
	}

	/// End-to-end smoke test: a default v8 `LedgerState` translates to v9,
	/// preserving the tag-stable pools and picking up the new v9 default
	/// `min_block_price`, and survives a v9 serialize round-trip.
	#[test]
	fn empty_state_translates_and_round_trips() {
		let v8 = ledger_v8::structure::LedgerState::<InMemoryDB>::new("test-network");
		let v9 = translate_to_completion(v8.clone());

		assert_eq!(v9.network_id, v8.network_id);
		assert_eq!(v9.reserve_pool, v8.reserve_pool);
		assert_eq!(v9.locked_pool, v8.locked_pool);
		assert_eq!(v9.block_reward_pool, v8.block_reward_pool);
		assert_eq!(
			v9.parameters.min_block_price,
			ledger_v9::structure::INITIAL_PARAMETERS.min_block_price,
		);

		let mut buf = Vec::new();
		serialize::tagged_serialize(&v9, &mut buf).expect("v9 serialize");
		let v9_rt: ledger_v9::structure::LedgerState<InMemoryDB> =
			serialize::tagged_deserialize(&mut &buf[..]).expect("v9 deserialize");
		assert_eq!(v9_rt.network_id, v9.network_id);
	}

	/// The translation wipes dust: whatever generation/utxo state v8 held, the
	/// v9 side comes out as the empty state genesis starts from.
	#[test]
	fn dust_state_is_wiped() {
		let mut v8 = ledger_v8::structure::LedgerState::<InMemoryDB>::new("test-network");
		let mut dust = (*v8.dust).clone();
		dust.generation.generating_tree_first_free = 7;
		dust.utxo.commitments_first_free = 3;
		v8.dust = Sp::new(dust);

		let v9 = translate_to_completion(v8);

		assert_eq!(*v9.dust, ledger_v9::dust::DustState::default());
	}
}
