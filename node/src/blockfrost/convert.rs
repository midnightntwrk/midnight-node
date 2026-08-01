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

//! Small pure conversions shared by more than one data source. No I/O here.

use blockfrost::blockfrost_openapi::models::{
	block_content::BlockContent, tx_content_output_amount_inner::TxContentOutputAmountInner,
};

use super::support::BoxError;

pub(crate) fn decode_hash32(hex_str: &str) -> Result<[u8; 32], BoxError> {
	let bytes = hex::decode(hex_str)?;
	bytes
		.try_into()
		.map_err(|_| format!("expected 32-byte hash, got {hex_str}").into())
}

pub(crate) fn block_height(b: &BlockContent) -> Result<u32, BoxError> {
	Ok(u32::try_from(b.height.ok_or("block has no height")?)?)
}

/// Whether the amount list carries `unit` at all. Cardano outputs never hold a
/// zero quantity of an asset, so presence is equivalent to a positive amount, and
/// this avoids parsing a quantity that is not needed.
pub(crate) fn has_unit(amounts: &[TxContentOutputAmountInner], unit: &str) -> bool {
	amounts.iter().any(|a| a.unit == unit)
}

/// Sum of `unit` in an amount list. A unit appears at most once per input/output,
/// but summing is harmless and matches SQL `SUM(quantity)` semantics.
///
/// A quantity that does not parse is an error rather than a zero: db-sync rejects
/// such a value, so silently dropping it here would put different data into the
/// inherent than a db-sync backed node produces.
pub(crate) fn amount_of(
	amounts: &[TxContentOutputAmountInner],
	unit: &str,
) -> Result<u128, BoxError> {
	let mut total: u128 = 0;
	for amount in amounts.iter().filter(|a| a.unit == unit) {
		let quantity: u128 = amount
			.quantity
			.parse()
			.map_err(|e| format!("invalid quantity {:?} for unit {unit}: {e}", amount.quantity))?;
		total = total.checked_add(quantity).ok_or("asset quantity sum overflowed u128")?;
	}
	Ok(total)
}
