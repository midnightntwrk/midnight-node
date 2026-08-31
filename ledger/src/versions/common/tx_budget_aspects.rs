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

//! Splits a transaction's cost into the aspects that produced it.
//!
//! The ledger prices a transaction as `validation_cost + application_cost`, each
//! a single [`SyntheticCost`]. Those totals are exact but opaque: they do not say
//! whether a transaction is expensive because it carries eight Zswap proofs, or
//! because it writes a lot of UTXO state. This module rebuilds the per-item terms
//! of `Transaction::validation_cost` from the *public* cost model, so a load run
//! can be attributed rather than merely measured.
//!
//! Two properties keep the reconstruction honest:
//!
//! * The **totals are never reconstructed** — they come from the ledger's own
//!   `validation_cost` / `application_cost`. Only the split is ours.
//! * The compute-time scale factor the ledger applies to validation is
//!   *measured*, not read: the per-item terms are summed unscaled and compared
//!   against the ledger's total. This tracks the factor across ledger versions
//!   (ledger 8 has none; ledger 9 has `validation_factor`) without reaching for a
//!   private field.
//!
//! Whatever does not fit the reconstruction lands in `validation.other`. On a
//! healthy build that is a rounding error; a large residual means the ledger's
//! cost model has moved and the item names below need re-syncing. See
//! [`super::tx_budget`] for the emitted format.

#![cfg(feature = "std")]

use super::{
	base_crypto_local, ledger_storage_local, mn_ledger_local, transient_crypto_local, zswap_local,
};
use base_crypto_local::cost_model::{CostDuration, RunningCost, SyntheticCost};
use ledger_storage_local::db::DB;
use mn_ledger_local::{
	dust::DUST_SPEND_PIS,
	structure::{
		ContractAction, LedgerParameters, ProofMarker, SignatureKind,
		Transaction as LedgerTransaction, TransactionCostModel,
	},
};
use std::collections::BTreeSet;
use transient_crypto_local::commitment::PureGeneratorPedersen;
use zswap_local::{INPUT_PIS, OUTPUT_PIS};

use super::tx_budget::{Aspect, residual, sum};

/// The transaction shape the node applies: proof-carrying, Pedersen-bound.
type Tx<S, D> = LedgerTransaction<S, ProofMarker, PureGeneratorPedersen, D>;

/// Mirrors `midnight-ledger`'s `VERIFIER_KEY_SIZE` — the assumed serialized size
/// of a verifier key, used to price the read that precedes a contract call.
/// `pub(crate)` upstream, so it has to be restated here; a change upstream shows
/// up as a `validation.other` residual rather than as a silently wrong split.
const VERIFIER_KEY_SIZE: u64 = 2875;
/// Mirrors `midnight-ledger`'s `EXPECTED_CONTRACT_DEPTH`.
const EXPECTED_CONTRACT_DEPTH: usize = 32;
/// Mirrors `midnight-ledger`'s `EXPECTED_OPERATIONS_DEPTH`.
const EXPECTED_OPERATIONS_DEPTH: usize = 8;

/// Mirrors `TransactionCostModel::proof_verify` (`pub(crate)` upstream): a PLONK
/// verification costs a constant plus a per-public-input coefficient.
fn proof_verify(model: &TransactionCostModel, public_inputs: usize) -> RunningCost {
	RunningCost::compute(
		model.runtime_cost_model.proof_verify_constant
			+ model.runtime_cost_model.proof_verify_coeff_size * public_inputs,
	)
}

/// Mirrors the verifier-key read `validation_cost` charges once per distinct
/// contract entry point called.
fn verifier_key_read(model: &TransactionCostModel) -> RunningCost {
	model.runtime_cost_model.read_cell(VERIFIER_KEY_SIZE, true)
		+ model.runtime_cost_model.read_map(EXPECTED_CONTRACT_DEPTH, true)
		+ model.runtime_cost_model.read_map(EXPECTED_OPERATIONS_DEPTH, true)
}

/// A validation item before the ledger's compute-time scale factor is applied.
struct Item {
	name: &'static str,
	count: u64,
	cost: RunningCost,
}

impl Item {
	fn new(name: &'static str, count: u64, cost: RunningCost) -> Self {
		Self { name, count, cost }
	}
}

/// The per-item validation terms of a standard transaction, in the same order
/// `Transaction::validation_cost` accumulates them.
fn standard_items<S: SignatureKind<D>, D: DB>(
	tx: &Tx<S, D>,
	stx: &mn_ledger_local::structure::StandardTransaction<S, ProofMarker, PureGeneratorPedersen, D>,
	model: &TransactionCostModel,
) -> Vec<Item> {
	let runtime = &model.runtime_cost_model;

	// One verifier-key read per distinct (contract, entry point). Ledger 9 also
	// keys on the proof version; a transaction that calls one entry point at two
	// proof versions would therefore be undercounted by one read here, and the
	// difference falls into `validation.other`.
	let unique_calls = tx
		.calls()
		.map(|(_, call)| (call.address, call.entry_point))
		.collect::<BTreeSet<_>>()
		.len();

	let offers = stx
		.guaranteed_coins
		.iter()
		.map(|offer| (**offer).clone())
		.chain(stx.fallible_coins.values())
		.collect::<Vec<_>>();
	// A transient coin is both spent and created, so it is charged on both sides.
	let zswap_inputs: usize = offers.iter().map(|o| o.inputs.len() + o.transient.len()).sum();
	let zswap_outputs: usize = offers.iter().map(|o| o.outputs.len() + o.transient.len()).sum();
	let deltas: usize = offers.iter().map(|o| o.deltas.len()).sum();

	let mut intents = 0u64;
	let mut signatures = 0usize;
	let mut calls = 0u64;
	let mut call_proofs = RunningCost::ZERO;
	let mut dust_spends = 0usize;

	for intent in stx.intents.values() {
		intents += 1;
		signatures += intent
			.guaranteed_unshielded_offer
			.iter()
			.chain(intent.fallible_unshielded_offer.iter())
			.map(|offer| offer.signatures.len())
			.sum::<usize>();

		for action in intent.actions.iter() {
			match &*action {
				ContractAction::Call(call) => {
					calls += 1;
					call_proofs.compute_time += runtime.verifier_key_load;
					call_proofs +=
						proof_verify(model, call.public_inputs(Default::default()).len());
				},
				ContractAction::Maintain(update) => {
					signatures += update.signatures.len();
				},
				_ => {},
			}
		}

		if let Some(dust_actions) = intent.dust_actions {
			dust_spends += dust_actions.spends.len();
			signatures += dust_actions.registrations.len();
		}
	}

	// One Pedersen validity check per intent, plus the transaction-wide
	// `ec_mul` the ledger adds after the per-delta terms.
	let pedersen_binding = RunningCost::compute(runtime.pedersen_valid * intents + runtime.ec_mul);
	let pedersen_deltas = RunningCost::compute((runtime.hash_to_curve + runtime.ec_mul) * deltas);

	vec![
		Item::new("validation.baseline", 1, model.baseline_cost),
		Item::new(
			"validation.verifier_key_read",
			unique_calls as u64,
			verifier_key_read(model) * unique_calls,
		),
		Item::new(
			"validation.zswap_input_proof",
			zswap_inputs as u64,
			proof_verify(model, INPUT_PIS) * zswap_inputs,
		),
		Item::new(
			"validation.zswap_output_proof",
			zswap_outputs as u64,
			proof_verify(model, OUTPUT_PIS) * zswap_outputs,
		),
		Item::new("validation.contract_call_proof", calls, call_proofs),
		Item::new(
			"validation.dust_spend_proof",
			dust_spends as u64,
			proof_verify(model, DUST_SPEND_PIS) * dust_spends,
		),
		Item::new(
			"validation.signature_verify",
			signatures as u64,
			RunningCost::compute(runtime.signature_verify_constant * signatures),
		),
		Item::new("validation.pedersen_binding", intents, pedersen_binding),
		Item::new("validation.pedersen_delta", deltas as u64, pedersen_deltas),
	]
}

/// The compute-time factor the ledger applies to validation, recovered by
/// comparing its total against the unscaled item sum, and applied by exact
/// integer arithmetic that rounds *down*. Rounding down matters: it keeps the
/// scaled items from ever exceeding the ledger's total, so the leftover
/// `validation.other` stays non-negative and the aspects genuinely partition the
/// cost. Returns `None` when there is nothing to scale.
fn compute_scale(items: &[Item], validation_total: &SyntheticCost) -> Option<(u128, u128)> {
	let unscaled: CostDuration = items.iter().map(|item| item.cost.compute_time).sum();
	match unscaled.into_picoseconds() {
		0 => None,
		denominator => {
			Some((validation_total.compute_time.into_picoseconds() as u128, denominator as u128))
		},
	}
}

/// Itemises a transaction's cost.
///
/// Returns the exact total (the same figure `Transaction::cost` yields, and the
/// same one accrued into the block's fullness) together with the aspects that
/// make it up. The aspects sum to the total: anything the item reconstruction
/// misses is carried explicitly as `validation.other`.
pub fn aspects<S: SignatureKind<D>, D: DB>(
	tx: &Tx<S, D>,
	params: &LedgerParameters,
) -> (SyntheticCost, Vec<Aspect>) {
	let model = &params.cost_model;
	let validation_total = tx.validation_cost(model);
	let (apply_guaranteed, apply_total) = tx.application_cost(model);

	let items = match tx {
		LedgerTransaction::Standard(stx) => standard_items(tx, stx, model),
		// A rewards claim is a signature check on top of the baseline.
		_ => vec![
			Item::new("validation.baseline", 1, model.baseline_cost),
			Item::new(
				"validation.signature_verify",
				1,
				RunningCost::compute(model.runtime_cost_model.signature_verify_constant),
			),
		],
	};

	let scale = compute_scale(&items, &validation_total);
	let mut aspects: Vec<Aspect> = items
		.into_iter()
		.map(|item| {
			let mut cost = SyntheticCost::from(item.cost);
			if let Some((numerator, denominator)) = scale {
				let scaled = cost.compute_time.into_picoseconds() as u128 * numerator / denominator;
				cost.compute_time = CostDuration::from_picoseconds(scaled as u64);
			}
			Aspect::new(item.name, item.count, cost)
		})
		.collect();

	// `validation_cost` charges the serialized transaction size against the
	// block-usage dimension; it is the only dimension the items above never touch.
	aspects.push(Aspect::new(
		"validation.tx_size",
		validation_total.block_usage,
		SyntheticCost { block_usage: validation_total.block_usage, ..SyntheticCost::ZERO },
	));
	aspects.push(residual("validation.other", &validation_total, &sum(&aspects)));

	aspects.push(Aspect::new("apply.guaranteed", 1, apply_guaranteed));
	aspects.push(Aspect::new(
		"apply.fallible",
		1,
		super::tx_budget::saturating_sub(&apply_total, &apply_guaranteed),
	));

	(validation_total + apply_total, aspects)
}

// grcov-excl-start
#[cfg(test)]
mod tests {
	use super::super::super::{
		CRATE_NAME, TransactionSignature as Signature, helpers_local, midnight_serialize_local,
	};
	use super::*;
	use ledger_storage_local::DefaultDB;
	use midnight_node_res::{
		networks::{MidnightNetwork, UndeployedNetwork},
		undeployed::transactions::{CHECK_TX, DEPLOY_TX, MAINTENANCE_TX, STORE_TX},
	};
	use midnight_serialize_local::tagged_deserialize;
	use mn_ledger_local::structure::LedgerState;

	fn genesis_parameters() -> LedgerParameters {
		let state: LedgerState<DefaultDB> = tagged_deserialize(UndeployedNetwork.genesis_state())
			.expect("genesis ledger state should deserialize");
		(*state.parameters).clone()
	}

	fn decode(raw: &[u8]) -> Tx<Signature, DefaultDB> {
		let (bytes, _) = helpers_local::extract_tx_with_context(raw);
		tagged_deserialize(&bytes[..]).expect("test transaction should deserialize")
	}

	fn aspect<'a>(aspects: &'a [Aspect], name: &str) -> &'a Aspect {
		aspects.iter().find(|a| a.name == name).expect("aspect should be present")
	}

	/// The contract-lifecycle transactions shipped with the node. Ledger 9 pays
	/// fees in Dust rather than Zswap coins, so these exercise contract-call and
	/// Dust-spend proofs; `ZSWAP_TX` is deliberately not used here, as it is stale
	/// against the current genesis (see `pallets/midnight/src/tests.rs`).
	const TEST_TXS: [(&str, &[u8]); 4] = [
		("deploy", DEPLOY_TX),
		("store", STORE_TX),
		("check", CHECK_TX),
		("maintenance", MAINTENANCE_TX),
	];

	/// The bill has to add up: whatever the item reconstruction misses is carried
	/// by `validation.other`, so the aspects always sum to the ledger's own total.
	#[test]
	fn aspects_sum_to_the_ledger_total() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let params = genesis_parameters();

		for (label, raw) in TEST_TXS {
			let tx = decode(raw);
			let (total, aspects) = aspects(&tx, &params);
			assert_eq!(sum(&aspects), total, "{label}: aspects must partition the total");
		}
	}

	/// The reconstruction is only useful while it actually explains the cost. A
	/// residual above a rounding error means `midnight-ledger`'s validation cost
	/// model has changed and the items in this module need re-syncing.
	#[test]
	fn validation_residual_is_a_rounding_error() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let params = genesis_parameters();

		for (label, raw) in TEST_TXS {
			let tx = decode(raw);
			let validation = tx.validation_cost(&params.cost_model);
			let (_, aspects) = aspects(&tx, &params);
			let other = &aspect(&aspects, "validation.other").cost;

			for (dimension, unattributed, total) in [
				(
					"compute",
					other.compute_time.into_picoseconds(),
					validation.compute_time.into_picoseconds(),
				),
				(
					"read",
					other.read_time.into_picoseconds(),
					validation.read_time.into_picoseconds(),
				),
			] {
				assert!(
					unattributed * 1000 <= total,
					"{label}: unattributed validation {dimension} {unattributed} is more than \
					 0.1% of {total} — the ledger cost model has drifted from this module",
				);
			}
		}
	}

	/// The point of the split: proof verification is what a transaction mostly
	/// spends its compute budget on, and the calculator has to say so.
	#[test]
	fn proof_verification_is_attributed() {
		if CRATE_NAME != crate::latest::CRATE_NAME {
			println!("This test should only be run with ledger latest");
			return;
		}
		let params = genesis_parameters();

		for (label, raw) in TEST_TXS {
			let (_, aspects) = aspects(&decode(raw), &params);
			let proofs: u64 = aspects
				.iter()
				.filter(|a| a.name.ends_with("_proof"))
				.map(|a| a.cost.compute_time.into_picoseconds())
				.sum();
			let validation: u64 = aspects
				.iter()
				.filter(|a| a.name.starts_with("validation."))
				.map(|a| a.cost.compute_time.into_picoseconds())
				.sum();

			assert!(proofs > 0, "{label}: every transaction carries at least one proof");
			assert!(
				proofs * 2 > validation,
				"{label}: proof verification ({proofs}ps) should dominate validation compute \
				 ({validation}ps)",
			);
		}
	}
}
// grcov-excl-stop
