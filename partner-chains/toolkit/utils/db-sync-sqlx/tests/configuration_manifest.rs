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

use db_sync_sqlx::{
	DbSyncAddressMode, DbSyncQueryConfig, DbSyncSchemaMode, DbSyncTxInputMode,
	ResolvedDbSyncAddressMode, ResolvedDbSyncQueryConfig, ResolvedDbSyncTxInputMode,
	candidate_index_specs,
};
use serde::{Deserialize, de::value::StrDeserializer};
use std::collections::BTreeSet;

fn deserialize<T>(value: &str) -> Result<T, serde::de::value::Error>
where
	T: for<'de> Deserialize<'de>,
{
	T::deserialize(StrDeserializer::new(value))
}

fn resolved(address_mode: ResolvedDbSyncAddressMode) -> ResolvedDbSyncQueryConfig {
	ResolvedDbSyncQueryConfig { tx_input_mode: ResolvedDbSyncTxInputMode::TxIn, address_mode }
}

#[test]
fn defaults_preserve_legacy_query_and_schema_behaviour() {
	assert_eq!(
		DbSyncQueryConfig::default(),
		DbSyncQueryConfig {
			tx_input_mode: DbSyncTxInputMode::Auto,
			address_mode: DbSyncAddressMode::Inline,
		}
	);
	assert_eq!(DbSyncSchemaMode::default(), DbSyncSchemaMode::Apply)
}

#[test]
fn documented_configuration_values_deserialize() {
	assert_eq!(deserialize::<DbSyncTxInputMode>("auto").unwrap(), DbSyncTxInputMode::Auto);
	assert_eq!(deserialize::<DbSyncTxInputMode>("tx_in").unwrap(), DbSyncTxInputMode::TxIn);
	assert_eq!(deserialize::<DbSyncTxInputMode>("consumed").unwrap(), DbSyncTxInputMode::Consumed);

	assert_eq!(deserialize::<DbSyncAddressMode>("inline").unwrap(), DbSyncAddressMode::Inline);
	assert_eq!(
		deserialize::<DbSyncAddressMode>("address_table").unwrap(),
		DbSyncAddressMode::AddressTable
	);

	assert_eq!(deserialize::<DbSyncSchemaMode>("apply").unwrap(), DbSyncSchemaMode::Apply);
	assert_eq!(deserialize::<DbSyncSchemaMode>("verify").unwrap(), DbSyncSchemaMode::Verify);
	assert_eq!(deserialize::<DbSyncSchemaMode>("skip").unwrap(), DbSyncSchemaMode::Skip);

	assert!(deserialize::<DbSyncTxInputMode>("tx-in").is_err());
	assert!(deserialize::<DbSyncAddressMode>("normalized").is_err());
	assert!(deserialize::<DbSyncSchemaMode>("disabled").is_err())
}

#[test]
fn inline_manifest_targets_only_inline_address_storage() {
	let indexes = candidate_index_specs(resolved(ResolvedDbSyncAddressMode::Inline));
	let names: BTreeSet<_> = indexes.iter().map(|index| index.name).collect();

	assert_eq!(names.len(), indexes.len(), "index names must be unique");
	assert!(names.contains("idx_ma_tx_out_ident"));
	assert!(names.contains("idx_ma_tx_out_id_ident"));
	assert!(names.contains("idx_tx_out_address"));
	assert!(!names.contains("idx_address_address"));
	assert!(!names.contains("idx_tx_out_address_id"));
	let by_output = indexes
		.iter()
		.find(|index| index.name == "idx_ma_tx_out_id_ident")
		.expect("ma_tx_out output lookup index is present");
	assert_eq!(by_output.keys, &["tx_out_id"]);

	let address = indexes
		.iter()
		.find(|index| index.name == "idx_tx_out_address")
		.expect("inline address index is present");
	assert_eq!(address.relation, "tx_out");
	assert_eq!(address.access_methods, &["hash", "btree"]);
	assert_eq!(address.keys, &["address"]);
}

#[test]
fn address_table_manifest_targets_only_normalized_address_storage() {
	let indexes = candidate_index_specs(resolved(ResolvedDbSyncAddressMode::AddressTable));
	let names: BTreeSet<_> = indexes.iter().map(|index| index.name).collect();

	assert_eq!(names.len(), indexes.len(), "index names must be unique");
	assert!(names.contains("idx_ma_tx_out_ident"));
	assert!(names.contains("idx_ma_tx_out_id_ident"));
	assert!(names.contains("idx_address_address"));
	assert!(names.contains("idx_tx_out_address_id"));
	assert!(!names.contains("idx_tx_out_address"));

	let address = indexes
		.iter()
		.find(|index| index.name == "idx_address_address")
		.expect("normalized address index is present");
	assert_eq!(address.relation, "address");
	assert_eq!(address.access_methods, &["hash", "btree"]);
	assert_eq!(address.keys, &["address"]);

	let foreign_key = indexes
		.iter()
		.find(|index| index.name == "idx_tx_out_address_id")
		.expect("normalized address foreign-key index is present");
	assert_eq!(foreign_key.relation, "tx_out");
	assert_eq!(foreign_key.access_methods, &["btree"]);
	assert_eq!(foreign_key.keys, &["address_id"]);
}

#[test]
fn input_manifest_tracks_the_selected_transaction_input_layout() {
	let tx_in = candidate_index_specs(resolved(ResolvedDbSyncAddressMode::Inline));
	let tx_in_names: BTreeSet<_> = tx_in.iter().map(|index| index.name).collect();
	assert!(tx_in_names.contains("idx_tx_in_tx_in_id"));
	assert!(tx_in_names.contains("idx_tx_in_tx_out_id_tx_out_index"));
	assert!(!tx_in_names.contains("idx_tx_out_consumed_by_tx_id"));

	let consumed = candidate_index_specs(ResolvedDbSyncQueryConfig {
		tx_input_mode: ResolvedDbSyncTxInputMode::Consumed,
		address_mode: ResolvedDbSyncAddressMode::Inline,
	});
	let consumed_names: BTreeSet<_> = consumed.iter().map(|index| index.name).collect();
	assert!(consumed_names.contains("idx_tx_out_consumed_by_tx_id"));
	assert!(!consumed_names.contains("idx_tx_in_tx_in_id"));
	assert!(!consumed_names.contains("idx_tx_in_tx_out_id_tx_out_index"))
}
