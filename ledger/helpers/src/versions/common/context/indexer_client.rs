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

//! A GraphQL client for the Midnight indexer's `api/v4` interface.
//!
//! Operations are typed against the indexer's committed schema
//! (`indexer/indexer-api/graphql/schema-v4.graphql`) at compile time via [`graphql_client`], so a
//! schema change in the indexer submodule (renamed/removed field, changed type) breaks this build
//! instead of failing at runtime. The query documents live in `ledger/helpers/graphql/indexer.graphql`.
//!
//! It speaks GraphQL over HTTP (queries/mutations) and over the `graphql-transport-ws` WebSocket
//! sub-protocol (subscriptions) — the WS framing mirrors the indexer's own reference client in
//! `indexer/indexer-tests/src/graphql_ws_client.rs`. The `HexEncoded` scalar is decoded to raw
//! bytes here; turning those blobs into ledger types is the job of
//! [`super::indexer_context::IndexerContext`].
//!
//! Only the operations needed by the read-only `show-wallet` path are defined (see issue #1186):
//! `connect`/`disconnect`, the latest `block`, and the shielded/unshielded/dust subscriptions.

use futures::{SinkExt, StreamExt};
use graphql_client::GraphQLQuery;
use serde_json::{Value, json};
use thiserror::Error;
use tokio::net::TcpStream;
use tokio_tungstenite::{
	MaybeTlsStream, WebSocketStream, connect_async,
	tungstenite::{Message, client::IntoClientRequest},
};

// Custom GraphQL scalars, resolved by name from this module scope by the `GraphQLQuery` derives
// below (the indexer's own client does the same). `HexEncoded` blobs are kept as hex strings and
// decoded to bytes here; `Unit` is the indexer's void scalar (JSON `null`).
#[allow(non_camel_case_types)]
type HexEncoded = String;
#[allow(non_camel_case_types)]
type ViewingKey = String;
#[allow(non_camel_case_types)]
type UnshieldedAddress = String;
#[allow(non_camel_case_types)]
type Unit = Option<bool>;

#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "../../indexer/indexer-api/graphql/schema-v4.graphql",
	query_path = "graphql/indexer.graphql",
	response_derives = "Debug, Clone"
)]
pub struct LatestBlock;

#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "../../indexer/indexer-api/graphql/schema-v4.graphql",
	query_path = "graphql/indexer.graphql",
	response_derives = "Debug, Clone"
)]
pub struct Connect;

#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "../../indexer/indexer-api/graphql/schema-v4.graphql",
	query_path = "graphql/indexer.graphql",
	response_derives = "Debug, Clone"
)]
pub struct Disconnect;

#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "../../indexer/indexer-api/graphql/schema-v4.graphql",
	query_path = "graphql/indexer.graphql",
	response_derives = "Debug, Clone"
)]
pub struct ShieldedTransactions;

#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "../../indexer/indexer-api/graphql/schema-v4.graphql",
	query_path = "graphql/indexer.graphql",
	response_derives = "Debug, Clone"
)]
pub struct UnshieldedTransactions;

#[derive(GraphQLQuery)]
#[graphql(
	schema_path = "../../indexer/indexer-api/graphql/schema-v4.graphql",
	query_path = "graphql/indexer.graphql",
	response_derives = "Debug, Clone"
)]
pub struct DustLedgerEvents;

#[derive(Debug, Error)]
pub enum IndexerClientError {
	#[error("http transport error: {0}")]
	Http(#[from] reqwest::Error),
	#[error("websocket error: {0}")]
	WebSocket(String),
	#[error("graphql error: {0}")]
	GraphQl(String),
	#[error("malformed response: {0}")]
	Malformed(String),
	#[error("decode error: {0}")]
	Decode(String),
}

pub type IndexerResult<T> = Result<T, IndexerClientError>;

/// Status of a transaction as reported by the indexer (`TransactionResultStatus`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionResultKind {
	Success,
	PartialSuccess,
	Failure,
}

/// The latest block as seen by the indexer; used to detect catch-up and to read ledger params.
#[derive(Debug, Clone)]
pub struct BlockInfo {
	pub height: u64,
	/// Block timestamp in unix seconds.
	pub timestamp: u64,
	/// Exclusive end index into the global zswap merkle tree at this block.
	pub zswap_end_index: u64,
	/// Tagged-serialized `LedgerParameters` blob.
	pub ledger_parameters: Vec<u8>,
}

/// An item from the `shieldedTransactions` subscription.
#[derive(Debug, Clone)]
pub enum ShieldedEvent {
	/// A wallet-relevant transaction, with the gap-filling merkle update that precedes it.
	Relevant {
		/// Tagged-serialized `Transaction` blob (`RegularTransaction.raw`).
		raw_transaction: Vec<u8>,
		result: TransactionResultKind,
		/// Inclusive start index of this transaction's outputs in the global zswap tree.
		zswap_start_index: u64,
		/// Exclusive end index of this transaction's outputs in the global zswap tree.
		zswap_end_index: u64,
		/// Serialized `MerkleTreeCollapsedUpdate` filling the gap before `zswap_start_index`.
		collapsed_update: Option<Vec<u8>>,
	},
	/// A progress sentinel describing how far the indexer has scanned for this wallet.
	Progress {
		highest_end_index: u64,
		highest_checked_end_index: u64,
		highest_relevant_end_index: u64,
	},
}

/// A single unshielded UTXO as reported by the indexer.
#[derive(Debug, Clone)]
pub struct UnshieldedUtxoData {
	/// Serialized token type (32-byte hash).
	pub token_type: Vec<u8>,
	pub value: u128,
	/// Serialized intent hash (32-byte hash).
	pub intent_hash: Vec<u8>,
	pub output_index: u32,
	/// Creation time in unix seconds (absent for some genesis UTXOs).
	pub ctime: Option<u64>,
}

/// An item from the `unshieldedTransactions` subscription.
#[derive(Debug, Clone)]
pub enum UnshieldedEvent {
	Transaction {
		/// The indexer's transaction id, compared against `Progress::highest_transaction_id` to
		/// detect catch-up (the same id space `make_progress_update` reports).
		transaction_id: u64,
		created: Vec<UnshieldedUtxoData>,
		spent: Vec<UnshieldedUtxoData>,
	},
	Progress { highest_transaction_id: u64 },
}

/// An item from the `dustLedgerEvents` subscription.
#[derive(Debug, Clone)]
pub struct DustLedgerEventData {
	pub id: u64,
	pub max_id: u64,
	/// Tagged-serialized ledger `Event` blob (`DustLedgerEvent.raw`).
	pub raw: Vec<u8>,
}

/// A GraphQL client targeting one indexer `api/v4` base URL.
pub struct IndexerClient {
	http: reqwest::Client,
	/// HTTP endpoint, e.g. `http://host:8088/api/v4/graphql`.
	http_url: String,
	/// WebSocket endpoint, e.g. `ws://host:8088/api/v4/graphql/ws`.
	ws_url: String,
}

impl IndexerClient {
	/// Build a client from an `api/v4` base URL, e.g. `http://127.0.0.1:8088/api/v4`.
	///
	/// The GraphQL endpoints (`/graphql`, `/graphql/ws`) are appended automatically, and the
	/// WebSocket scheme is derived from the HTTP scheme (`http`→`ws`, `https`→`wss`).
	pub fn new(base_url: &str) -> IndexerResult<Self> {
		let base = base_url.trim_end_matches('/');
		let http_url = format!("{base}/graphql");

		let ws_base = if let Some(rest) = base.strip_prefix("https://") {
			format!("wss://{rest}")
		} else if let Some(rest) = base.strip_prefix("http://") {
			format!("ws://{rest}")
		} else {
			return Err(IndexerClientError::Decode(format!(
				"indexer url must start with http:// or https://, got {base_url}"
			)));
		};
		let ws_url = format!("{ws_base}/graphql/ws");

		Ok(Self { http: reqwest::Client::new(), http_url, ws_url })
	}

	/// POST a typed GraphQL query/mutation and return its (validated) response data.
	async fn run_query<Q: GraphQLQuery>(
		&self,
		variables: Q::Variables,
	) -> IndexerResult<Q::ResponseData> {
		let body = Q::build_query(variables);
		let resp = self.http.post(&self.http_url).json(&body).send().await?;
		let resp: graphql_client::Response<Q::ResponseData> =
			resp.error_for_status()?.json().await?;

		if let Some(errors) = resp.errors.filter(|e| !e.is_empty()) {
			let msg = errors.iter().map(|e| e.message.clone()).collect::<Vec<_>>().join(", ");
			return Err(IndexerClientError::GraphQl(msg));
		}
		resp.data
			.ok_or_else(|| IndexerClientError::Malformed("missing `data` in response".into()))
	}

	/// `connect(viewingKey, options)` — establish a wallet session, returns the session id.
	pub async fn connect(
		&self,
		viewing_key: &str,
		start_index: Option<u64>,
	) -> IndexerResult<String> {
		let variables = connect::Variables {
			viewing_key: viewing_key.to_owned(),
			options: Some(connect::ConnectOptions { start_index: start_index.map(|i| i as i64) }),
		};
		Ok(self.run_query::<Connect>(variables).await?.connect)
	}

	/// `disconnect(sessionId)` — release a wallet session. Best-effort; errors are surfaced.
	pub async fn disconnect(&self, session_id: &str) -> IndexerResult<()> {
		let variables = disconnect::Variables { session_id: session_id.to_owned() };
		self.run_query::<Disconnect>(variables).await?;
		Ok(())
	}

	/// `block` (no offset) — the latest indexed block.
	pub async fn latest_block(&self) -> IndexerResult<BlockInfo> {
		let data = self.run_query::<LatestBlock>(latest_block::Variables {}).await?;
		let block = data
			.block
			.ok_or_else(|| IndexerClientError::Malformed("no block returned".into()))?;
		Ok(BlockInfo {
			height: block.height as u64,
			timestamp: block.timestamp as u64,
			zswap_end_index: block.zswap_end_index as u64,
			ledger_parameters: decode_hex(&block.ledger_parameters)?,
		})
	}

	/// Open the `shieldedTransactions` subscription starting at zswap `index`.
	pub async fn shielded_transactions(
		&self,
		session_id: &str,
		index: u64,
	) -> IndexerResult<ShieldedStream> {
		let variables = shielded_transactions::Variables {
			session_id: session_id.to_owned(),
			index: Some(index as i64),
		};
		Ok(ShieldedStream(self.open_subscription::<ShieldedTransactions>(variables).await?))
	}

	/// Open the `unshieldedTransactions` subscription for `address`.
	pub async fn unshielded_transactions(
		&self,
		address: &str,
		transaction_id: u64,
	) -> IndexerResult<UnshieldedStream> {
		let variables = unshielded_transactions::Variables {
			address: address.to_owned(),
			transaction_id: Some(transaction_id as i64),
		};
		Ok(UnshieldedStream(self.open_subscription::<UnshieldedTransactions>(variables).await?))
	}

	/// Open the `dustLedgerEvents` subscription starting at ledger-event `id`.
	pub async fn dust_ledger_events(&self, id: u64) -> IndexerResult<DustStream> {
		let variables = dust_ledger_events::Variables { id: Some(id as i64) };
		Ok(DustStream(self.open_subscription::<DustLedgerEvents>(variables).await?))
	}

	/// Open a `graphql-transport-ws` subscription for a typed operation: connect, perform the init
	/// handshake, and send the `subscribe` message built from the typed query.
	async fn open_subscription<Q: GraphQLQuery>(
		&self,
		variables: Q::Variables,
	) -> IndexerResult<SubscriptionStream> {
		let body = Q::build_query(variables);
		let payload = json!({
			"operationName": body.operation_name,
			"query": body.query,
			"variables": body.variables,
		});

		let mut request = self
			.ws_url
			.as_str()
			.into_client_request()
			.map_err(|e| IndexerClientError::WebSocket(format!("build ws request: {e}")))?;
		let proto = "graphql-transport-ws"
			.parse()
			.map_err(|e| IndexerClientError::WebSocket(format!("parse subprotocol: {e}")))?;
		request.headers_mut().insert("Sec-WebSocket-Protocol", proto);

		let (mut ws, _) = connect_async(request)
			.await
			.map_err(|e| IndexerClientError::WebSocket(format!("connect: {e}")))?;

		// connection_init → connection_ack handshake.
		ws.send(Message::text(json!({ "type": "connection_init" }).to_string()))
			.await
			.map_err(|e| IndexerClientError::WebSocket(format!("send connection_init: {e}")))?;
		loop {
			match ws.next().await {
				Some(Ok(Message::Text(text))) => {
					let msg: Value = serde_json::from_str(&text).map_err(|e| {
						IndexerClientError::Malformed(format!("ack json: {e}: {text}"))
					})?;
					match msg.get("type").and_then(Value::as_str) {
						Some("connection_ack") => break,
						Some(other) => {
							return Err(IndexerClientError::WebSocket(format!(
								"expected connection_ack, got {other}"
							)));
						},
						None => {
							return Err(IndexerClientError::Malformed(
								"ack message without `type`".into(),
							));
						},
					}
				},
				Some(Ok(_)) => continue,
				Some(Err(e)) => {
					return Err(IndexerClientError::WebSocket(format!("awaiting ack: {e}")));
				},
				None => {
					return Err(IndexerClientError::WebSocket(
						"connection closed before connection_ack".into(),
					));
				},
			}
		}

		let subscribe = json!({ "type": "subscribe", "id": "1", "payload": payload });
		ws.send(Message::text(subscribe.to_string()))
			.await
			.map_err(|e| IndexerClientError::WebSocket(format!("send subscribe: {e}")))?;

		Ok(SubscriptionStream { ws })
	}
}

/// A live `graphql-transport-ws` subscription. Yields the GraphQL `data` object per `next`
/// message and ends (`None`) on `complete`. Dropping it closes the socket.
pub struct SubscriptionStream {
	ws: WebSocketStream<MaybeTlsStream<TcpStream>>,
}

impl SubscriptionStream {
	/// Read the next `data` payload, or `None` once the server sends `complete`/closes.
	async fn next_data(&mut self) -> Option<IndexerResult<Value>> {
		loop {
			match self.ws.next().await {
				Some(Ok(Message::Text(text))) => {
					let msg: Value = match serde_json::from_str(&text) {
						Ok(v) => v,
						Err(e) => {
							return Some(Err(IndexerClientError::Malformed(format!(
								"server message json: {e}: {text}"
							))));
						},
					};
					match msg.get("type").and_then(Value::as_str) {
						Some("next") => {
							let payload = &msg["payload"];
							if let Some(errors) = payload.get("errors").filter(|e| !e.is_null()) {
								return Some(Err(IndexerClientError::GraphQl(errors.to_string())));
							}
							match payload.get("data") {
								Some(data) if !data.is_null() => return Some(Ok(data.clone())),
								_ => {
									return Some(Err(IndexerClientError::Malformed(
										"`next` without data".into(),
									)));
								},
							}
						},
						Some("complete") => return None,
						Some("error") => {
							return Some(Err(IndexerClientError::GraphQl(
								msg["payload"].to_string(),
							)));
						},
						// ping/pong and other control messages: keep reading.
						_ => continue,
					}
				},
				// tungstenite answers control frames internally; ignore non-text data frames.
				Some(Ok(_)) => continue,
				Some(Err(e)) => return Some(Err(IndexerClientError::WebSocket(e.to_string()))),
				None => return None,
			}
		}
	}
}

/// Typed view over the `shieldedTransactions` subscription.
pub struct ShieldedStream(SubscriptionStream);

impl ShieldedStream {
	pub async fn next(&mut self) -> Option<IndexerResult<ShieldedEvent>> {
		let data = self.0.next_data().await?;
		Some(data.and_then(|v| {
			let resp: shielded_transactions::ResponseData = serde_json::from_value(v)
				.map_err(|e| IndexerClientError::Malformed(format!("shieldedTransactions: {e}")))?;
			map_shielded(resp.shielded_transactions)
		}))
	}
}

/// Typed view over the `unshieldedTransactions` subscription.
pub struct UnshieldedStream(SubscriptionStream);

impl UnshieldedStream {
	pub async fn next(&mut self) -> Option<IndexerResult<UnshieldedEvent>> {
		let data = self.0.next_data().await?;
		Some(data.and_then(|v| {
			let resp: unshielded_transactions::ResponseData =
				serde_json::from_value(v).map_err(|e| {
					IndexerClientError::Malformed(format!("unshieldedTransactions: {e}"))
				})?;
			map_unshielded(resp.unshielded_transactions)
		}))
	}
}

/// Typed view over the `dustLedgerEvents` subscription.
pub struct DustStream(SubscriptionStream);

impl DustStream {
	pub async fn next(&mut self) -> Option<IndexerResult<DustLedgerEventData>> {
		let data = self.0.next_data().await?;
		Some(data.and_then(|v| {
			let resp: dust_ledger_events::ResponseData = serde_json::from_value(v)
				.map_err(|e| IndexerClientError::Malformed(format!("dustLedgerEvents: {e}")))?;
			let e = resp.dust_ledger_events;
			Ok(DustLedgerEventData {
				id: e.id as u64,
				max_id: e.max_id as u64,
				raw: decode_hex(&e.raw)?,
			})
		}))
	}
}

fn map_shielded(
	node: shielded_transactions::ShieldedTransactionsShieldedTransactions,
) -> IndexerResult<ShieldedEvent> {
	use shielded_transactions::ShieldedTransactionsShieldedTransactions as Node;
	use shielded_transactions::TransactionResultStatus as Status;
	match node {
		Node::RelevantTransaction(rt) => {
			let tx = rt.transaction;
			let result = match tx.transaction_result.status {
				Status::SUCCESS => TransactionResultKind::Success,
				Status::PARTIAL_SUCCESS => TransactionResultKind::PartialSuccess,
				// FAILURE and any future/unknown status → apply nothing.
				_ => TransactionResultKind::Failure,
			};
			Ok(ShieldedEvent::Relevant {
				raw_transaction: decode_hex(&tx.raw)?,
				result,
				zswap_start_index: tx.zswap_start_index as u64,
				zswap_end_index: tx.zswap_end_index as u64,
				collapsed_update: rt
					.zswap_collapsed_update
					.map(|u| decode_hex(&u.update))
					.transpose()?,
			})
		},
		Node::ShieldedTransactionsProgress(p) => Ok(ShieldedEvent::Progress {
			highest_end_index: p.highest_zswap_end_index as u64,
			highest_checked_end_index: p.highest_checked_zswap_end_index as u64,
			highest_relevant_end_index: p.highest_relevant_zswap_end_index as u64,
		}),
	}
}

fn map_unshielded(
	node: unshielded_transactions::UnshieldedTransactionsUnshieldedTransactions,
) -> IndexerResult<UnshieldedEvent> {
	use unshielded_transactions::UnshieldedTransactionsUnshieldedTransactions as Node;
	match node {
		Node::UnshieldedTransaction(t) => Ok(UnshieldedEvent::Transaction {
			transaction_id: t.transaction.id as u64,
			created: t
				.created_utxos
				.into_iter()
				.map(map_unshielded_utxo)
				.collect::<IndexerResult<_>>()?,
			spent: t
				.spent_utxos
				.into_iter()
				.map(map_unshielded_utxo)
				.collect::<IndexerResult<_>>()?,
		}),
		Node::UnshieldedTransactionsProgress(p) => Ok(UnshieldedEvent::Progress {
			highest_transaction_id: p.highest_transaction_id as u64,
		}),
	}
}

fn map_unshielded_utxo(
	u: unshielded_transactions::UnshieldedTransactionsUnshieldedTransactionsOnUnshieldedTransactionCreatedUtxos,
) -> IndexerResult<UnshieldedUtxoData> {
	Ok(UnshieldedUtxoData {
		token_type: decode_hex(&u.token_type)?,
		value: u
			.value
			.parse::<u128>()
			.map_err(|e| IndexerClientError::Decode(format!("utxo value: {e}")))?,
		intent_hash: decode_hex(&u.intent_hash)?,
		output_index: u.output_index as u32,
		ctime: u.ctime.map(|c| c as u64),
	})
}

/// Decode a `HexEncoded` scalar, tolerating an optional `0x`/`0X` prefix.
fn decode_hex(s: &str) -> IndexerResult<Vec<u8>> {
	let s = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
	hex::decode(s).map_err(|e| IndexerClientError::Decode(format!("hex: {e}")))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn new_derives_graphql_and_ws_urls() {
		let c = IndexerClient::new("http://127.0.0.1:8088/api/v4").unwrap();
		assert_eq!(c.http_url, "http://127.0.0.1:8088/api/v4/graphql");
		assert_eq!(c.ws_url, "ws://127.0.0.1:8088/api/v4/graphql/ws");
	}

	#[test]
	fn new_handles_trailing_slash_and_https() {
		let c = IndexerClient::new("https://indexer.example/api/v4/").unwrap();
		assert_eq!(c.http_url, "https://indexer.example/api/v4/graphql");
		assert_eq!(c.ws_url, "wss://indexer.example/api/v4/graphql/ws");
	}

	#[test]
	fn new_rejects_non_http_scheme() {
		assert!(IndexerClient::new("ftp://nope/api/v4").is_err());
	}

	#[test]
	fn decode_hex_tolerates_prefix() {
		assert_eq!(decode_hex("0x0102").unwrap(), vec![1, 2]);
		assert_eq!(decode_hex("0102").unwrap(), vec![1, 2]);
	}

	/// The schema path the derives compile against must exist (the indexer submodule must be
	/// checked out). This makes the requirement explicit rather than only a derive-macro error.
	#[test]
	fn schema_path_exists() {
		let path = concat!(
			env!("CARGO_MANIFEST_DIR"),
			"/../../indexer/indexer-api/graphql/schema-v4.graphql"
		);
		assert!(
			std::path::Path::new(path).exists(),
			"indexer schema not found at {path} — is the `indexer` submodule checked out?"
		);
	}

	/// Maps a `ShieldedTransactionsProgress` JSON payload through the generated types, pinning the
	/// query field names against the schema.
	#[test]
	fn map_shielded_progress_from_json() {
		let v = json!({
			"shieldedTransactions": {
				"__typename": "ShieldedTransactionsProgress",
				"highestZswapEndIndex": 10,
				"highestCheckedZswapEndIndex": 10,
				"highestRelevantZswapEndIndex": 4,
			}
		});
		let resp: shielded_transactions::ResponseData = serde_json::from_value(v).unwrap();
		match map_shielded(resp.shielded_transactions).unwrap() {
			ShieldedEvent::Progress { highest_end_index, highest_relevant_end_index, .. } => {
				assert_eq!(highest_end_index, 10);
				assert_eq!(highest_relevant_end_index, 4);
			},
			_ => panic!("expected progress"),
		}
	}

	#[test]
	fn map_unshielded_transaction_from_json() {
		let v = json!({
			"unshieldedTransactions": {
				"__typename": "UnshieldedTransaction",
				"transaction": { "__typename": "RegularTransaction", "id": 7 },
				"createdUtxos": [{
					"tokenType": "00",
					"value": "1000",
					"intentHash": "11",
					"outputIndex": 0,
					"ctime": 123,
				}],
				"spentUtxos": [],
			}
		});
		let resp: unshielded_transactions::ResponseData = serde_json::from_value(v).unwrap();
		match map_unshielded(resp.unshielded_transactions).unwrap() {
			UnshieldedEvent::Transaction { transaction_id, created, spent } => {
				assert_eq!(transaction_id, 7);
				assert_eq!(created.len(), 1);
				assert_eq!(created[0].value, 1000);
				assert_eq!(created[0].ctime, Some(123));
				assert!(spent.is_empty());
			},
			_ => panic!("expected transaction"),
		}
	}
}
