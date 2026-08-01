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

//! Live 1:1 parity test between the db-sync and Blockfrost main chain follower backends.
//!
//! Requires live instances of both backends, synced to the same network:
//!
//! ```text
//! DB_SYNC_POSTGRES_CONNECTION_STRING=postgres://... \
//! BLOCKFROST_ENDPOINT=https://cardano-preview.blockfrost.io/api/v0 \
//! BLOCKFROST_PROJECT_ID=preview... \
//! cargo test -p midnight-node --test blockfrost_parity -- --ignored --nocapture
//! ```
//!
//! Network parameters default to the testnet values from `res/cfg/default.toml` and can
//! be overridden with `CARDANO_SECURITY_PARAMETER`, `CARDANO_ACTIVE_SLOTS_COEFF`,
//! `BLOCK_STABILITY_MARGIN`, `MC__SLOT_DURATION_MILLIS`, `MC__FIRST_EPOCH_TIMESTAMP_MILLIS`,
//! `MC__EPOCH_DURATION_MILLIS`, `MC__FIRST_EPOCH_NUMBER`, `MC__FIRST_SLOT_NUMBER`.
//!
//! Every domain below must be configured or the test fails, so that a pass means all six
//! data sources were compared. `PARITY_ALLOW_PARTIAL=1` accepts a run covering only some
//! of them. Inputs per domain:
//! - cNIGHT observation: `CNIGHT_MAPPING_VALIDATOR_ADDRESS`, `CNIGHT_AUTH_TOKEN_ASSET_NAME`,
//!   `CNIGHT_POLICY_ID` (hex), `CNIGHT_ASSET_NAME`
//! - candidates: `COMMITTEE_CANDIDATE_ADDRESS`, `PERMISSIONED_CANDIDATE_POLICY_ID` (hex)
//! - federated authority: `FEDAUTH_COUNCIL_ADDRESS`, `FEDAUTH_COUNCIL_POLICY_ID` (hex),
//!   `FEDAUTH_TECHNICAL_COMMITTEE_ADDRESS`, `FEDAUTH_TECHNICAL_COMMITTEE_POLICY_ID` (hex)
//! - bridge: `BRIDGE_TOKEN_POLICY_ID`, `BRIDGE_TOKEN_ASSET_NAME`,
//!   `ILLIQUID_CIRCULATION_SUPPLY_VALIDATOR_ADDRESS`, `RESERVE_VALIDATOR_ADDRESS`
//!   (the same env names `MainChainScripts::read_from_env` uses)
//!
//! `PARITY_WINDOW_BLOCKS` (default 1000) sets the look-back window for the cNIGHT and
//! bridge comparisons. All queries are anchored at `db-sync tip - (k + margin)` so both
//! backends read identical, immutable chain state.

use authority_selection_inherents::AuthoritySelectionDataSource;
use midnight_node::blockfrost::{
	BlockfrostAuthoritySelectionDataSource, BlockfrostBlockDataSource,
	BlockfrostCNightObservationDataSource, BlockfrostClient,
	BlockfrostFederatedAuthorityObservationDataSource, BlockfrostTokenBridgeDataSource,
};
use midnight_primitives::BridgeRecipient;
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition};
use midnight_primitives_federated_authority_observation::{
	AuthBodyConfig, FederatedAuthorityObservationConfig,
};
use midnight_primitives_mainchain_follower::{
	CandidatesDataSourceImpl, FederatedAuthorityObservationDataSource,
	FederatedAuthorityObservationDataSourceImpl, MidnightCNightObservationDataSource,
	MidnightCNightObservationDataSourceImpl,
};
use partner_chains_db_sync_data_sources::{
	BlockDataSourceImpl, DbSyncBlockDataSourceConfig, McHashDataSourceImpl,
	TokenBridgeDataSourceImpl,
};
use sidechain_domain::mainchain_epoch::{Duration, MainchainEpochConfig, Timestamp};
use sidechain_domain::*;
use sidechain_mc_hash::McHashDataSource;
use sp_partner_chains_bridge::{BridgeDataCheckpoint, MainChainScripts, TokenBridgeDataSource};
use std::future::Future;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Instant;

/// Transaction capacity used for the cNIGHT comparison: high enough not to truncate a
/// realistic window, low enough that `tx_capacity * 64` stays well inside `usize`.
const CAPACITY: usize = 100_000;

fn env(name: &str) -> Option<String> {
	std::env::var(name).ok().filter(|v| !v.is_empty())
}

/// For values where the empty string is meaningful — an asset name is empty for the
/// unnamed asset of a policy — so "set but empty" must not read as "unset".
fn env_allow_empty(name: &str) -> Option<String> {
	std::env::var(name).ok()
}

fn env_or<T: FromStr>(name: &str, default: T) -> T
where
	T::Err: std::fmt::Debug,
{
	env(name).map(|v| v.parse().expect(name)).unwrap_or(default)
}

fn policy_id(hex_str: &str) -> PolicyId {
	PolicyId(hex::decode(hex_str).expect("valid policy id hex").try_into().expect("28 bytes"))
}

/// Runs the same query against both backends, prints the timings, and returns both results.
async fn compare<T, FA, FB>(label: &str, db_sync: FA, blockfrost: FB) -> (T, T)
where
	FA: Future<Output = T>,
	FB: Future<Output = T>,
{
	let start = Instant::now();
	let db_result = db_sync.await;
	let db_elapsed = start.elapsed();
	let start = Instant::now();
	let bf_result = blockfrost.await;
	let bf_elapsed = start.elapsed();
	eprintln!(
		"{label}: db-sync {}ms, blockfrost {}ms",
		db_elapsed.as_millis(),
		bf_elapsed.as_millis()
	);
	(db_result, bf_result)
}

#[tokio::test]
#[ignore = "requires live db-sync and Blockfrost instances (see module docs)"]
async fn backends_return_identical_results() {
	let postgres_url =
		env("DB_SYNC_POSTGRES_CONNECTION_STRING").expect("DB_SYNC_POSTGRES_CONNECTION_STRING");
	let blockfrost_endpoint = env("BLOCKFROST_ENDPOINT").expect("BLOCKFROST_ENDPOINT");
	let blockfrost_project_id = env("BLOCKFROST_PROJECT_ID");

	let security_parameter: u32 = env_or("CARDANO_SECURITY_PARAMETER", 432);
	let active_slots_coeff: f64 = env_or("CARDANO_ACTIVE_SLOTS_COEFF", 0.05);
	let block_stability_margin: u32 = env_or("BLOCK_STABILITY_MARGIN", 10);
	let slot_duration_millis: u64 = env_or("MC__SLOT_DURATION_MILLIS", 1000);
	let window_blocks: u32 = env_or("PARITY_WINDOW_BLOCKS", 1000);

	let mc_epoch_config = MainchainEpochConfig {
		first_epoch_timestamp_millis: Timestamp::from_unix_millis(env_or(
			"MC__FIRST_EPOCH_TIMESTAMP_MILLIS",
			1666656000000,
		)),
		epoch_duration_millis: Duration::from_millis(env_or("MC__EPOCH_DURATION_MILLIS", 86400000)),
		first_epoch_number: env_or("MC__FIRST_EPOCH_NUMBER", 0),
		first_slot_number: env_or("MC__FIRST_SLOT_NUMBER", 0),
		slot_duration_millis: Duration::from_millis(slot_duration_millis),
	};

	// --- backends -------------------------------------------------------------------
	let pool = sqlx::postgres::PgPoolOptions::new()
		.max_connections(5)
		.connect(&postgres_url)
		.await
		.expect("db-sync connection");

	let db_block_source = Arc::new(BlockDataSourceImpl::from_config(
		pool.clone(),
		DbSyncBlockDataSourceConfig {
			cardano_security_parameter: security_parameter,
			cardano_active_slots_coeff: active_slots_coeff,
			block_stability_margin,
		},
		&mc_epoch_config,
	));
	let db_mc_hash = McHashDataSourceImpl::new(db_block_source.clone(), None);

	let client = Arc::new(
		BlockfrostClient::new(
			&blockfrost_endpoint,
			blockfrost_project_id.as_deref(),
			security_parameter,
		)
		.expect("blockfrost client"),
	);
	let bf_block_source = BlockfrostBlockDataSource::new(
		client.clone(),
		security_parameter,
		active_slots_coeff,
		block_stability_margin,
		slot_duration_millis,
	);

	// --- anchor ---------------------------------------------------------------------
	// A block deep enough below both backends' tips to be immutable and identical.
	let db_tip = db_block_source.get_latest_block_info().await.expect("db-sync tip");
	let anchor_height = db_tip.number.0.saturating_sub(security_parameter + block_stability_margin);
	// Resolve the anchor hash via Blockfrost (db-sync has no public by-number getter).
	let anchor_hash = client
		.block_hash_by_number(anchor_height)
		.await
		.expect("blockfrost anchor lookup")
		.expect("anchor block exists");
	let anchor = McHashDataSource::get_block_by_hash(&bf_block_source, anchor_hash)
		.await
		.expect("anchor block info")
		.expect("anchor block exists");
	eprintln!(
		"anchor: block {} ({}) at epoch {}",
		anchor.number.0,
		hex::encode(anchor.hash.0),
		anchor.epoch.0
	);

	// --- block source ---------------------------------------------------------------
	let (db, bf) = compare(
		"get_block_by_hash(anchor)",
		McHashDataSource::get_block_by_hash(&db_mc_hash, anchor.hash.clone()),
		McHashDataSource::get_block_by_hash(&bf_block_source, anchor.hash.clone()),
	)
	.await;
	assert_eq!(db.expect("db-sync"), bf.expect("blockfrost"), "get_block_by_hash mismatch");

	// Reference timestamp placed inside the Praos window relative to the anchor:
	// [anchor_time + k/f, anchor_time + 3k/f].
	let min_boundary_ms = (slot_duration_millis as f64 * security_parameter as f64
		/ active_slots_coeff)
		.round() as u64;
	let reference = sp_timestamp::Timestamp::new(anchor.timestamp * 1000 + 2 * min_boundary_ms);
	let (db, bf) = compare(
		"get_stable_block_for(anchor)",
		db_mc_hash.get_stable_block_for(anchor.hash.clone(), reference),
		bf_block_source.get_stable_block_for(anchor.hash.clone(), reference),
	)
	.await;
	assert_eq!(db.expect("db-sync"), bf.expect("blockfrost"), "get_stable_block_for mismatch");

	// Domains whose inputs were not supplied. Checked at the end: a pass has to mean
	// "every domain was compared", otherwise a run with only the two connection strings
	// set compares the block source alone and still reports success.
	let mut skipped: Vec<&str> = Vec::new();

	// --- cNIGHT observation ---------------------------------------------------------
	if let Some(mapping_validator_address) = env("CNIGHT_MAPPING_VALIDATOR_ADDRESS") {
		let config = CNightAddresses {
			mapping_validator_address,
			auth_token_asset_name: env_allow_empty("CNIGHT_AUTH_TOKEN_ASSET_NAME")
				.expect("CNIGHT_AUTH_TOKEN_ASSET_NAME"),
			cnight_policy_id: hex::decode(env("CNIGHT_POLICY_ID").expect("CNIGHT_POLICY_ID"))
				.expect("valid policy hex")
				.try_into()
				.expect("28 bytes"),
			cnight_asset_name: env_allow_empty("CNIGHT_ASSET_NAME").unwrap_or_default(),
		};
		let db_source = MidnightCNightObservationDataSourceImpl::new(pool.clone(), None, 1000);
		let bf_source = BlockfrostCNightObservationDataSource::new(
			client.clone(),
			security_parameter,
			midnight_primitives_mainchain_follower::data_source::DEFAULT_WINDOW_SIZE,
			None,
		);
		// Only `(block_number, tx_index_in_block)` of the start position act as range
		// bounds; hash and timestamp are placeholders on both backends alike.
		let start = CardanoPosition::min_for_block(anchor.number.0.saturating_sub(window_blocks));
		let (db, bf) = compare(
			"get_utxos_up_to_capacity",
			db_source.get_utxos_up_to_capacity(
				&config,
				&start,
				anchor.hash.clone(),
				// Effectively unbounded, but small enough that the helper's
				// `tx_capacity * 64` capacity hint cannot overflow.
				CAPACITY,
				10_000_000,
			),
			bf_source.get_utxos_up_to_capacity(
				&config,
				&start,
				anchor.hash.clone(),
				// Effectively unbounded, but small enough that the helper's
				// `tx_capacity * 64` capacity hint cannot overflow.
				CAPACITY,
				10_000_000,
			),
		)
		.await;
		let db = db.expect("db-sync");
		let bf = bf.expect("blockfrost");
		assert_eq!(db.start, bf.start, "cNIGHT start mismatch");
		assert_eq!(db.end, bf.end, "cNIGHT end mismatch");
		assert_eq!(db.utxos, bf.utxos, "cNIGHT events mismatch");
		eprintln!("cNIGHT events compared: {}", db.utxos.len());
	} else {
		skipped.push("cNIGHT (CNIGHT_MAPPING_VALIDATOR_ADDRESS)");
	}

	// --- authority selection ---------------------------------------------------------
	if let Some(committee_address) = env("COMMITTEE_CANDIDATE_ADDRESS") {
		let permissioned_policy = policy_id(
			&env("PERMISSIONED_CANDIDATE_POLICY_ID").expect("PERMISSIONED_CANDIDATE_POLICY_ID"),
		);
		let db_source = CandidatesDataSourceImpl::new(pool.clone(), None)
			.await
			.expect("candidates source");
		let bf_source =
			BlockfrostAuthoritySelectionDataSource::new(client.clone(), security_parameter, None);
		let epoch = McEpochNumber(anchor.epoch.0);
		let address = MainchainAddress::from_str(&committee_address).expect("valid address");

		let (db, bf) = compare(
			"get_epoch_nonce",
			db_source.get_epoch_nonce(epoch),
			bf_source.get_epoch_nonce(epoch),
		)
		.await;
		assert_eq!(db.expect("db-sync"), bf.expect("blockfrost"), "epoch nonce mismatch");

		let (db, bf) = compare(
			"get_ariadne_parameters",
			db_source.get_ariadne_parameters(
				epoch,
				permissioned_policy.clone(),
				permissioned_policy.clone(),
			),
			bf_source.get_ariadne_parameters(
				epoch,
				permissioned_policy.clone(),
				permissioned_policy,
			),
		)
		.await;
		assert_eq!(
			format!("{:?}", db.expect("db-sync")),
			format!("{:?}", bf.expect("blockfrost")),
			"ariadne parameters mismatch"
		);

		let (db, bf) = compare(
			"get_candidates",
			db_source.get_candidates(epoch, address.clone()),
			bf_source.get_candidates(epoch, address),
		)
		.await;
		let mut db = db.expect("db-sync");
		let mut bf = bf.expect("blockfrost");
		// Both backends group by stake pool key with unordered maps; sort for comparison.
		db.sort_by_key(|c| c.stake_pool_public_key.0);
		bf.sort_by_key(|c| c.stake_pool_public_key.0);
		assert_eq!(format!("{db:?}"), format!("{bf:?}"), "candidate registrations mismatch");
		eprintln!("candidates compared: {}", db.len());
	} else {
		skipped.push("candidates (COMMITTEE_CANDIDATE_ADDRESS)");
	}

	// --- federated authority ----------------------------------------------------------
	if let Some(council_address) = env("FEDAUTH_COUNCIL_ADDRESS") {
		let config = FederatedAuthorityObservationConfig {
			council: AuthBodyConfig {
				address: council_address,
				policy_id: policy_id(
					&env("FEDAUTH_COUNCIL_POLICY_ID").expect("FEDAUTH_COUNCIL_POLICY_ID"),
				),
				// Genesis-only fields; they don't affect the query.
				members: vec![],
				members_mainchain: vec![],
			},
			technical_committee: AuthBodyConfig {
				address: env("FEDAUTH_TECHNICAL_COMMITTEE_ADDRESS")
					.expect("FEDAUTH_TECHNICAL_COMMITTEE_ADDRESS"),
				policy_id: policy_id(
					&env("FEDAUTH_TECHNICAL_COMMITTEE_POLICY_ID")
						.expect("FEDAUTH_TECHNICAL_COMMITTEE_POLICY_ID"),
				),
				members: vec![],
				members_mainchain: vec![],
			},
		};
		let db_source = FederatedAuthorityObservationDataSourceImpl::new(pool.clone(), None, 1000);
		let bf_source =
			BlockfrostFederatedAuthorityObservationDataSource::new(client.clone(), None);
		let (db, bf) = compare(
			"get_federated_authority_data",
			db_source.get_federated_authority_data(&config, &anchor.hash),
			bf_source.get_federated_authority_data(&config, &anchor.hash),
		)
		.await;
		assert_eq!(
			format!("{:?}", db.expect("db-sync")),
			format!("{:?}", bf.expect("blockfrost")),
			"federated authority data mismatch"
		);
	} else {
		skipped.push("federated authority (FEDAUTH_COUNCIL_ADDRESS)");
	}

	// --- bridge ------------------------------------------------------------------------
	if env("ILLIQUID_CIRCULATION_SUPPLY_VALIDATOR_ADDRESS").is_some() {
		let scripts = MainChainScripts::read_from_env().expect("bridge script env vars");
		let db_source = TokenBridgeDataSourceImpl::new(pool.clone(), None);
		let bf_source = BlockfrostTokenBridgeDataSource::new(client.clone());
		let checkpoint = BridgeDataCheckpoint::Block(McBlockNumber(
			anchor.number.0.saturating_sub(window_blocks),
		));
		let (db, bf) = compare(
			"get_transfers",
			TokenBridgeDataSource::<BridgeRecipient>::get_transfers(
				&db_source,
				scripts.clone(),
				checkpoint.clone(),
				100,
				anchor.hash.clone(),
			),
			TokenBridgeDataSource::<BridgeRecipient>::get_transfers(
				&bf_source,
				scripts,
				checkpoint,
				100,
				anchor.hash.clone(),
			),
		)
		.await;
		let db = db.expect("db-sync");
		let bf = bf.expect("blockfrost");
		assert_eq!(db.0, bf.0, "bridge transfers mismatch");
		assert_eq!(db.1, bf.1, "bridge checkpoint mismatch");
		eprintln!("bridge transfers compared: {}", db.0.len());
	} else {
		skipped.push("bridge (ILLIQUID_CIRCULATION_SUPPLY_VALIDATOR_ADDRESS)");
	}

	// Fail rather than pass a partial comparison. `eprintln!` notes above are swallowed by
	// libtest on a passing test unless `--nocapture` is given, so a skip would otherwise be
	// invisible as well as silent.
	if !skipped.is_empty() && env("PARITY_ALLOW_PARTIAL").is_none() {
		panic!(
			"only the block data source was fully compared; these domains were not: {}. \
			 Supply their env vars (see the module docs) or set PARITY_ALLOW_PARTIAL=1 to \
			 accept a partial run.",
			skipped.join(", ")
		);
	}
}
