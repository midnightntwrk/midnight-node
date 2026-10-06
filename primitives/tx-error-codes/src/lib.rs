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

//! The shared list of `InvalidTransaction::Custom(u8)` codes the Midnight runtime returns.
//!
//! When the runtime rejects a transaction with a custom code, the byte is all a client
//! gets, so every producer must draw from one list. Two kinds of producer share the byte:
//!
//! * the ledger adapter (`midnight-node-ledger`), whose `From<LedgerApiError> for u8` maps
//!   every ledger error onto a code. Those codes are nested enum variants, not
//!   discriminants, so this crate records only their bounds: the ledger owns
//!   `0..=`[`LEDGER_LAST`] and [`HOST_API`]. Holes inside that range, including codes the
//!   ledger has retired, stay reserved for the ledger.
//! * node-side pallets and transaction extensions, whose codes are the discriminants of
//!   [`NodeTxCode`]. They are allocated downward from [`HOST_API`], and the exhaustive
//!   match in `From<NodeTxCode> for u8` checks at compile time that each one is outside
//!   the ledger's allocation. `#[repr(u8)]` makes two variants with one code a compile
//!   error.
//!
//! This crate is `no_std` and has no dependencies, so the ledger adapter and every pallet
//! can depend on it without depending on each other. The ledger adapter tests that each
//! code it produces satisfies [`is_ledger_code`].
//!
//! # Adding a node-side code
//!
//! Prefer a built-in `InvalidTransaction` variant (`Call`, `ExhaustsResources`, `Payment`,
//! `Stale`, …) when one fits. Those render as readable messages and cost nothing here.
//! Reach for a custom code only when the SDK has no name for the condition. Then add a
//! variant to [`NodeTxCode`] with the next code below the lowest one in use, and document
//! it. The crate does not compile until the variant also has its arm in
//! `From<NodeTxCode> for u8`. Codes are observable to clients, so a code that has shipped
//! is never renumbered or reused: keep the variant and mark it retired in its doc.

#![no_std]

/// Highest code the ledger adapter allocates from, apart from [`HOST_API`].
///
/// The ledger owns `0..=LEDGER_LAST`; node codes are allocated downward from [`HOST_API`].
pub const LEDGER_LAST: u8 = 250;

/// `LedgerApiError::HostApiError`: an error in the host API rather than the ledger itself.
/// It sat at the top of the byte before this list existed and stays there.
pub const HOST_API: u8 = 255;

const _: () = assert!(LEDGER_LAST < HOST_API);

/// Whether `code` belongs to the ledger adapter's allocation: `0..=LEDGER_LAST` or
/// [`HOST_API`]. Every other code is free for [`NodeTxCode`].
pub const fn is_ledger_code(code: u8) -> bool {
	code <= LEDGER_LAST || code == HOST_API
}

/// A node-side rejection reason. The discriminant is the `InvalidTransaction::Custom` code
/// a client sees; the doc comment is its meaning.
///
/// Convert with `InvalidTransaction::Custom(NodeTxCode::Variant.into())`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeTxCode {
	/// `pallet_throttle::CheckThrottle`: a signed transaction would take its account over
	/// the per-window byte or transaction-count limit.
	///
	/// Distinct from `InvalidTransaction::ExhaustsResources`, which is the per-transaction
	/// or per-block weight limit. The sender should wait for the throttle window to reset
	/// rather than make the transaction smaller.
	ThrottleLimitExceeded = 254,
}

impl From<NodeTxCode> for u8 {
	fn from(code: NodeTxCode) -> u8 {
		// Exhaustive on purpose: a new variant does not compile until it has an arm here,
		// and the arm's `const` block rejects a code inside the ledger's allocation at
		// compile time.
		match code {
			NodeTxCode::ThrottleLimitExceeded => {
				const { node_code(NodeTxCode::ThrottleLimitExceeded) }
			},
		}
	}
}

/// The code of `code`, failing const evaluation if it collides with the ledger's allocation.
const fn node_code(code: NodeTxCode) -> u8 {
	let code = code as u8;
	assert!(!is_ledger_code(code), "NodeTxCode collides with the ledger's allocation");
	code
}
