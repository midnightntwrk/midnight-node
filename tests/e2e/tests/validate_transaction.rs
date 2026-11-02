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

//! `midnight_validateTransaction`: takes hex tx bytes and returns either the tx_hash or a JSON-RPC
//! error: -32602 bad hex, or -32001 validation failure with structured
//! `data.{error_code, reason, details}`.

use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;

/// Random bytes: valid hex, so they pass the hex-decode gate, but never a valid tx.
fn junk_tx_hex() -> String {
    let junk: [u8; 100] = rand::random();
    hex::encode(junk)
}

#[e2e_test]
async fn validate_transaction_rejects_invalid_hex() {
    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    let err = client
        .validate_transaction("zz_not_hex")
        .await
        .expect_err("expected -32602 for invalid hex, got Ok");
    let msg = err.to_string();
    assert!(
        msg.contains("-32602") || msg.to_lowercase().contains("invalid hex"),
        "expected invalid-hex error, got: {msg}"
    );
}

#[e2e_test]
async fn validate_transaction_failure_carries_structured_details() {
    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;

    let err = client
        .validate_transaction(&junk_tx_hex())
        .await
        .expect_err("random bytes must not validate");
    // subxt's `UserError` Display renders only `{message} ({code})`, dropping `data`.
    let subxt::rpcs::Error::User(user_err) = &err else {
        panic!("expected JSON-RPC user error, got: {err}");
    };
    assert_eq!(
        user_err.code, -32001,
        "expected -32001 validation failure, got: {user_err}"
    );
    let data: serde_json::Value = serde_json::from_str(
        user_err
            .data
            .as_ref()
            .expect("validation failure must carry data")
            .get(),
    )
    .expect("data must be JSON");
    for field in ["error_code", "reason", "details"] {
        assert!(
            data.get(field).is_some(),
            "missing `{field}` in error data: {data}"
        );
    }
}
