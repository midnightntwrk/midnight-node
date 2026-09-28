// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
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

use midnight_primitives_cnight_observation::{
	CardanoPosition, CardanoRewardAddressBytes, CreateData, DATA_ORDERED_UTXOS_SPEC_VERSION,
	DeregistrationData, DustPublicKeyBytes, ObservedUtxo, ObservedUtxoData, ObservedUtxoHeader,
	RegistrationData, SpendData, TimestampUnixMillis, UtxoIndexInTx, sort_observed_utxos,
};
use sidechain_domain::{McBlockHash, McTxHash};

const LEGACY_SPEC_VERSION: u32 = DATA_ORDERED_UTXOS_SPEC_VERSION - 1;

const THIS_TX: u8 = 0xaa;

fn position(tx_index_in_block: u32) -> CardanoPosition {
	CardanoPosition {
		block_hash: McBlockHash([1; 32]),
		block_number: 10,
		block_timestamp: TimestampUnixMillis(0),
		tx_index_in_block,
	}
}

fn header(tx: u8, utxo_tx: u8, utxo_index: u16, tx_index_in_block: u32) -> ObservedUtxoHeader {
	ObservedUtxoHeader {
		tx_position: position(tx_index_in_block),
		tx_hash: McTxHash([tx; 32]),
		utxo_tx_hash: McTxHash([utxo_tx; 32]),
		utxo_index: UtxoIndexInTx(utxo_index),
	}
}

fn address() -> CardanoRewardAddressBytes {
	CardanoRewardAddressBytes([9; 29])
}

fn dust_key() -> DustPublicKeyBytes {
	DustPublicKeyBytes::try_from(&[7u8; 33][..]).expect("33 bytes is a valid dust public key")
}

/// Registration output at `utxo_index` of transaction `tx`.
fn registration_at(tx: u8, utxo_index: u16, tx_index_in_block: u32) -> ObservedUtxo {
	ObservedUtxo {
		header: header(tx, tx, utxo_index, tx_index_in_block),
		data: ObservedUtxoData::Registration(RegistrationData {
			cardano_reward_address: address(),
			dust_public_key: dust_key(),
		}),
	}
}

/// Deregistration observed when `tx` spends the registration UTXO
/// `utxo_tx#utxo_index`, necessarily created by an earlier transaction.
fn deregistration_at(tx: u8, utxo_tx: u8, utxo_index: u16, tx_index_in_block: u32) -> ObservedUtxo {
	ObservedUtxo {
		header: header(tx, utxo_tx, utxo_index, tx_index_in_block),
		data: ObservedUtxoData::Deregistration(DeregistrationData {
			cardano_reward_address: address(),
			dust_public_key: dust_key(),
		}),
	}
}

/// cNIGHT output at `utxo_index` of transaction `tx`.
fn asset_create_at(tx: u8, utxo_index: u16, tx_index_in_block: u32) -> ObservedUtxo {
	ObservedUtxo {
		header: header(tx, tx, utxo_index, tx_index_in_block),
		data: ObservedUtxoData::AssetCreate(CreateData {
			value: 1,
			owner: address(),
			utxo_tx_hash: McTxHash([tx; 32]),
			utxo_tx_index: utxo_index,
		}),
	}
}

/// cNIGHT spend observed when `tx` consumes `utxo_tx#utxo_index`.
fn asset_spend_at(tx: u8, utxo_tx: u8, utxo_index: u16, tx_index_in_block: u32) -> ObservedUtxo {
	ObservedUtxo {
		header: header(tx, utxo_tx, utxo_index, tx_index_in_block),
		data: ObservedUtxoData::AssetSpend(SpendData {
			value: 1,
			owner: address(),
			utxo_tx_hash: McTxHash([utxo_tx; 32]),
			utxo_tx_index: utxo_index,
			spending_tx_hash: McTxHash([tx; 32]),
		}),
	}
}

fn registration(utxo_index: u16, tx_index_in_block: u32) -> ObservedUtxo {
	registration_at(THIS_TX, utxo_index, tx_index_in_block)
}

fn deregistration(utxo_tx: u8, tx_index_in_block: u32) -> ObservedUtxo {
	deregistration_at(THIS_TX, utxo_tx, 0, tx_index_in_block)
}

fn asset_create(utxo_index: u16, tx_index_in_block: u32) -> ObservedUtxo {
	asset_create_at(THIS_TX, utxo_index, tx_index_in_block)
}

fn asset_spend(utxo_tx: u8, tx_index_in_block: u32) -> ObservedUtxo {
	asset_spend_at(THIS_TX, utxo_tx, 0, tx_index_in_block)
}

fn variant_name(utxo: &ObservedUtxo) -> &'static str {
	match utxo.data {
		ObservedUtxoData::Registration(_) => "Registration",
		ObservedUtxoData::Deregistration(_) => "Deregistration",
		ObservedUtxoData::AssetCreate(_) => "AssetCreate",
		ObservedUtxoData::AssetSpend(_) => "AssetSpend",
	}
}

fn labels(utxos: &[ObservedUtxo]) -> Vec<&'static str> {
	utxos.iter().map(variant_name).collect()
}

/// Number of distinct input permutations the two multi-transaction tests below
/// sort from. The sort has to land on the same total order for every one of
/// them: the data source's concatenation order is an implementation detail, so
/// nothing downstream may depend on it.
const SHUFFLE_SEEDS: u64 = 16;

/// Deterministic Fisher-Yates shuffle, seeded so a failure is reproducible and
/// needs no RNG dependency. The generator is SplitMix64.
fn shuffle(utxos: &mut [ObservedUtxo], seed: u64) {
	let mut state = seed;
	let mut next = || {
		state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
		let mut z = state;
		z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
		z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
		z ^ (z >> 31)
	};

	for i in (1..utxos.len()).rev() {
		let j = (next() % (i as u64 + 1)) as usize;
		utxos.swap(i, j);
	}
}

/// `tx<position>:<variant>(<utxo the observation refers to>)`, enough to tell
/// two observations of the same variant within one transaction apart.
fn describe(utxos: &[ObservedUtxo]) -> Vec<String> {
	utxos
		.iter()
		.map(|utxo| {
			format!(
				"tx{}:{}(0x{:02x}#{})",
				utxo.header.tx_position.tx_index_in_block,
				variant_name(utxo),
				utxo.header.utxo_tx_hash.0[0],
				utxo.header.utxo_index.0,
			)
		})
		.collect()
}

/// One Cardano transaction rotating a mapping while also moving cNIGHT: it
/// spends the old registration UTXO (created by tx `0x11`) and a cNIGHT UTXO
/// (created by tx `0x22`), and creates a cNIGHT output at index 0 plus the new
/// registration output at index 1.
fn mixed_single_transaction() -> Vec<ObservedUtxo> {
	vec![registration(1, 3), deregistration(0x11, 3), asset_create(0, 3), asset_spend(0x22, 3)]
}

#[test]
fn legacy_ordering_puts_creates_before_spends() {
	let mut utxos = mixed_single_transaction();
	sort_observed_utxos(&mut utxos, LEGACY_SPEC_VERSION);

	// Creates first (ordered by output index, so the cNIGHT output precedes the
	// registration output), then spends ordered by the spent UTXO's tx hash.
	assert_eq!(labels(&utxos), vec!["AssetCreate", "Registration", "Deregistration", "AssetSpend"],);
}

#[test]
fn data_ordering_puts_mapping_changes_before_asset_events() {
	let mut utxos = mixed_single_transaction();
	sort_observed_utxos(&mut utxos, DATA_ORDERED_UTXOS_SPEC_VERSION);

	assert_eq!(labels(&utxos), vec!["Deregistration", "Registration", "AssetSpend", "AssetCreate"],);
}

#[test]
fn data_ordering_is_independent_of_input_order() {
	let mut forwards = mixed_single_transaction();
	let mut backwards = mixed_single_transaction();
	backwards.reverse();

	sort_observed_utxos(&mut forwards, DATA_ORDERED_UTXOS_SPEC_VERSION);
	sort_observed_utxos(&mut backwards, DATA_ORDERED_UTXOS_SPEC_VERSION);

	assert_eq!(labels(&forwards), labels(&backwards));
}

#[test]
fn data_ordering_keeps_tx_position_primary() {
	// A cNIGHT UTXO created in transaction 0 and spent in transaction 1 of the
	// same Cardano block. Both land in one inherent, and the create must stay
	// ahead of the spend even though `AssetSpend` outranks `AssetCreate` within
	// a transaction — the pallet's `UtxoOwners` entry, and the indexer's
	// `dust_generation_info` row, are written by the create and read by the
	// spend.
	let mut utxos = vec![asset_spend(THIS_TX, 1), asset_create(0, 0)];
	sort_observed_utxos(&mut utxos, DATA_ORDERED_UTXOS_SPEC_VERSION);

	assert_eq!(labels(&utxos), vec!["AssetCreate", "AssetSpend"]);
	assert_eq!(utxos[0].header.tx_position.tx_index_in_block, 0);
	assert_eq!(utxos[1].header.tx_position.tx_index_in_block, 1);
}

#[test]
fn ordering_rank_is_total_and_ascending() {
	let ranks = [
		ObservedUtxoData::Deregistration(DeregistrationData {
			cardano_reward_address: address(),
			dust_public_key: dust_key(),
		})
		.ordering_rank(),
		ObservedUtxoData::Registration(RegistrationData {
			cardano_reward_address: address(),
			dust_public_key: dust_key(),
		})
		.ordering_rank(),
		ObservedUtxoData::AssetSpend(SpendData {
			value: 0,
			owner: address(),
			utxo_tx_hash: McTxHash([0; 32]),
			utxo_tx_index: 0,
			spending_tx_hash: McTxHash([0; 32]),
		})
		.ordering_rank(),
		ObservedUtxoData::AssetCreate(CreateData {
			value: 0,
			owner: address(),
			utxo_tx_hash: McTxHash([0; 32]),
			utxo_tx_index: 0,
		})
		.ordering_rank(),
	];

	assert_eq!(ranks, [0, 1, 2, 3]);
}

// Two consecutive rotate-and-consolidate transactions in one Cardano block,
// the shape a holder produces when moving to a new DUST address while sweeping
// their cNIGHT into a single UTXO. `TX_B` consumes what `TX_A` paid out, so the
// pair also pins the cross-transaction dependency that `tx_position` protects.
const TX_A: u8 = 0xa1;
const TX_B: u8 = 0xb2;
const TX_A_POSITION: u32 = 1;
const TX_B_POSITION: u32 = 4;

/// The five observations one rotate-and-consolidate transaction produces: it
/// spends the previous registration UTXO and two cNIGHT UTXOs, and pays out an
/// updated registration at output 0 plus one consolidated cNIGHT UTXO at
/// output 1.
fn rotate_and_consolidate(
	tx: u8,
	tx_position: u32,
	spent_registration: (u8, u16),
	spent_cnight: [(u8, u16); 2],
) -> [ObservedUtxo; 5] {
	[
		registration_at(tx, 0, tx_position),
		deregistration_at(tx, spent_registration.0, spent_registration.1, tx_position),
		asset_create_at(tx, 1, tx_position),
		asset_spend_at(tx, spent_cnight[0].0, spent_cnight[0].1, tx_position),
		asset_spend_at(tx, spent_cnight[1].0, spent_cnight[1].1, tx_position),
	]
}

/// The ten observations the two transactions produce, grouped by variant the
/// way the data source concatenates the results of its four queries before
/// sorting them.
fn two_rotate_and_consolidate_transactions() -> Vec<ObservedUtxo> {
	// TX_A rotates off a registration created by tx 0x01 and sweeps cNIGHT
	// held by 0x02#0 and 0x03#1.
	let tx_a = rotate_and_consolidate(TX_A, TX_A_POSITION, (0x01, 0), [(0x02, 0), (0x03, 1)]);
	// TX_B rotates off the registration TX_A created and sweeps the cNIGHT
	// TX_A consolidated plus a second UTXO held by 0x04#0.
	let tx_b = rotate_and_consolidate(TX_B, TX_B_POSITION, (TX_A, 0), [(0x04, 0), (TX_A, 1)]);

	let utxos = vec![
		tx_a[0].clone(),
		tx_b[0].clone(), // registrations
		tx_a[1].clone(),
		tx_b[1].clone(), // deregistrations
		tx_a[2].clone(),
		tx_b[2].clone(), // asset creates
		tx_a[3].clone(),
		tx_a[4].clone(),
		tx_b[3].clone(),
		tx_b[4].clone(), // asset spends
	];
	assert_eq!(utxos.len(), 10);
	utxos
}

#[test]
fn legacy_ordering_of_two_rotate_and_consolidate_transactions() {
	// Within each transaction: both outputs first (by output index), then the
	// three spends ordered only by the spent UTXO's originating tx hash. That
	// last rule is why TX_B's deregistration lands between its two cNIGHT
	// spends — 0x04 sorts below 0xa1 — while TX_A's leads them.
	let expected = vec![
		"tx1:Registration(0xa1#0)",
		"tx1:AssetCreate(0xa1#1)",
		"tx1:Deregistration(0x01#0)",
		"tx1:AssetSpend(0x02#0)",
		"tx1:AssetSpend(0x03#1)",
		"tx4:Registration(0xb2#0)",
		"tx4:AssetCreate(0xb2#1)",
		"tx4:AssetSpend(0x04#0)",
		"tx4:Deregistration(0xa1#0)",
		"tx4:AssetSpend(0xa1#1)",
	];

	for seed in 0..SHUFFLE_SEEDS {
		let mut utxos = two_rotate_and_consolidate_transactions();
		shuffle(&mut utxos, seed);
		sort_observed_utxos(&mut utxos, LEGACY_SPEC_VERSION);

		assert_eq!(describe(&utxos), expected, "shuffle seed {seed}");
	}
}

#[test]
fn data_ordering_of_two_rotate_and_consolidate_transactions() {
	// Within each transaction: deregistration, registration, both cNIGHT
	// spends (by originating tx hash), then the consolidated cNIGHT create.
	// The mapping is therefore unambiguous by the time the create looks the
	// owner's registration up, in both transactions.
	let expected = vec![
		"tx1:Deregistration(0x01#0)",
		"tx1:Registration(0xa1#0)",
		"tx1:AssetSpend(0x02#0)",
		"tx1:AssetSpend(0x03#1)",
		"tx1:AssetCreate(0xa1#1)",
		"tx4:Deregistration(0xa1#0)",
		"tx4:Registration(0xb2#0)",
		"tx4:AssetSpend(0x04#0)",
		"tx4:AssetSpend(0xa1#1)",
		"tx4:AssetCreate(0xb2#1)",
	];

	for seed in 0..SHUFFLE_SEEDS {
		let mut utxos = two_rotate_and_consolidate_transactions();
		shuffle(&mut utxos, seed);
		sort_observed_utxos(&mut utxos, DATA_ORDERED_UTXOS_SPEC_VERSION);

		let described = describe(&utxos);
		assert_eq!(described, expected, "shuffle seed {seed}");

		// TX_A creates 0xa1#1 and TX_B spends it. Even though AssetSpend
		// outranks AssetCreate within a transaction, `tx_position` keeps the
		// create first.
		let position_of = |entry: &str| described.iter().position(|e| e == entry);
		assert!(
			position_of("tx1:AssetCreate(0xa1#1)").expect("TX_A's consolidated cNIGHT create")
				< position_of("tx4:AssetSpend(0xa1#1)").expect("TX_B's spend of it"),
			"shuffle seed {seed}"
		);
	}
}
