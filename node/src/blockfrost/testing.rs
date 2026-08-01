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

//! Shared test scaffolding: a minimal HTTP server built on `tokio::net` so the client
//! tests need no mocking crate, plus fixtures used by more than one module's tests.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use cardano_serialization_lib::{BigNum, ConstrPlutusData, PlutusData, PlutusList};

use super::client::BlockfrostClient;

/// One canned reply from the fake server.
pub(crate) enum Reply {
	Json(u16, String),
	/// Reply only after a delay, to exercise a deadline across several attempts.
	Slow(Duration, u16, String),
}

/// Serves `replies` in order (repeating the last once exhausted) and records the
/// request line of everything it received. Built on `tokio::net`, which the workspace
/// already depends on, so this needs no HTTP-mocking crate.
pub(crate) async fn fake_server(replies: Vec<Reply>) -> (String, Arc<Mutex<Vec<String>>>) {
	let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.expect("bind");
	let addr = listener.local_addr().expect("local addr");
	let seen: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
	let recorder = seen.clone();
	tokio::spawn(async move {
		use tokio::io::{AsyncReadExt, AsyncWriteExt};
		let mut served = 0usize;
		while let Ok((mut socket, _)) = listener.accept().await {
			let mut buf = [0u8; 8192];
			let read = socket.read(&mut buf).await.unwrap_or(0);
			let request = String::from_utf8_lossy(&buf[..read]).to_string();
			recorder
				.lock()
				.expect("recorder")
				.push(request.lines().next().unwrap_or_default().to_string());
			let reply = match replies.get(served).or_else(|| replies.last()) {
				Some(reply) => reply,
				None => return,
			};
			served += 1;
			let (status, body) = match reply {
				Reply::Json(status, body) => (*status, body.clone()),
				Reply::Slow(delay, status, body) => {
					tokio::time::sleep(*delay).await;
					(*status, body.clone())
				},
			};
			// `connection: close` keeps one request per connection, so the recorded
			// count is the number of requests the client actually made.
			let response = format!(
				"HTTP/1.1 {status} S\r\ncontent-type: application/json\r\n\
				 content-length: {}\r\nconnection: close\r\n\r\n{body}",
				body.len()
			);
			let _ = socket.write_all(response.as_bytes()).await;
		}
	});
	(format!("http://{addr}"), seen)
}

pub(crate) fn client_at(url: &str) -> BlockfrostClient {
	BlockfrostClient::new(url, None, 432).expect("client")
}

/// `count` asset-transaction rows, ascending from `first_block`.
pub(crate) fn asset_txs_json(count: usize, first_block: u32) -> String {
	let rows: Vec<String> = (0..count)
		.map(|i| {
			let height = first_block as usize + i;
			format!(
				r#"{{"tx_hash":"{:064x}","tx_index":0,"block_height":{height},"block_time":{height}}}"#,
				height
			)
		})
		.collect();
	format!("[{}]", rows.join(","))
}

/// Blockfrost's documented error body shape.
pub(crate) fn over_quota_body() -> String {
	r#"{"status_code":402,"error":"Payment Required","message":"Usage is over limit."}"#.to_string()
}

/// `[Credential(key hash), DustPublicKey]` registration datum, as inline CBOR hex.
pub(crate) fn registration_datum_hex(owner_key_hash: [u8; 28], dust_key: [u8; 32]) -> String {
	let mut credential_fields = PlutusList::new();
	credential_fields.add(&PlutusData::new_bytes(owner_key_hash.to_vec()));
	let credential = PlutusData::new_constr_plutus_data(&ConstrPlutusData::new(
		&BigNum::zero(),
		&credential_fields,
	));
	let mut fields = PlutusList::new();
	fields.add(&credential);
	fields.add(&PlutusData::new_bytes(dust_key.to_vec()));
	PlutusData::new_constr_plutus_data(&ConstrPlutusData::new(&BigNum::zero(), &fields)).to_hex()
}
