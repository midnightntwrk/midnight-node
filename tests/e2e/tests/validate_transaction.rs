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

//! `midnight_validateTransaction`: takes hex tx bytes and returns either the tx_hash or one of
//! three JSON-RPC errors: -32602 bad hex, -32001 validation failure with structured
//! `data.{error_code, reason, details}`, or -32005 rate-limited (per-tx cooldown or global).

use midnight_node_e2e::api::midnight::MidnightClient;
use midnight_node_e2e::config::Settings;
use midnight_node_e2e::e2e_test;

/// Random bytes: valid hex, so they pass the hex-decode gate, but never a valid tx. Fresh per
/// test so the per-tx cooldown key (blake2_256 of the bytes) can't collide across tests.
fn junk_tx_hex() -> String {
    let junk: [u8; 100] = rand::random();
    hex::encode(junk)
}

/// Bad hex is rejected before the rate limiter is touched, so this can't pollute the per-tx
/// cooldown for any other test.
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

#[e2e_test]
async fn validate_transaction_per_tx_rate_limit_blocks_replay() {
    let settings = Settings::default();
    let client = MidnightClient::new(settings.node_client).await;
    let tx_hex = junk_tx_hex();

    let first_msg = client
        .validate_transaction(&tx_hex)
        .await
        .expect_err("random bytes must not validate")
        .to_string();
    assert!(
        !first_msg.contains("-32005"),
        "first call should fail on validation, not rate limit: {first_msg}"
    );

    let second_msg = client
        .validate_transaction(&tx_hex)
        .await
        .expect_err("second call with same hex must be rate-limited")
        .to_string();
    assert!(
        second_msg.contains("-32005") || second_msg.to_lowercase().contains("rate limit"),
        "expected -32005 per-tx rate limit, got: {second_msg}"
    );
    assert!(
        second_msg.to_lowercase().contains("per-transaction")
            || second_msg.to_lowercase().contains("cooldown"),
        "expected per-transaction cooldown message, got: {second_msg}"
    );
}
