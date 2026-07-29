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

//! Frozen v1 cNIGHT observation derivation (`release/node-1.0.1`).
//!
//! This reproduces, byte-for-byte, the inherent that mainnet finalized under
//! node 1.0.x. It is selected for blocks whose parent runtime `spec_version`
//! predates the v2 activation; on import every node re-derives and compares the
//! inherent (`pallet_cnight_observation::check_inherent`), so any divergence
//! here rejects a historically-valid block.
//!
//! DO NOT "improve" anything in this file. Correctness is defined as *matches
//! what the chain finalized*, not *is right*. In particular the cursor rule
//! below carries a latent row-vs-transaction completeness bug; it is preserved
//! deliberately. The post-v1 fix and the bulk cache live in the v2 path.
//!
//! ## Faithfulness notes (the only places this is not a literal copy)
//!
//! 1. **Coarse id bounds.** Since 1.0.1, `get_low_bounds`/`get_high_bounds`
//!    gained `ORDER BY id` (1.0.1 had a bare `LIMIT 1`, i.e. an arbitrary row
//!    when a `block_no` is duplicated by a Cardano rollback orphan). These are
//!    *coarse* bounds — they constrain the id-space scan but must not clip real
//!    rows, and the ordered form is strictly more inclusive — so on orphan-free
//!    history the selected rows are identical. The v1-replay test over real
//!    finalized blocks is the arbiter.
//! 2. **Asset-quantity decode.** The shared category helpers now route
//!    `quantity` through `checked_asset_quantity` (panics on a negative value
//!    instead of sign-extending). cNIGHT quantities are non-negative on chain,
//!    so the decoded value is identical; only a corrupt/forged negative differs
//!    (v1 would mint garbage, the helper halts — strictly safer, never on real
//!    history).
//!
//! For production this module should eventually be *hermetically sealed* (its
//! own copies of the four category queries and their row decoding) so a future
//! edit to the shared helpers can never silently re-define v1. It currently
//! reuses them for review brevity; the two notes above bound exactly how that
//! reuse could matter.

use crate::ObservedUtxo;
use crate::data_source::cnight_observation::{
	MidnightCNightObservationDataSourceError, MidnightCNightObservationDataSourceImpl,
};
use crate::db::PagedQuery;
use cardano_serialization_lib::{Address, EnterpriseAddress};
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition, ObservedUtxos};
use sidechain_domain::McBlockHash;
use sqlx::PgPool;

/// v1 over-fetch factor: the inherent budget is `tx_capacity` whole
/// *transactions*, but the SQL works in *UTXOs*, so v1 over-estimates 64 UTXOs
/// per tx and uses that as the per-category row `LIMIT`. Frozen at 64 — it is
/// part of the consensus output, not a tunable.
const V1_UTXO_PER_TX_OVERESTIMATE: usize = 64;

/// The exact `release/node-1.0.1`
/// `MidnightCNightObservationDataSourceImpl::get_utxos_up_to_capacity`, lifted
/// out as a free function so the v2 path can sit beside it and a `spec_version`
/// gate can pick between them.
///
/// `tx_capacity` is the on-chain `CardanoTxCapacityPerBlock` at the parent. v1
/// has no separate `max_utxos`: its row budget is `tx_capacity * 64`.
pub async fn derive_inherent_v1(
	pool: &PgPool,
	config: &CNightAddresses,
	start_position: &CardanoPosition,
	current_tip: McBlockHash,
	tx_capacity: usize,
) -> Result<ObservedUtxos, Box<dyn std::error::Error + Send + Sync>> {
	let data_source = MidnightCNightObservationDataSourceImpl::new(pool.clone(), None, 0);

	// --- address / network / asset idents (verbatim from 1.0.1) ---
	let mapping_validator_address = Address::from_bech32(&config.mapping_validator_address)
		.map_err(|e| {
			MidnightCNightObservationDataSourceError::MappingValidatorInvalidAddress(e.to_string())
		})?;
	let cardano_network = mapping_validator_address.network_id().map_err(|_| {
		MidnightCNightObservationDataSourceError::CardanoNetworkError(
			config.mapping_validator_address.clone(),
		)
	})?;
	let mapping_validator_policy_id = EnterpriseAddress::from_address(&mapping_validator_address)
		.ok_or(MidnightCNightObservationDataSourceError::MappingValidatorInvalidAddress(
			"Not EnterpriseAddress".to_string(),
		))?
		.payment_cred()
		.to_scripthash()
		.ok_or(MidnightCNightObservationDataSourceError::MappingValidatorInvalidAddress(
			"MappingValidator address does not contain a script hash".to_string(),
		))?;

	let auth_token_ident = crate::db::resolve_multi_asset_id(
		pool,
		&mapping_validator_policy_id.to_bytes(),
		config.auth_token_asset_name.as_bytes(),
	)
	.await?;
	let cnight_ident = crate::db::resolve_multi_asset_id(
		pool,
		&config.cnight_policy_id,
		config.cnight_asset_name.as_bytes(),
	)
	.await?;

	// --- end position = tip block, then +1 tx index (verbatim) ---
	let end: CardanoPosition = crate::db::get_block_by_hash(pool, current_tip.clone())
		.await?
		.ok_or(MidnightCNightObservationDataSourceError::MissingBlockReference(current_tip))?
		.into();
	// Bounds use the un-incremented block_number (increment only bumps tx_index,
	// so block_number is unchanged either way — kept in 1.0.1's order regardless).
	let (low_bounds, high_bounds) = tokio::try_join!(
		crate::db::get_low_bounds(pool, start_position.block_number.into()),
		crate::db::get_high_bounds(pool, end.block_number.into()),
	)?;
	let low_bounds =
		low_bounds.expect("Start position contains block hash that exists in database");
	let high_bounds =
		high_bounds.expect("End position contains block hash that exists in database");
	let end = end.increment();

	// v1 row budget: tx_capacity TRANSACTIONS over-estimated at 64 UTXOs each.
	let utxo_capacity = tx_capacity * V1_UTXO_PER_TX_OVERESTIMATE;
	let paged = PagedQuery {
		start: start_position,
		end: &end,
		limit: utxo_capacity,
		offset: 0,
		low_bound: low_bounds,
		high_bound: high_bounds,
	};

	// Four category queries, concatenated in this fixed order, then sorted. The
	// order before the sort is irrelevant to the result (total `Ord`), but kept
	// as 1.0.1 had it: reg, dereg, create, spend.
	let mut utxos: Vec<ObservedUtxo> = Vec::new();
	if let Some(ident) = auth_token_ident {
		utxos.extend(
			data_source
				.get_registration_utxos(
					cardano_network,
					ident,
					&config.mapping_validator_address,
					&paged,
				)
				.await?,
		);
	}
	utxos.extend(
		data_source
			.get_deregistration_utxos(cardano_network, &config.mapping_validator_address, &paged)
			.await?,
	);
	if let Some(ident) = cnight_ident {
		utxos.extend(data_source.get_asset_create_utxos(cardano_network, ident, &paged).await?);
		utxos.extend(data_source.get_asset_spend_utxos(cardano_network, ident, &paged).await?);
	}

	utxos.sort();
	Ok(truncate_v1(utxos, tx_capacity, start_position, end))
}

/// v1 whole-transaction truncation and cursor rule, factored out of
/// [`derive_inherent_v1`] so the frozen consensus semantics can be unit-tested
/// without a database.
///
/// `sorted_events` must already be sorted (v1 sorts the concatenated category
/// results first). `tip_end` is the incremented tip position used as the cursor
/// when the fetched range did not fill `tx_capacity` transactions — the spot
/// where v1's latent skip bug lives.
///
/// Frozen behaviour, preserved verbatim:
/// - admits whole txs, stopping the instant the `tx_capacity`-th distinct tx
///   begins, so at most `tx_capacity - 1` whole txs are kept (the off-by-one);
/// - if fewer than `tx_capacity` distinct txs were seen, the cursor jumps to
///   `tip_end` — even when an upstream row limit truncated the fetch, silently
///   skipping the unfetched rows (the bug). Do not "fix" either here; the v2
///   path does, behind the `spec_version` gate.
fn truncate_v1(
	sorted_events: Vec<ObservedUtxo>,
	tx_capacity: usize,
	start_position: &CardanoPosition,
	tip_end: CardanoPosition,
) -> ObservedUtxos {
	let mut truncated_utxos = Vec::with_capacity(sorted_events.len());
	let mut num_txs = 0;
	let mut cur_tx: Option<CardanoPosition> = None;
	for utxo in sorted_events {
		if cur_tx.as_ref().is_none_or(|tx| tx < &utxo.header.tx_position) {
			num_txs += 1;
			cur_tx = Some(utxo.header.tx_position.clone());
		}
		if num_txs == tx_capacity {
			break;
		}
		truncated_utxos.push(utxo);
	}

	if num_txs < tx_capacity {
		ObservedUtxos { start: start_position.clone(), end: tip_end, utxos: truncated_utxos }
	} else {
		ObservedUtxos {
			start: start_position.clone(),
			end: truncated_utxos
				.last()
				.map_or(start_position.clone(), |u| u.header.tx_position.clone())
				.increment(),
			utxos: truncated_utxos,
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{ObservedUtxoData, ObservedUtxoHeader, RegistrationData, UtxoIndexInTx};
	use midnight_primitives_cnight_observation::CardanoRewardAddressBytes;
	use sidechain_domain::{McBlockHash, McTxHash};

	/// One UTXO at `(block, tx_index)`. Only the `tx_position` drives truncation;
	/// the payload is filler.
	fn utxo(block: u32, tx_index: u32) -> ObservedUtxo {
		ObservedUtxo {
			header: ObservedUtxoHeader {
				tx_position: pos(block, tx_index),
				tx_hash: McTxHash([0u8; 32]),
				utxo_tx_hash: McTxHash([0u8; 32]),
				utxo_index: UtxoIndexInTx(0),
			},
			data: ObservedUtxoData::Registration(RegistrationData {
				cardano_reward_address: CardanoRewardAddressBytes([0u8; 29]),
				dust_public_key: vec![0u8; 33].try_into().unwrap(),
			}),
		}
	}

	fn pos(block: u32, tx_index: u32) -> CardanoPosition {
		CardanoPosition {
			block_hash: McBlockHash([0u8; 32]),
			block_number: block,
			block_timestamp: Default::default(),
			tx_index_in_block: tx_index,
		}
	}

	/// `n` distinct txs (one per block here), `utxos_per_tx` UTXOs each, sorted.
	fn txs(n: u32, utxos_per_tx: u32) -> Vec<ObservedUtxo> {
		let mut v: Vec<ObservedUtxo> =
			(0..n).flat_map(|t| (0..utxos_per_tx).map(move |_| utxo(t, 0))).collect();
		v.sort();
		v
	}

	/// FROZEN off-by-one: with `tx_capacity` distinct txs available, v1 admits
	/// `tx_capacity - 1` whole txs and parks the cursor just past the last one.
	#[test]
	fn admits_tx_capacity_minus_one_whole_txs() {
		// 5 txs (2 UTXOs each), capacity 3 → admit txs 0 and 1 = 4 UTXOs.
		let got = truncate_v1(txs(5, 2), 3, &pos(0, 0), pos(100, 0));
		assert_eq!(got.utxos.len(), 4, "must keep exactly tx_capacity-1 whole txs");
		let distinct: std::collections::BTreeSet<u32> =
			got.utxos.iter().map(|u| u.header.tx_position.block_number).collect();
		assert_eq!(distinct.len(), 2);
		// Filled (num_txs reached tx_capacity) → cursor = last admitted tx + 1,
		// NOT the tip.
		assert_eq!(got.end, pos(1, 0).increment());
		assert_ne!(got.end, pos(100, 0));
	}

	/// FROZEN BUG: when the fetched events form fewer than `tx_capacity` txs, the
	/// cursor jumps to the tip — even though only a few txs were consumed. This
	/// is the row-vs-tx skip; it MUST stay until the v2 gate flips.
	#[test]
	fn underfilled_jumps_cursor_to_tip() {
		// 2 txs available, capacity 5 → all admitted, cursor = tip (the bug).
		let got = truncate_v1(txs(2, 3), 5, &pos(0, 0), pos(100, 0));
		assert_eq!(got.utxos.len(), 6, "all underfilled UTXOs admitted");
		assert_eq!(got.end, pos(100, 0), "FROZEN: cursor jumps to tip when underfilled");
	}

	/// A single tx below capacity is admitted whole; still underfilled → tip.
	#[test]
	fn single_tx_admitted_whole_then_tip() {
		let got = truncate_v1(txs(1, 4), 5, &pos(0, 0), pos(100, 0));
		assert_eq!(got.utxos.len(), 4);
		assert_eq!(got.end, pos(100, 0));
	}

	/// Exactly `tx_capacity` distinct txs present: still only `tx_capacity - 1`
	/// kept, cursor past the last kept tx (boundary of the off-by-one).
	#[test]
	fn exactly_capacity_txs_keeps_capacity_minus_one() {
		let got = truncate_v1(txs(3, 1), 3, &pos(0, 0), pos(100, 0));
		assert_eq!(got.utxos.len(), 2);
		assert_eq!(got.end, pos(1, 0).increment());
	}
}
