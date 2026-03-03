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

//! UTXO ordering overrides for blocks produced before the HashMap→BTreeMap fix.
//!
//! Old blocks on preview/preprod/qanet were produced with HashMap ordering in
//! `unshielded_utxos()`. The BTreeMap fix (commit 91015433) changed ordering for
//! transactions with multiple distinct intent_hashes. This module provides the
//! original ordering so syncing nodes can reproduce old state roots.

use std::{collections::HashMap, sync::OnceLock};

use crate::common::types::{Hash, UtxoInfo};

use super::LOG_TARGET;

static NETWORK_ID: OnceLock<String> = OnceLock::new();
static OVERRIDES: OnceLock<HashMap<Hash, UtxoOrdering>> = OnceLock::new();

#[derive(Debug, Clone)]
pub struct UtxoOrdering {
	pub created: Vec<(Hash, u32)>,
	pub spent: Vec<(Hash, u32)>,
}

#[derive(serde::Deserialize)]
struct UtxoEntry {
	intent_hash: String,
	output_index: u32,
}

#[derive(serde::Deserialize)]
struct TxOverride {
	tx_hash: String,
	#[allow(dead_code)]
	block_height: u64,
	created: Vec<UtxoEntry>,
	spent: Vec<UtxoEntry>,
}

pub fn set_network_id(network_id: &str) {
	let _ = NETWORK_ID.set(network_id.to_string());
}

fn parse_hash(hex_str: &str) -> Hash {
	let mut hash = [0u8; 32];
	if let Ok(bytes) = hex::decode(hex_str) {
		let len = bytes.len().min(32);
		hash[..len].copy_from_slice(&bytes[..len]);
	}
	hash
}

fn parse_entry(entry: &UtxoEntry) -> (Hash, u32) {
	(parse_hash(&entry.intent_hash), entry.output_index)
}

fn load_overrides() -> HashMap<Hash, UtxoOrdering> {
	let network_id = NETWORK_ID.get().map(|s| s.as_str()).unwrap_or("");
	let filename = match network_id {
		"preview" | "preprod" | "qanet" => format!("res/utxo-ordering-override-{network_id}.json"),
		_ => return HashMap::new(),
	};

	let json = match std::fs::read_to_string(&filename) {
		Ok(json) => json,
		Err(e) => {
			log::warn!(
				target: LOG_TARGET,
				"No UTXO ordering overrides for {network_id} (tried {filename}): {e}"
			);
			return HashMap::new();
		},
	};

	let entries: Vec<TxOverride> = match serde_json::from_str(&json) {
		Ok(entries) => entries,
		Err(e) => {
			log::error!(
				target: LOG_TARGET,
				"Failed to parse UTXO ordering overrides for {network_id}: {e}"
			);
			return HashMap::new();
		},
	};

	let mut map = HashMap::with_capacity(entries.len());
	for tx in entries {
		let tx_hash = parse_hash(&tx.tx_hash);
		let ordering = UtxoOrdering {
			created: tx.created.iter().map(parse_entry).collect(),
			spent: tx.spent.iter().map(parse_entry).collect(),
		};
		map.insert(tx_hash, ordering);
	}
	log::warn!(
		target: LOG_TARGET,
		"Loaded {} UTXO ordering overrides for {network_id}",
		map.len()
	);
	map
}

pub fn get_override(tx_hash: &Hash) -> Option<&'static UtxoOrdering> {
	OVERRIDES.get_or_init(load_overrides).get(tx_hash)
}

impl UtxoOrdering {
	pub fn apply(
		&self,
		outputs: &mut Vec<UtxoInfo>,
		output_segments: usize,
		inputs: &mut Vec<UtxoInfo>,
		input_segments: usize,
	) {
		log::warn!(
			target: LOG_TARGET,
			"UTXO ordering override: created={} (segments={}), spent={} (segments={})",
			self.created.len(),
			output_segments,
			self.spent.len(),
			input_segments,
		);
		if output_segments > 1 {
			reorder(outputs, &self.created);
		}
		if input_segments > 1 {
			reorder(inputs, &self.spent);
		}
	}
}

fn reorder(utxos: &mut Vec<UtxoInfo>, expected: &[(Hash, u32)]) {
	if expected.is_empty() || utxos.len() != expected.len() {
		return;
	}

	let mut reordered = Vec::with_capacity(utxos.len());
	for (intent_hash, output_no) in expected {
		if let Some(pos) = utxos
			.iter()
			.position(|u| u.intent_hash == *intent_hash && u.output_no == *output_no)
		{
			reordered.push(utxos.swap_remove(pos));
		} else {
			// Entry not found — override data may be stale, skip reordering
			log::warn!(
				target: LOG_TARGET,
				"UTXO ordering override: entry not found, skipping reorder"
			);
			return;
		}
	}
	*utxos = reordered;
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn reorder_matches_expected_sequence() {
		let make = |ih: u8, out: u32| UtxoInfo {
			address: [0u8; 32],
			token_type: [0u8; 32],
			intent_hash: {
				let mut h = [0u8; 32];
				h[0] = ih;
				h
			},
			value: 100,
			output_no: out,
		};

		let mut utxos = vec![make(1, 0), make(2, 0), make(3, 0)];
		let expected = vec![
			(
				{
					let mut h = [0u8; 32];
					h[0] = 3;
					h
				},
				0,
			),
			(
				{
					let mut h = [0u8; 32];
					h[0] = 1;
					h
				},
				0,
			),
			(
				{
					let mut h = [0u8; 32];
					h[0] = 2;
					h
				},
				0,
			),
		];

		reorder(&mut utxos, &expected);

		assert_eq!(utxos[0].intent_hash[0], 3);
		assert_eq!(utxos[1].intent_hash[0], 1);
		assert_eq!(utxos[2].intent_hash[0], 2);
	}

	#[test]
	fn reorder_noop_on_empty() {
		let mut utxos: Vec<UtxoInfo> = vec![];
		reorder(&mut utxos, &[]);
		assert!(utxos.is_empty());
	}

	#[test]
	fn reorder_noop_on_length_mismatch() {
		let make = |ih: u8| UtxoInfo {
			address: [0u8; 32],
			token_type: [0u8; 32],
			intent_hash: {
				let mut h = [0u8; 32];
				h[0] = ih;
				h
			},
			value: 100,
			output_no: 0,
		};
		let mut utxos = vec![make(1), make(2)];
		let expected = vec![(
			{
				let mut h = [0u8; 32];
				h[0] = 1;
				h
			},
			0,
		)];
		reorder(&mut utxos, &expected);
		// Should be unchanged
		assert_eq!(utxos[0].intent_hash[0], 1);
		assert_eq!(utxos[1].intent_hash[0], 2);
	}
}
