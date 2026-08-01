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

//! cNIGHT observation data source: extracts registration, deregistration and token
//! create/spend events from Cardano transactions, behind a sliding window cache.

use std::collections::HashMap;
use std::sync::Arc;

use blockfrost::{
	BlockCursor,
	blockfrost_openapi::models::{block_content::BlockContent, tx_content_utxo::TxContentUtxo},
};
use cardano_serialization_lib::{
	Address, BaseAddress, EnterpriseAddress, PlutusData, RewardAddress,
};
use midnight_primitives_cnight_observation as cnight;
use midnight_primitives_cnight_observation::{CNightAddresses, CardanoPosition};
use midnight_primitives_mainchain_follower::data_source::cnight_observation_bulk::truncate_to_tx_capacity;
use midnight_primitives_mainchain_follower::data_source::metrics::{
	MidnightDataSourceMetrics, start_sub_query_timer,
};
use midnight_primitives_mainchain_follower::{
	MidnightCNightObservationDataSource, MidnightCNightObservationDataSourceImpl,
};
use sidechain_domain::*;

use super::client::*;
use super::convert::*;
use super::support::*;

/// Blockfrost block times are unix seconds; [CardanoPosition] carries milliseconds.
fn cardano_position(b: &BlockContent) -> Result<CardanoPosition, BoxError> {
	Ok(CardanoPosition {
		block_hash: McBlockHash(decode_hash32(&b.hash)?),
		block_number: block_height(b)?,
		block_timestamp: cnight::TimestampUnixMillis(i64::from(b.time) * 1000),
		tx_index_in_block: u32::try_from(b.tx_count)?,
	})
}

// ---------------------------------------------------------------------------
// cNIGHT observation
// ---------------------------------------------------------------------------

/// Config-derived query inputs, mirroring the derivation in `bulk_pull`.
struct CNightQueryConfig {
	cardano_network: u8,
	mapping_validator_address: String,
	auth_unit: String,
	cnight_unit: String,
}

impl CNightQueryConfig {
	fn derive(config: &CNightAddresses) -> Result<Self, BoxError> {
		let mapping_validator_address = Address::from_bech32(&config.mapping_validator_address)
			.map_err(|e| format!("invalid mapping validator address: {e:?}"))?;
		let cardano_network = mapping_validator_address
			.network_id()
			.map_err(|e| format!("failed to extract network id: {e:?}"))?;
		let mapping_validator_policy_id =
			EnterpriseAddress::from_address(&mapping_validator_address)
				.ok_or("mapping validator address is not an EnterpriseAddress")?
				.payment_cred()
				.to_scripthash()
				.ok_or("mapping validator address has no script hash")?;
		Ok(Self {
			cardano_network,
			mapping_validator_address: config.mapping_validator_address.clone(),
			auth_unit: format!(
				"{}{}",
				hex::encode(mapping_validator_policy_id.to_bytes()),
				hex::encode(config.auth_token_asset_name.as_bytes())
			),
			cnight_unit: format!(
				"{}{}",
				hex::encode(config.cnight_policy_id),
				hex::encode(config.cnight_asset_name.as_bytes())
			),
		})
	}
}

/// Maps a bech32 holder address to the 29-byte reward address of its stake credential.
/// Non-base addresses (enterprise, pointer, reward) carry no stake credential and are
/// skipped, exactly like the db-sync implementation.
fn holder_reward_address(
	cardano_network: u8,
	bech32_address: &str,
) -> Option<cnight::CardanoRewardAddressBytes> {
	let Ok(address) = Address::from_bech32(bech32_address) else {
		log::debug!("Cardano address {bech32_address:?} not valid bech32 cardano address");
		return None;
	};
	let base_address = BaseAddress::from_address(&address)?;
	let reward_address = RewardAddress::new(cardano_network, &base_address.stake_cred());
	// Unwrap here is OK - we know the reward_address is always 29 bytes
	Some(cnight::CardanoRewardAddressBytes(
		reward_address.to_address().to_bytes().try_into().unwrap(),
	))
}

/// Decodes a registration datum into the reward-address + dust-key pair, reusing the
/// db-sync decoder. Returns `None` (with an error log) on malformed datums — such
/// events are skipped, not errors, to match db-sync behavior.
fn registration_pair(
	cardano_network: u8,
	datum: &PlutusData,
	context: &cnight::ObservedUtxoHeader,
) -> Option<(cnight::CardanoRewardAddressBytes, cnight::DustPublicKeyBytes)> {
	let Some(constr) = datum.as_constr_plutus_data() else {
		log::error!("Plutus data for mapping validator not Constr ({context:?})");
		return None;
	};
	let (credential, dust_public_key) =
		match MidnightCNightObservationDataSourceImpl::decode_registration_datum(constr) {
			Ok(pair) => pair,
			Err(e) => {
				log::error!("Failed to decode registration datum: {e:?} ({context:?})");
				return None;
			},
		};
	let reward_address = RewardAddress::new(cardano_network, &credential);
	// Unwrap here is OK - we know the reward_address is always 29 bytes
	let cardano_address = reward_address.to_address().to_bytes().try_into().unwrap();
	Some((cnight::CardanoRewardAddressBytes(cardano_address), dust_public_key))
}

/// All cNIGHT observation events of a single transaction, mirroring the four db-sync
/// range queries:
/// - Registration: output at the mapping validator address holding **exactly 1** auth
///   token, with a datum.
/// - Deregistration: spent input at the mapping validator address with a datum
///   (no token check — same as the SQL).
/// - AssetCreate / AssetSpend: outputs / spent inputs carrying the cNIGHT asset.
///
/// Collateral outputs and collateral/reference inputs are excluded: db-sync's
/// `tx_out`/`tx_in` tables only contain regular inputs and outputs.
async fn tx_cnight_events(
	client: &BlockfrostClient,
	cfg: &CNightQueryConfig,
	tx_position: &CardanoPosition,
	utxos: &TxContentUtxo,
) -> Result<Vec<cnight::ObservedUtxo>, BoxError> {
	let tx_hash = McTxHash(decode_hash32(&utxos.hash)?);
	let mut events = Vec::new();

	for output in utxos.outputs.iter().filter(|o| !o.collateral) {
		let utxo_index = cnight::UtxoIndexInTx(u16::try_from(output.output_index)?);
		let header = cnight::ObservedUtxoHeader {
			tx_position: tx_position.clone(),
			tx_hash,
			utxo_tx_hash: tx_hash,
			utxo_index,
		};
		if output.address == cfg.mapping_validator_address
			&& amount_of(&output.amount, &cfg.auth_unit)? == 1
			&& (output.inline_datum.is_some() || output.data_hash.is_some())
		{
			let datum =
				client.datum(output.inline_datum.as_ref(), output.data_hash.as_ref()).await?;
			if let Some(datum) = datum
				&& let Some((cardano_reward_address, dust_public_key)) =
					registration_pair(cfg.cardano_network, &datum, &header)
			{
				events.push(cnight::ObservedUtxo {
					header: header.clone(),
					data: cnight::ObservedUtxoData::Registration(cnight::RegistrationData {
						cardano_reward_address,
						dust_public_key,
					}),
				});
			}
		}
		let quantity = amount_of(&output.amount, &cfg.cnight_unit)?;
		if quantity > 0
			&& let Some(owner) = holder_reward_address(cfg.cardano_network, &output.address)
		{
			events.push(cnight::ObservedUtxo {
				header,
				data: cnight::ObservedUtxoData::AssetCreate(cnight::CreateData {
					value: quantity,
					owner,
					utxo_tx_hash: tx_hash,
					utxo_tx_index: utxo_index.0,
				}),
			});
		}
	}

	for input in utxos.inputs.iter().filter(|i| !i.collateral && !i.reference.unwrap_or(false)) {
		let utxo_tx_hash = McTxHash(decode_hash32(&input.tx_hash)?);
		let utxo_index = cnight::UtxoIndexInTx(u16::try_from(input.output_index)?);
		let header = cnight::ObservedUtxoHeader {
			tx_position: tx_position.clone(),
			tx_hash,
			utxo_tx_hash,
			utxo_index,
		};
		if input.address == cfg.mapping_validator_address
			&& (input.inline_datum.is_some() || input.data_hash.is_some())
		{
			let datum = client.datum(input.inline_datum.as_ref(), input.data_hash.as_ref()).await?;
			if let Some(datum) = datum
				&& let Some((cardano_reward_address, dust_public_key)) =
					registration_pair(cfg.cardano_network, &datum, &header)
			{
				events.push(cnight::ObservedUtxo {
					header: header.clone(),
					data: cnight::ObservedUtxoData::Deregistration(cnight::DeregistrationData {
						cardano_reward_address,
						dust_public_key,
					}),
				});
			}
		}
		let quantity = amount_of(&input.amount, &cfg.cnight_unit)?;
		if quantity > 0
			&& let Some(owner) = holder_reward_address(cfg.cardano_network, &input.address)
		{
			events.push(cnight::ObservedUtxo {
				header,
				data: cnight::ObservedUtxoData::AssetSpend(cnight::SpendData {
					value: quantity,
					owner,
					utxo_tx_hash,
					utxo_tx_index: utxo_index.0,
					spending_tx_hash: tx_hash,
				}),
			});
		}
	}

	Ok(events)
}

/// Contiguous range of already-fetched observation events.
///
/// Events are complete for `[covered_from, covered_to_block]` (inclusive;
/// `covered_to_block == None` means nothing fetched yet). Serving from the window
/// is valid in exactly the two cases where the result provably equals a full
/// `[start, tip]` fetch (see `get_utxos_up_to_capacity`).
struct EventWindow {
	// Query identity — reset the window if the runtime config ever changes.
	mapping_validator_address: String,
	auth_unit: String,
	cnight_unit: String,
	/// Exact `(block, tx_index)` lower bound of coverage.
	covered_from: (u32, u32),
	covered_to_block: Option<u32>,
	/// All events in the covered range, sorted by `ObservedUtxo::Ord`.
	events: Vec<cnight::ObservedUtxo>,
}

/// What the extension loop should fetch next, and whether the result may be
/// cached (only ranges entirely at or below the stable height are immutable).
#[derive(Debug, PartialEq)]
enum FetchPlan {
	/// Fetch `[from, to_block]` and append it to the cached window.
	Cached { from: (u32, u32), to_block: u32 },
	/// Remainder reaches above the stable height: fetch `[from, tip_block]` for
	/// this call only, without caching.
	Uncached { from: (u32, u32) },
}

/// Pure extension planning, factored out for testability.
///
/// `window_blocks` is `cnight_observation_window_size`: the Cardano blocks fetched per
/// extension. Consumed events are pruned as `start` advances, so it is also roughly the
/// retained history. Bigger means fewer but proportionally longer fetches — the total
/// request count is unchanged, since each transaction is fetched exactly once either way.
fn plan_extension(
	covered_to_block: Option<u32>,
	covered_from: (u32, u32),
	tip_block: u32,
	stable_height: u32,
	window_blocks: u32,
) -> FetchPlan {
	let from = match covered_to_block {
		None => covered_from,
		// Never fetch below the (pruned) coverage bound: when `start` has advanced past
		// stale coverage, the blocks in between hold nothing we would serve.
		Some(covered) => (covered + 1, 0).max(covered_from),
	};
	let to_block = tip_block.min(from.0.saturating_add(window_blocks.saturating_sub(1)));
	if to_block <= stable_height {
		FetchPlan::Cached { from, to_block }
	} else if from.0 <= stable_height {
		FetchPlan::Cached { from, to_block: stable_height }
	} else {
		FetchPlan::Uncached { from }
	}
}

fn event_key(event: &cnight::ObservedUtxo) -> (u32, u32) {
	(event.header.tx_position.block_number, event.header.tx_position.tx_index_in_block)
}

/// Events within `[start, end)` — the bounds the db-sync SQL applies to its queries.
///
/// The cached window can legitimately hold events outside the requested range: its
/// coverage may extend past a lower tip (sibling blocks referencing different Cardano
/// tips) or start below `start` (coverage predating a position jump). Bounding here
/// keeps the served set identical to a fresh `[start, end)` fetch regardless of how
/// coverage was accumulated.
fn bounded_events(
	events: &[cnight::ObservedUtxo],
	start: (u32, u32),
	end: (u32, u32),
) -> Vec<cnight::ObservedUtxo> {
	events
		.iter()
		.filter(|e| {
			let key = event_key(e);
			key >= start && key < end
		})
		.cloned()
		.collect()
}

/// Windowed cNIGHT observation source. Semantically identical to a per-call full
/// `[start, tip]` fetch (which mirrors `MidnightCNightObservationDataSourceImpl`),
/// but fetches the range in bounded windows and serves consecutive Midnight blocks
/// from the accumulated events — the Blockfrost equivalent of the db-sync sliding
/// window cache. Without this, a network whose cNIGHT genesis anchor lies far in
/// the past (e.g. preview's block-0 anchor) re-scans the whole history per block.
pub struct BlockfrostCNightObservationDataSource {
	client: Arc<BlockfrostClient>,
	security_parameter: u32,
	/// `cnight_observation_window_size`: Cardano blocks per window extension.
	window_blocks: u32,
	metrics_opt: Option<MidnightDataSourceMetrics>,
	/// Async mutex: held across fetches, serializing observation calls the same
	/// way the db-sync bulk cache single-flights its refreshes.
	window: tokio::sync::Mutex<Option<EventWindow>>,
}

impl BlockfrostCNightObservationDataSource {
	pub fn new(
		client: Arc<BlockfrostClient>,
		security_parameter: u32,
		window_blocks: u32,
		metrics_opt: Option<MidnightDataSourceMetrics>,
	) -> Self {
		Self {
			client,
			security_parameter,
			window_blocks,
			metrics_opt,
			window: tokio::sync::Mutex::new(None),
		}
	}

	/// Fetch and expand all observation events in the inclusive range
	/// `[from.0:from.1, to_block]`, sorted.
	async fn fetch_events(
		&self,
		cfg: &CNightQueryConfig,
		from: (u32, u32),
		to_block: u32,
	) -> Result<Vec<cnight::ObservedUtxo>, BoxError> {
		let _t = Timer::new(format!("fetch_events[{}:{} -> {to_block}]", from.0, from.1));
		// The asset scan lists transactions that *produce* an output holding cNIGHT, so a
		// transaction that burned cNIGHT outright would not appear here, while db-sync
		// finds it through the spent output. This relies on the cNIGHT policy never
		// burning; if that changes, burns have to be picked up from the asset's
		// mint/burn history as well.
		let from_cursor = Some(BlockCursor::tx(u64::from(from.0), from.1));
		let to_cursor = Some(BlockCursor::block(u64::from(to_block)));
		let (address_rows, asset_rows) = tokio::try_join!(
			self.client.range_txs(
				TxSource::Address(&cfg.mapping_validator_address),
				from_cursor,
				to_cursor,
			),
			self.client.range_txs(TxSource::Asset(&cfg.cnight_unit), from_cursor, to_cursor),
		)?;

		// One entry per distinct tx: a tx present in both scans produces all its event
		// kinds in a single pass, which is equivalent to the four independent SQL scans.
		let mut txs: HashMap<String, RangeTx> = HashMap::new();
		for row in address_rows.into_iter().chain(asset_rows) {
			txs.entry(row.tx_hash.clone()).or_insert(row);
		}

		// Block hash per height (the tx lists don't carry it).
		let mut block_hashes: HashMap<u32, McBlockHash> = HashMap::new();
		let mut events: Vec<cnight::ObservedUtxo> = Vec::new();
		for row in txs.values() {
			let block_hash = match block_hashes.get(&row.block_height) {
				Some(hash) => hash.clone(),
				None => {
					let block = self
						.client
						.block_by_id(&row.block_height.to_string())
						.await?
						.ok_or_else(|| format!("block {} not found", row.block_height))?;
					let hash = McBlockHash(decode_hash32(&block.hash)?);
					block_hashes.insert(row.block_height, hash.clone());
					hash
				},
			};
			let tx_position = CardanoPosition {
				block_hash,
				block_number: row.block_height,
				block_timestamp: cnight::TimestampUnixMillis(i64::try_from(row.block_time)? * 1000),
				tx_index_in_block: row.tx_index,
			};
			let utxos = self.client.tx_utxos(&row.tx_hash).await?;
			events.extend(tx_cnight_events(&self.client, cfg, &tx_position, &utxos).await?);
		}

		// Deterministic consensus ordering comes only from `ObservedUtxo::Ord`
		// (tx position, create-before-spend, utxo identity) — never from API order.
		events.sort();
		Ok(events)
	}
}

#[async_trait::async_trait]
impl MidnightCNightObservationDataSource for BlockfrostCNightObservationDataSource {
	async fn get_utxos_up_to_capacity(
		&self,
		config: &CNightAddresses,
		start_position: &CardanoPosition,
		current_tip: McBlockHash,
		tx_capacity: usize,
		// Deliberately unused, which matches the bulk cache: when
		// `BulkCachedCNightObservationDataSource` serves from its window it applies only
		// `truncate_to_tx_capacity` and never this bound, and its refresh pulls with a
		// wide `LARGE_LIMIT` in place of it (`cnight_observation_bulk.rs`). The direct SQL
		// path does apply it, as a per-category `LIMIT` across four queries, so the two
		// upstream paths already diverge once it binds — one fat transaction with more
		// than the bound's worth of outputs in a single category is enough. That
		// truncation is also not reproducible: three of the four `ORDER BY` clauses are
		// not total orders, so the rows kept at the boundary are whatever Postgres
		// happens to return. We fetch the exact range instead.
		_utxo_overestimate: usize,
	) -> Result<cnight::ObservedUtxos, BoxError> {
		let _t = Timer::new(format!(
			"get_utxos_up_to_capacity[start={}:{}, tip={current_tip}]",
			start_position.block_number, start_position.tx_index_in_block
		));

		let tip = {
			let _sq_timer = start_sub_query_timer(&self.metrics_opt, "cnight_get_block_by_hash");
			self.client
				.block_by_id(&hex::encode(current_tip.0))
				.await?
				.ok_or_else(|| format!("missing reference for block hash `{current_tip}`"))?
		};
		// Historic replay semantics: query through the block's Cardano tip and only then
		// truncate by tx capacity. Clipping the range earlier changes the inherent
		// payload and breaks imports of already-authored blocks.
		let end = cardano_position(&tip)?.increment();
		let tip_block = block_height(&tip)?;
		let cfg = CNightQueryConfig::derive(config)?;
		let start_key = (start_position.block_number, start_position.tx_index_in_block);

		let mut window_guard = self.window.lock().await;

		// (Re)initialize when the window can't serve this query: first call, config
		// change, or a start below current coverage (e.g. reorg or restart).
		let usable = window_guard.as_ref().is_some_and(|w| {
			w.mapping_validator_address == cfg.mapping_validator_address
				&& w.auth_unit == cfg.auth_unit
				&& w.cnight_unit == cfg.cnight_unit
				&& w.covered_from <= start_key
		});
		if !usable {
			*window_guard = Some(EventWindow {
				mapping_validator_address: cfg.mapping_validator_address.clone(),
				auth_unit: cfg.auth_unit.clone(),
				cnight_unit: cfg.cnight_unit.clone(),
				covered_from: start_key,
				covered_to_block: None,
				events: Vec::new(),
			});
		}
		let window = window_guard.as_mut().expect("initialized above");

		// `start` advances monotonically during sync: prune consumed events so the
		// window holds at most one window's worth of history.
		if start_key > window.covered_from {
			window.events.retain(|e| event_key(e) >= start_key);
			window.covered_from = start_key;
		}

		// Stable height resolved lazily — only extensions need it.
		let mut stable_height: Option<u32> = None;

		let end_key = (end.block_number, end.tx_index_in_block);
		loop {
			// Serving is valid in exactly two cases, both decided by the truncation
			// helper over the covered events (bounded to the requested range):
			// (a) capacity binds inside covered range: events past the cut point can't
			//     affect the result, so unfetched range beyond coverage is irrelevant;
			// (b) coverage reaches the tip: the event set is complete for [start, tip].
			let (result, full_window) = truncate_to_tx_capacity(
				bounded_events(&window.events, start_key, end_key),
				tx_capacity,
				start_position,
				end.clone(),
			);
			if !full_window || window.covered_to_block.is_some_and(|c| c >= tip_block) {
				return Ok(result);
			}

			if stable_height.is_none() {
				let _t = Timer::new("GET blocks/latest (stable height)");
				let latest = deadline("blocks/latest", self.client.api.blocks_latest())
					.await?
					.map_err(box_err)?;
				stable_height =
					Some(block_height(&latest)?.saturating_sub(self.security_parameter));
			}

			match plan_extension(
				window.covered_to_block,
				window.covered_from,
				tip_block,
				stable_height.expect("resolved above"),
				self.window_blocks,
			) {
				FetchPlan::Cached { from, to_block } => {
					let new_events = self.fetch_events(&cfg, from, to_block).await?;
					window.events.extend(new_events);
					window.events.sort();
					window.covered_to_block = Some(to_block);
				},
				FetchPlan::Uncached { from } => {
					// The remainder reaches above the stable height, where blocks can
					// still be rolled back: serve it for this call without caching.
					// (Rare: referenced tips are k-deep by the mc-hash stability rules.)
					let overlay = self.fetch_events(&cfg, from, tip_block).await?;
					let mut combined = window.events.clone();
					combined.extend(overlay);
					combined.sort();
					let (result, _) = truncate_to_tx_capacity(
						bounded_events(&combined, start_key, end_key),
						tx_capacity,
						start_position,
						end,
					);
					return Ok(result);
				},
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::super::testing::registration_datum_hex;
	use super::*;
	use blockfrost::blockfrost_openapi::models::tx_content_output_amount_inner::TxContentOutputAmountInner;
	use blockfrost::blockfrost_openapi::models::{
		tx_content_utxo_inputs_inner::TxContentUtxoInputsInner,
		tx_content_utxo_outputs_inner::TxContentUtxoOutputsInner,
	};
	use cardano_serialization_lib::{Credential, Ed25519KeyHash, ScriptHash};

	const NETWORK: u8 = 0;

	fn script_hash() -> ScriptHash {
		ScriptHash::from_bytes(vec![7u8; 28]).unwrap()
	}

	fn mapping_validator_address() -> String {
		EnterpriseAddress::new(NETWORK, &Credential::from_scripthash(&script_hash()))
			.to_address()
			.to_bech32(None)
			.unwrap()
	}

	fn holder_stake_cred() -> Credential {
		Credential::from_keyhash(&Ed25519KeyHash::from_bytes(vec![2u8; 28]).unwrap())
	}

	fn holder_base_address() -> String {
		BaseAddress::new(
			NETWORK,
			&Credential::from_keyhash(&Ed25519KeyHash::from_bytes(vec![1u8; 28]).unwrap()),
			&holder_stake_cred(),
		)
		.to_address()
		.to_bech32(None)
		.unwrap()
	}

	fn holder_enterprise_address() -> String {
		EnterpriseAddress::new(
			NETWORK,
			&Credential::from_keyhash(&Ed25519KeyHash::from_bytes(vec![3u8; 28]).unwrap()),
		)
		.to_address()
		.to_bech32(None)
		.unwrap()
	}

	fn cnight_addresses() -> CNightAddresses {
		CNightAddresses {
			mapping_validator_address: mapping_validator_address(),
			auth_token_asset_name: "auth".into(),
			cnight_policy_id: [9u8; 28],
			cnight_asset_name: "cNIGHT".into(),
		}
	}

	fn query_config() -> CNightQueryConfig {
		CNightQueryConfig::derive(&cnight_addresses()).unwrap()
	}

	fn amount(unit: &str, quantity: &str) -> TxContentOutputAmountInner {
		TxContentOutputAmountInner { unit: unit.into(), quantity: quantity.into() }
	}

	fn output(
		address: &str,
		index: i32,
		amounts: Vec<TxContentOutputAmountInner>,
		inline_datum: Option<String>,
		collateral: bool,
	) -> TxContentUtxoOutputsInner {
		TxContentUtxoOutputsInner {
			address: address.into(),
			amount: amounts,
			output_index: index,
			data_hash: None,
			inline_datum,
			collateral,
			reference_script_hash: None,
			consumed_by_tx: None,
		}
	}

	fn input(
		address: &str,
		tx_hash: &str,
		index: i32,
		amounts: Vec<TxContentOutputAmountInner>,
		inline_datum: Option<String>,
		collateral: bool,
		reference: bool,
	) -> TxContentUtxoInputsInner {
		TxContentUtxoInputsInner {
			address: address.into(),
			amount: amounts,
			tx_hash: tx_hash.into(),
			output_index: index,
			data_hash: None,
			inline_datum,
			reference_script_hash: None,
			collateral,
			reference: Some(reference),
		}
	}

	fn test_client() -> BlockfrostClient {
		// Never contacted: the tests only use inline datums.
		BlockfrostClient::new("http://127.0.0.1:1", None, 432).unwrap()
	}

	fn test_position() -> CardanoPosition {
		CardanoPosition {
			block_hash: McBlockHash([0xBB; 32]),
			block_number: 100,
			block_timestamp: cnight::TimestampUnixMillis(1_700_000_000_000),
			tx_index_in_block: 3,
		}
	}

	fn expected_holder_reward_address() -> cnight::CardanoRewardAddressBytes {
		cnight::CardanoRewardAddressBytes(
			RewardAddress::new(NETWORK, &holder_stake_cred())
				.to_address()
				.to_bytes()
				.try_into()
				.unwrap(),
		)
	}

	const TX_HASH_HEX: &str = "c000000000000000000000000000000000000000000000000000000000000002";

	const SPENT_TX_HASH_HEX: &str =
		"c000000000000000000000000000000000000000000000000000000000000001";

	#[tokio::test]
	async fn cnight_events_from_a_tx() {
		let cfg = query_config();
		let dust_key = [0xAB; 32];
		let datum_hex = registration_datum_hex([4u8; 28], dust_key);
		let utxos = TxContentUtxo {
			hash: TX_HASH_HEX.into(),
			inputs: vec![
				// deregistration: spent mapping-validator UTXO with datum
				input(
					&cfg.mapping_validator_address,
					SPENT_TX_HASH_HEX,
					1,
					vec![amount(&cfg.auth_unit, "1")],
					Some(datum_hex.clone()),
					false,
					false,
				),
				// cNIGHT spend from a base address
				input(
					&holder_base_address(),
					SPENT_TX_HASH_HEX,
					2,
					vec![amount("lovelace", "2000000"), amount(&cfg.cnight_unit, "500")],
					None,
					false,
					false,
				),
				// collateral input carrying cNIGHT: must be ignored
				input(
					&holder_base_address(),
					SPENT_TX_HASH_HEX,
					3,
					vec![amount(&cfg.cnight_unit, "77")],
					None,
					true,
					false,
				),
				// reference input carrying cNIGHT: must be ignored
				input(
					&holder_base_address(),
					SPENT_TX_HASH_HEX,
					4,
					vec![amount(&cfg.cnight_unit, "88")],
					None,
					false,
					true,
				),
			],
			outputs: vec![
				// registration: exactly 1 auth token + datum at the mapping validator
				output(
					&cfg.mapping_validator_address,
					0,
					vec![amount(&cfg.auth_unit, "1")],
					Some(datum_hex.clone()),
					false,
				),
				// auth quantity != 1: not a registration
				output(
					&cfg.mapping_validator_address,
					1,
					vec![amount(&cfg.auth_unit, "2")],
					Some(datum_hex.clone()),
					false,
				),
				// cNIGHT create at a base address
				output(
					&holder_base_address(),
					2,
					vec![amount(&cfg.cnight_unit, "500")],
					None,
					false,
				),
				// cNIGHT at an enterprise address: no stake credential, skipped
				output(
					&holder_enterprise_address(),
					3,
					vec![amount(&cfg.cnight_unit, "10")],
					None,
					false,
				),
				// collateral output carrying cNIGHT: must be ignored
				output(&holder_base_address(), 4, vec![amount(&cfg.cnight_unit, "20")], None, true),
			],
		};

		let mut events =
			tx_cnight_events(&test_client(), &cfg, &test_position(), &utxos).await.unwrap();
		events.sort();

		let tx_hash = McTxHash(decode_hash32(TX_HASH_HEX).unwrap());
		let spent_tx_hash = McTxHash(decode_hash32(SPENT_TX_HASH_HEX).unwrap());
		let expected_registration_reward_address = cnight::CardanoRewardAddressBytes(
			RewardAddress::new(
				NETWORK,
				&Credential::from_keyhash(&Ed25519KeyHash::from_bytes(vec![4u8; 28]).unwrap()),
			)
			.to_address()
			.to_bytes()
			.try_into()
			.unwrap(),
		);

		assert_eq!(events.len(), 4);
		let kinds: Vec<&cnight::ObservedUtxoData> = events.iter().map(|e| &e.data).collect();
		match &kinds[..] {
			[
				cnight::ObservedUtxoData::Registration(registration),
				cnight::ObservedUtxoData::AssetCreate(create),
				cnight::ObservedUtxoData::Deregistration(deregistration),
				cnight::ObservedUtxoData::AssetSpend(spend),
			] => {
				assert_eq!(&registration.dust_public_key.0[..], &dust_key[..]);
				assert_eq!(
					registration.cardano_reward_address,
					expected_registration_reward_address
				);
				assert_eq!(create.value, 500);
				assert_eq!(create.owner, expected_holder_reward_address());
				assert_eq!(create.utxo_tx_hash, tx_hash);
				assert_eq!(create.utxo_tx_index, 2);
				assert_eq!(&deregistration.dust_public_key.0[..], &dust_key[..]);
				assert_eq!(spend.value, 500);
				assert_eq!(spend.owner, expected_holder_reward_address());
				assert_eq!(spend.utxo_tx_hash, spent_tx_hash);
				assert_eq!(spend.utxo_tx_index, 2);
				assert_eq!(spend.spending_tx_hash, tx_hash);
			},
			other => panic!("unexpected events: {other:?}"),
		}

		// Deregistration header points at the spent UTXO, positioned at the spending tx.
		let deregistration = &events[2];
		assert_eq!(deregistration.header.tx_hash, tx_hash);
		assert_eq!(deregistration.header.utxo_tx_hash, spent_tx_hash);
		assert_eq!(deregistration.header.utxo_index.0, 1);
		assert_eq!(deregistration.header.tx_position, test_position());
	}

	fn event_at(block: u32, tx_index: u32) -> cnight::ObservedUtxo {
		cnight::ObservedUtxo {
			header: cnight::ObservedUtxoHeader {
				tx_position: CardanoPosition {
					block_hash: McBlockHash([0xAA; 32]),
					block_number: block,
					block_timestamp: cnight::TimestampUnixMillis(0),
					tx_index_in_block: tx_index,
				},
				tx_hash: McTxHash([1; 32]),
				utxo_tx_hash: McTxHash([1; 32]),
				utxo_index: cnight::UtxoIndexInTx(0),
			},
			data: cnight::ObservedUtxoData::AssetCreate(cnight::CreateData {
				value: 1,
				owner: expected_holder_reward_address(),
				utxo_tx_hash: McTxHash([1; 32]),
				utxo_tx_index: 0,
			}),
		}
	}

	#[test]
	fn bounded_events_applies_sql_range_bounds() {
		let events: Vec<_> = [(100, 0), (200, 5), (300, 0), (300, 7), (400, 0)]
			.map(|(b, i)| event_at(b, i))
			.into();
		// A cached window wider than the request on both sides serves only [start, end).
		let served = bounded_events(&events, (200, 5), (300, 8));
		assert_eq!(
			served.iter().map(event_key).collect::<Vec<_>>(),
			vec![(200, 5), (300, 0), (300, 7)]
		);
		// Start bound is inclusive, end bound exclusive.
		assert_eq!(bounded_events(&events, (200, 6), (300, 7)).len(), 1);
		// A tip below all coverage serves nothing (sibling block with a lower tip).
		assert!(bounded_events(&events, (100, 0), (100, 0)).is_empty());
	}

	#[test]
	fn plan_extension_windows_and_stable_clamp() {
		// Any span works; this was the hard-coded value before it came from config.
		const SPAN: u32 = 50_000;
		// Fresh window: fetch starts at the exact (block, tx_index) coverage bound.
		assert_eq!(
			plan_extension(None, (100, 3), 1_000, 1_000_000, SPAN),
			FetchPlan::Cached { from: (100, 3), to_block: 1_000 }
		);
		// Extension continues at covered+1, index 0, clamped to the window span.
		assert_eq!(
			plan_extension(Some(1_000), (100, 3), 1_000_000, 2_000_000, SPAN),
			FetchPlan::Cached { from: (1_001, 0), to_block: 1_000 + SPAN }
		);
		// Tip clamps below the window size.
		assert_eq!(
			plan_extension(Some(1_000), (100, 3), 20_000, 2_000_000, SPAN),
			FetchPlan::Cached { from: (1_001, 0), to_block: 20_000 }
		);
		// Range crossing the stable height caches only the stable part.
		assert_eq!(
			plan_extension(Some(1_000), (100, 3), 40_000, 30_000, SPAN),
			FetchPlan::Cached { from: (1_001, 0), to_block: 30_000 }
		);
		// Entirely above the stable height: fetch without caching.
		assert_eq!(
			plan_extension(Some(30_000), (100, 3), 40_000, 30_000, SPAN),
			FetchPlan::Uncached { from: (30_001, 0) }
		);
		// `start` advanced past stale coverage: never fetch below the coverage bound.
		assert_eq!(
			plan_extension(Some(200), (300, 0), 1_000, 1_000_000, SPAN),
			FetchPlan::Cached { from: (300, 0), to_block: 1_000 }
		);
	}
}
