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

use super::*;
use common::types::Hash;

/// Version-agnostic validation error returned across the native API boundary.
#[derive(Debug)]
pub struct ValidationError {
	/// The LedgerApiError encoded as a u8 error code.
	pub error_code: u8,
	/// Human-readable Display of the LedgerApiError.
	pub reason: String,
	/// Debug representation of the original ledger error (before lossy conversion).
	pub details: String,
}

type Ledger8Sig = ledger_8::base_crypto_local::signatures::Signature;
type Ledger8Db = ledger_8::ledger_storage_local::db::ParityDb;
type Ledger8Bridge = ledger_8::Bridge<Ledger8Sig, Ledger8Db>;

type Ledger7Sig = ledger_7::base_crypto_local::signatures::Signature;
type Ledger7Db = ledger_7::ledger_storage_local::db::ParityDb;
type Ledger7Bridge = ledger_7::Bridge<Ledger7Sig, Ledger7Db>;

fn ledger_7_version() -> Vec<u8> {
	Ledger7Bridge::get_version()
}

fn ledger_8_version() -> Vec<u8> {
	Ledger8Bridge::get_version()
}

pub fn validate_transaction_verbose(
	runtime_ledger_version: &[u8],
	state_key: &[u8],
	tx: &[u8],
	block_context: latest::BlockContext,
	runtime_version: u32,
	max_weight: u64,
) -> Result<Hash, ValidationError> {
	if runtime_ledger_version == ledger_8_version() {
		Ledger8Bridge::validate_transaction_verbose(
			state_key,
			tx,
			block_context,
			runtime_version,
			max_weight,
		)
		.map_err(|e| {
			let reason = format!("{}", e.error);
			let error_code: u8 = e.error.into();
			ValidationError { error_code, reason, details: e.details }
		})
	} else if runtime_ledger_version == ledger_7_version() {
		let l7_ctx = ledger_7::BlockContext {
			tblock: block_context.tblock,
			tblock_err: block_context.tblock_err,
			parent_block_hash: block_context.parent_block_hash,
		};
		Ledger7Bridge::validate_transaction_verbose(
			state_key,
			tx,
			l7_ctx,
			runtime_version,
			max_weight,
		)
		.map_err(|e| {
			let reason = format!("{}", e.error);
			let error_code: u8 = e.error.into();
			ValidationError { error_code, reason, details: e.details }
		})
	} else {
		Err(ValidationError {
			error_code: 151, // NoLedgerState
			reason: "Unsupported ledger version".into(),
			details: format!(
				"Unsupported ledger version: {}",
				String::from_utf8_lossy(runtime_ledger_version)
			),
		})
	}
}
