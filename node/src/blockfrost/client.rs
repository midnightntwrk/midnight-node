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

//! The HTTP/SDK client: one `BlockfrostClient` wrapping the Blockfrost SDK plus a raw
//! `reqwest` path for the one endpoint the SDK cannot model, with paging and cursors.

use std::num::NonZeroUsize;
use std::sync::Mutex;

use blockfrost::{
	BlockCursor, BlockFrostSettings, BlockfrostAPI, Order, Pagination, RetrySettings,
	blockfrost_openapi::models::{block_content::BlockContent, tx_content_utxo::TxContentUtxo},
};
use cardano_serialization_lib::{PlutusData, PlutusDatumSchema, encode_json_value_to_plutus_datum};
use lru::LruCache;
use sidechain_domain::*;

use super::convert::*;
use super::support::*;

const USER_AGENT: &str = concat!("midnight-node/", env!("CARGO_PKG_VERSION"));

/// One row of `/addresses/{addr}/transactions` or `/assets/{unit}/transactions`.
#[derive(Debug, Clone, serde::Deserialize)]
pub(crate) struct RangeTx {
	pub(crate) tx_hash: String,
	pub(crate) tx_index: u32,
	pub(crate) block_height: u32,
	pub(crate) block_time: u64,
}

#[derive(Debug, serde::Deserialize)]
struct RawTxMetadata {
	label: String,
	json_metadata: serde_json::Value,
}

/// Which transaction list to query.
#[derive(Clone, Copy)]
pub(crate) enum TxSource<'a> {
	Address(&'a str),
	Asset(&'a str),
}

impl std::fmt::Display for TxSource<'_> {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			TxSource::Address(address) => write!(f, "addresses/{address}"),
			TxSource::Asset(asset) => write!(f, "assets/{asset}"),
		}
	}
}

fn range_tx(
	tx_hash: String,
	tx_index: i32,
	block_height: i32,
	block_time: i32,
) -> Result<RangeTx, BoxError> {
	Ok(RangeTx {
		tx_hash,
		tx_index: u32::try_from(tx_index)?,
		block_height: u32::try_from(block_height)?,
		block_time: u64::try_from(block_time)?,
	})
}

/// Shared Blockfrost client: the typed SDK plus a raw `reqwest` client for the one
/// endpoint the SDK cannot represent (array-shaped tx metadata).
pub struct BlockfrostClient {
	pub(crate) api: BlockfrostAPI,
	http: reqwest::Client,
	base_url: String,
	security_parameter: u32,
	/// Blocks fetched by hash, shared across all data sources.
	///
	/// Only blocks with at least `security_parameter` confirmations are cached. A
	/// block's *contents* are immutable, but its membership in the canonical chain is
	/// not: caching an unstable block would let a later stability check classify a
	/// since-rolled-back block as stable, where db-sync reports it as unknown. Beyond
	/// `k` confirmations a rollback is outside the security model — the same assumption
	/// the db-sync block cache makes. By-number lookups are never cached (the block at
	/// a given height can change on rollback).
	blocks_by_hash: Mutex<LruCache<String, BlockContent>>,
}

impl BlockfrostClient {
	pub fn new(
		endpoint: &str,
		project_id: Option<&str>,
		security_parameter: u32,
	) -> Result<Self, BoxError> {
		// Parsed here so a malformed endpoint is a startup error. The SDK parses per
		// request, so otherwise the node boots, reports the backend, then fails every
		// Cardano read for the life of the process.
		let base = endpoint.trim_end_matches('/').to_string();
		let parsed = reqwest::Url::parse(&base)
			.map_err(|e| format!("blockfrost_endpoint is not a valid URL ({e}): {base}"))?;
		if !matches!(parsed.scheme(), "http" | "https") {
			return Err(format!(
				"blockfrost_endpoint must be an http(s) URL, got scheme `{}`: {base}",
				parsed.scheme()
			)
			.into());
		}
		let mut settings = BlockFrostSettings::new();
		settings.base_url = Some(base);
		settings.retry_settings = RetrySettings::new(RETRY_AMOUNT, RETRY_DELAY);
		settings.headers.insert("User-Agent".to_string(), USER_AGENT.to_string());

		// Check the project id before the SDK sees it: the SDK builds a header from it
		// with an `expect`, so a value that is not header-safe panics, and the panic
		// message contains the id. Report the problem without echoing the secret.
		let project_id = project_id.unwrap_or_default();
		if !project_id.is_empty() && !project_id.bytes().all(|b| b.is_ascii_alphanumeric()) {
			return Err("blockfrost_project_id must be ASCII alphanumeric".into());
		}

		// Our own client gets a deadline here; SDK calls get one from `deadline()` (its
		// HTTP client is built by the crate itself, on a different reqwest version).
		// Without a deadline a server that accepts the connection and then stalls blocks
		// inherent creation or verification indefinitely.
		let api = BlockfrostAPI::new(project_id, settings);

		let mut headers = reqwest::header::HeaderMap::new();
		if !project_id.is_empty() {
			headers.insert("project_id", project_id.parse()?);
		}
		let http = reqwest::Client::builder()
			.default_headers(headers)
			.user_agent(USER_AGENT)
			.timeout(HTTP_TIMEOUT)
			.build()?;

		Ok(Self {
			api,
			http,
			base_url: endpoint.trim_end_matches('/').to_string(),
			security_parameter,
			blocks_by_hash: Mutex::new(LruCache::new(NonZeroUsize::new(1024).unwrap())),
		})
	}

	/// Raw paged GET returning deserialized rows; treats 404 as an empty result
	/// (unknown address/asset ≡ no db-sync rows) and retries on 429.
	async fn get_paged<T: serde::de::DeserializeOwned>(
		&self,
		path_and_query: &str,
		page: usize,
	) -> Result<Vec<T>, BoxError> {
		let sep = if path_and_query.contains('?') { '&' } else { '?' };
		let url = format!("{}/{path_and_query}{sep}count={PAGE_SIZE}&page={page}", self.base_url);
		let _t = Timer::new(format!("GET {path_and_query} page={page}"));
		// The deadline covers the whole retry loop, not each attempt: `HTTP_TIMEOUT` is
		// already the per-request bound on `self.http`, and `RETRY_AMOUNT` attempts of it
		// would otherwise add up to minutes. This matches the SDK paths, where `deadline`
		// also bounds the SDK's internal retries.
		let fetch = async {
			let mut attempts = 0;
			loop {
				let response = self.http.get(&url).send().await?;
				match response.status().as_u16() {
					404 => return Ok(vec![]),
					402 => {
						log::error!("{OVER_QUOTA_MESSAGE}");
						return Err(OVER_QUOTA_MESSAGE.into());
					},
					// Same transient set the SDK's `RetrySettings` covers, so the raw path
					// is no less resilient than every SDK call. 402 is deliberately absent:
					// a quota does not clear inside a retry window.
					408 | 429 | 500 | 502 | 503 | 504 if attempts < RETRY_AMOUNT => {
						attempts += 1;
						tokio::time::sleep(RETRY_DELAY).await;
					},
					status if !response.status().is_success() => {
						return Err(format!("GET {url} failed with status {status}").into());
					},
					_ => return Ok(response.json().await?),
				}
			}
		};
		deadline::<Vec<T>, BoxError>(&format!("GET {path_and_query}"), fetch).await?
	}

	/// One page of a transaction list with optional server-side block-range cursors.
	/// 404 (unknown address/asset) ≡ no db-sync rows: an empty page.
	async fn txs_page(
		&self,
		source: TxSource<'_>,
		pagination: Pagination,
	) -> Result<Vec<RangeTx>, BoxError> {
		let _t = Timer::new(format!(
			"SDK {source}/transactions page={} from={:?} to={:?}",
			pagination.page,
			pagination.from.map(|c| c.to_string()),
			pagination.to.map(|c| c.to_string()),
		));
		let label = format!("{source}/transactions");
		let rows = match source {
			TxSource::Address(address) => {
				deadline(&label, self.api.addresses_transactions(address, pagination))
					.await?
					.map(|rows| {
						rows.into_iter()
							.map(|r| (r.tx_hash, r.tx_index, r.block_height, r.block_time))
							.collect::<Vec<_>>()
					})
			},
			TxSource::Asset(asset) => {
				deadline(&label, self.api.assets_transactions(asset, pagination)).await?.map(
					|rows| {
						rows.into_iter()
							.map(|r| (r.tx_hash, r.tx_index, r.block_height, r.block_time))
							.collect::<Vec<_>>()
					},
				)
			},
		};
		match rows {
			Ok(rows) => rows
				.into_iter()
				.map(|(hash, tx_index, height, time)| range_tx(hash, tx_index, height, time))
				.collect(),
			Err(e) if is_404(&e) => Ok(vec![]),
			Err(e) => Err(box_err(e)),
		}
	}

	/// All transactions of `source` within the inclusive `from`/`to` block(-and-tx-index)
	/// range, in ascending chain order.
	///
	/// The cursors are evaluated server-side; page-walking below a pinned `to` bound is
	/// safe for consensus data because the chain below that bound is append-only.
	pub(crate) async fn range_txs(
		&self,
		source: TxSource<'_>,
		from: Option<BlockCursor>,
		to: Option<BlockCursor>,
	) -> Result<Vec<RangeTx>, BoxError> {
		let mut rows: Vec<RangeTx> = Vec::new();
		for page in 1..=MAX_PAGES {
			let mut pagination = Pagination::new(Order::Asc, page, PAGE_SIZE);
			pagination.from = from;
			pagination.to = to;
			let batch = self.txs_page(source, pagination).await?;
			let last_page = batch.len() < PAGE_SIZE;
			rows.extend(batch);
			if last_page {
				return Ok(rows);
			}
		}
		Err(too_many_pages(&format!("{source}/transactions")))
	}

	/// One descending-order page of transactions of `source` up to `to_block` (inclusive).
	pub(crate) async fn range_txs_desc_page(
		&self,
		source: TxSource<'_>,
		to_block: u32,
		page: usize,
	) -> Result<Vec<RangeTx>, BoxError> {
		let mut pagination = Pagination::new(Order::Desc, page, PAGE_SIZE);
		pagination.to = Some(BlockCursor::block(u64::from(to_block)));
		self.txs_page(source, pagination).await
	}

	/// The metadata value for `label`, via raw GET: the SDK's model cannot represent
	/// array-shaped `json_metadata` (the shape used by bridge transfers).
	///
	/// Pages until the label is found. db-sync selects the row by key
	/// (`tx_metadata.key = $5`), so stopping at page one would lose the transfer label
	/// on a transaction carrying more than `PAGE_SIZE` labels — which the sender
	/// controls, and which would make this backend disagree with db-sync on the block.
	pub(crate) async fn tx_metadata_label(
		&self,
		tx_hash: &str,
		label: &str,
	) -> Result<Option<serde_json::Value>, BoxError> {
		for page in 1..=MAX_PAGES {
			let rows: Vec<RawTxMetadata> =
				self.get_paged(&format!("txs/{tx_hash}/metadata"), page).await?;
			let last_page = rows.len() < PAGE_SIZE;
			if let Some(row) = rows.into_iter().find(|r| r.label == label) {
				return Ok(Some(row.json_metadata));
			}
			if last_page {
				return Ok(None);
			}
		}
		Err(too_many_pages(&format!("txs/{tx_hash}/metadata")))
	}

	/// Hash of the block at `number`; `None` on 404. Used by the parity test to
	/// resolve an anchor block by height.
	pub async fn block_hash_by_number(&self, number: u32) -> Result<Option<McBlockHash>, BoxError> {
		let block = self.block_by_id(&number.to_string()).await?;
		block.map(|b| Ok(McBlockHash(decode_hash32(&b.hash)?))).transpose()
	}

	/// Block by hash or number; `None` on 404.
	pub(crate) async fn block_by_id(&self, id: &str) -> Result<Option<BlockContent>, BoxError> {
		let is_hash = id.len() == 64 && id.bytes().all(|b| b.is_ascii_hexdigit());
		if is_hash
			&& let Ok(mut cache) = self.blocks_by_hash.lock()
			&& let Some(block) = cache.get(id)
		{
			return Ok(Some(block.clone()));
		}
		let _t = Timer::new(format!("GET blocks/{id}"));
		match deadline(&format!("blocks/{id}"), self.api.blocks_by_id(id)).await? {
			Ok(b) => {
				let stable = u32::try_from(b.confirmations).unwrap_or(0) >= self.security_parameter;
				if is_hash
					&& stable && let Ok(mut cache) = self.blocks_by_hash.lock()
				{
					cache.put(id.to_string(), b.clone());
				}
				Ok(Some(b))
			},
			Err(e) if is_404(&e) => Ok(None),
			Err(e) => Err(box_err(e)),
		}
	}

	pub(crate) async fn tx_utxos(&self, tx_hash: &str) -> Result<TxContentUtxo, BoxError> {
		let _t = Timer::new(format!("GET txs/{tx_hash}/utxos"));
		deadline(&format!("txs/{tx_hash}/utxos"), self.api.transactions_utxos(tx_hash))
			.await?
			.map_err(box_err)
	}

	/// Resolve a Plutus datum: inline CBOR when present, otherwise `/scripts/datum/{hash}`.
	///
	/// The datum-by-hash path decodes the same detailed-schema JSON as db-sync's
	/// `datum.value` JSONB, through the identical `encode_json_value_to_plutus_datum`
	/// call, so all downstream datum parsers behave exactly as on the db-sync path.
	pub(crate) async fn datum(
		&self,
		inline_datum: Option<&String>,
		data_hash: Option<&String>,
	) -> Result<Option<PlutusData>, BoxError> {
		if let Some(cbor_hex) = inline_datum {
			let datum = PlutusData::from_hex(cbor_hex)
				.map_err(|e| format!("invalid inline datum CBOR: {e:?}"))?;
			return Ok(Some(datum));
		}
		let Some(hash) = data_hash else {
			return Ok(None);
		};
		let _t = Timer::new(format!("GET scripts/datum/{hash}"));
		let value =
			match deadline(&format!("scripts/datum/{hash}"), self.api.scripts_datum_hash(hash))
				.await?
			{
				Ok(v) => v,
				Err(e) if is_404(&e) => return Ok(None),
				Err(e) => return Err(box_err(e)),
			};
		let json = value
			.get("json_value")
			.cloned()
			.ok_or_else(|| format!("no json_value in datum response for {hash}"))?;
		let datum = encode_json_value_to_plutus_datum(json, PlutusDatumSchema::DetailedSchema)
			.map_err(|e| format!("invalid datum JSON for {hash}: {e:?}"))?;
		Ok(Some(datum))
	}
}

#[cfg(test)]
mod tests {
	use super::super::testing::{
		Reply, asset_txs_json, client_at, fake_server, over_quota_body, registration_datum_hex,
	};
	use super::*;
	use cardano_serialization_lib::{
		PlutusDatumSchema, decode_plutus_datum_to_json_value, encode_json_value_to_plutus_datum,
	};

	// These pin the mechanics the live parity test cannot reach without credentials:
	// pagination termination, cursor forwarding, 404 handling, 429 retries and the
	// deadline. Cursor forwarding matters most — the SDK honours `from`/`to` only on the
	// account, address and asset transaction endpoints and drops them silently elsewhere,
	// which an SDK upgrade could change without any compile error.

	#[test]
	fn detailed_schema_json_decodes_to_same_datum_as_inline_cbor() {
		// The datum-by-hash path decodes Blockfrost's `json_value` (db-sync detailed
		// schema JSON); it must yield the same PlutusData as the inline CBOR path.
		let hex = registration_datum_hex([4u8; 28], [0xAB; 32]);
		let from_cbor = PlutusData::from_hex(&hex).unwrap();
		let json = decode_plutus_datum_to_json_value(&from_cbor, PlutusDatumSchema::DetailedSchema)
			.unwrap();
		let from_json =
			encode_json_value_to_plutus_datum(json, PlutusDatumSchema::DetailedSchema).unwrap();
		assert_eq!(from_cbor.to_hex(), from_json.to_hex());
	}

	#[tokio::test]
	async fn raw_path_reports_over_quota_plainly() {
		let (url, seen) = fake_server(vec![Reply::Json(402, over_quota_body())]).await;
		let err = client_at(&url).tx_metadata_label("ab", "1").await.expect_err("402 must fail");
		let message = err.to_string();
		assert!(message.contains("over its request limit"), "{message}");
		assert!(message.contains("402"), "{message}");
		assert!(message.contains("will not progress"), "must explain the consequence: {message}");
		assert_eq!(seen.lock().expect("seen").len(), 1, "402 must not be retried");
	}

	#[tokio::test]
	async fn sdk_path_reports_over_quota_plainly() {
		let (url, _seen) = fake_server(vec![Reply::Json(402, over_quota_body())]).await;
		let err = client_at(&url)
			.range_txs(TxSource::Asset("beef"), None, None)
			.await
			.expect_err("402 must fail");
		let message = err.to_string();
		assert!(message.contains("over its request limit"), "{message}");
		assert!(
			!message.contains("Blockfrost error:"),
			"must not fall through to the generic error: {message}"
		);
	}

	#[tokio::test]
	async fn metadata_label_is_found_beyond_the_first_page() {
		// A full page of unrelated labels, then the transfer label on page two. Stopping
		// at page one would yield no recipient here while db-sync, which selects the row
		// by key, still finds it — a divergence the sender can trigger at will.
		let filler: Vec<String> = (0..PAGE_SIZE)
			.map(|i| format!(r#"{{"label":"{i}","json_metadata":"x"}}"#))
			.collect();
		let page_one = format!("[{}]", filler.join(","));
		let page_two = r#"[{"label":"6500973","json_metadata":["0xabcd"]}]"#.to_string();
		let (url, seen) =
			fake_server(vec![Reply::Json(200, page_one), Reply::Json(200, page_two)]).await;

		let found = client_at(&url)
			.tx_metadata_label("ab", "6500973")
			.await
			.expect("metadata lookup");

		assert_eq!(found, Some(serde_json::json!(["0xabcd"])));
		assert_eq!(seen.lock().expect("seen").len(), 2, "must walk to the second page");
	}

	#[tokio::test]
	async fn paged_get_maps_404_to_empty_without_retrying() {
		let (url, seen) = fake_server(vec![Reply::Json(404, "{}".into())]).await;
		let found = client_at(&url)
			.tx_metadata_label("ab", "1")
			.await
			.expect("404 is an empty result");
		assert!(found.is_none());
		assert_eq!(seen.lock().expect("seen").len(), 1, "a 404 must not be retried");
	}

	#[tokio::test]
	async fn paged_get_retries_429_then_succeeds() {
		let body = r#"[{"label":"1","json_metadata":"abc"}]"#;
		let (url, seen) = fake_server(vec![
			Reply::Json(429, "{}".into()),
			Reply::Json(429, "{}".into()),
			Reply::Json(200, body.into()),
		])
		.await;
		let found = client_at(&url)
			.tx_metadata_label("ab", "1")
			.await
			.expect("retried past the 429s");
		assert_eq!(found, Some(serde_json::json!("abc")));
		assert_eq!(seen.lock().expect("seen").len(), 3, "two 429s then the success");
	}

	#[tokio::test]
	async fn paged_get_gives_up_after_the_retry_budget() {
		let (url, seen) = fake_server(vec![Reply::Json(429, "{}".into())]).await;
		let err = client_at(&url)
			.tx_metadata_label("ab", "1")
			.await
			.expect_err("429 forever must fail");
		assert!(err.to_string().contains("429"), "unexpected error: {err}");
		assert_eq!(
			seen.lock().expect("seen").len(),
			RETRY_AMOUNT as usize + 1,
			"the budget is attempts-after-the-first"
		);
	}

	#[tokio::test]
	async fn deadline_bounds_the_whole_retry_loop_not_each_attempt() {
		// Each attempt answers just under the per-request bound, so only a loop-level
		// deadline can stop the retries: per-attempt bounds alone would allow
		// RETRY_AMOUNT of them to stack up.
		let slow = || Reply::Slow(HTTP_TIMEOUT.mul_f32(0.75), 429, "{}".into());
		let (url, seen) = fake_server(vec![slow(), slow(), slow(), slow()]).await;
		let started = std::time::Instant::now();
		let err = client_at(&url).tx_metadata_label("ab", "1").await.expect_err("must not run on");
		let elapsed = started.elapsed();
		assert!(err.to_string().contains("timed out"), "unexpected error: {err}");
		assert!(elapsed < HTTP_TIMEOUT * 2, "not bounded by the deadline: {elapsed:?}");
		assert!(
			seen.lock().expect("seen").len() < RETRY_AMOUNT as usize,
			"stopped before exhausting the retry budget"
		);
	}

	#[tokio::test]
	async fn range_txs_forwards_cursors_and_stops_on_a_short_page() {
		let (url, seen) = fake_server(vec![
			Reply::Json(200, asset_txs_json(PAGE_SIZE, 1)),
			Reply::Json(200, asset_txs_json(2, PAGE_SIZE as u32 + 1)),
		])
		.await;
		let rows = client_at(&url)
			.range_txs(
				TxSource::Asset("beef"),
				Some(BlockCursor::block(10)),
				Some(BlockCursor::block(999)),
			)
			.await
			.expect("range_txs");

		assert_eq!(rows.len(), PAGE_SIZE + 2, "rows from both pages are returned");
		assert_eq!(rows[0].block_height, 1);
		assert_eq!(rows[PAGE_SIZE + 1].block_height, PAGE_SIZE as u32 + 2);
		let seen = seen.lock().expect("seen");
		assert_eq!(seen.len(), 2, "stopped at the short page");
		// The cursors must reach the wire: the SDK drops them for other endpoints.
		assert!(seen[0].contains("from=10"), "`from` not forwarded: {}", seen[0]);
		assert!(seen[0].contains("to=999"), "`to` not forwarded: {}", seen[0]);
		assert!(seen[0].contains("order=asc"), "wrong order: {}", seen[0]);
		assert!(seen[0].contains("/assets/beef/transactions"), "wrong path: {}", seen[0]);
		assert!(seen[1].contains("page=2"), "second page not requested: {}", seen[1]);
		assert!(seen[1].contains("from=10"), "cursor lost on page 2: {}", seen[1]);
	}

	#[tokio::test]
	async fn range_txs_gives_up_when_the_pages_never_end() {
		// A backend that ignores `page` and always answers with a full page: without the
		// cap this walks for ever and grows `rows` without bound.
		let (url, seen) = fake_server(vec![Reply::Json(200, asset_txs_json(PAGE_SIZE, 1))]).await;
		let err = client_at(&url)
			.range_txs(TxSource::Asset("beef"), None, None)
			.await
			.expect_err("must be bounded");
		assert!(err.to_string().contains("without reaching the end"), "unexpected: {err}");
		assert_eq!(seen.lock().expect("seen").len(), MAX_PAGES, "walked exactly the cap");
	}
}
