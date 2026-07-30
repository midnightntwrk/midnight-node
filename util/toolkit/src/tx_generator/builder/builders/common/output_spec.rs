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

//! Per-destination output specs for `generate-txs single-tx` and friends.
//!
//! Resolves the two CLI shapes into [`ShieldedOutputSpec`] / [`UnshieldedOutputSpec`]
//! lists that the offer / intent builders consume:
//!
//!   * Bundled `--output` triples: each occurrence is already a complete
//!     `(address, amount, token)` triple; just consumed by
//!     [`resolve_outputs_from_triples`].
//!   * Parallel `--destination-address` + per-side `--*-amount` / `--*-token-type`
//!     flags: normalised into `Vec<OutputArg>` by [`legacy_to_output_args`],
//!     then handed off to [`resolve_outputs_from_triples`]. This keeps a single
//!     code path for HRP classification and OutputSpec construction.
//!
//! Living in its own module lets both `single_tx` and `batch_single_tx` share
//! the spec types without one of them reaching into the other.

use super::ledger_helpers_local::{
	DefaultDB, ShieldedTokenType, ShieldedWallet, UnshieldedTokenType, UnshieldedWallet,
};

use crate::{cli_parsers::OutputArg, tx_generator::builder::SingleTxArgs};

/// Per-destination shielded output spec, after CLI argument resolution
/// (broadcast / index alignment) has produced one entry per destination.
pub(crate) struct ShieldedOutputSpec<D: super::ledger_helpers_local::DB + Clone> {
	pub wallet: ShieldedWallet<D>,
	pub amount: u128,
	pub token_type: ShieldedTokenType,
}

/// Per-destination unshielded output spec, after CLI argument resolution.
pub(crate) struct UnshieldedOutputSpec {
	pub wallet: UnshieldedWallet,
	pub amount: u128,
	pub token_type: UnshieldedTokenType,
}

/// Shallow-clone a `ShieldedOutputSpec`. Manual because `ShieldedWallet<D>`
/// derives `Clone` only via `derive_where`, which doesn't compose with a plain
/// `#[derive(Clone)]` on a wrapping struct that's generic over `D`.
pub(crate) fn clone_shielded_spec(
	spec: &ShieldedOutputSpec<DefaultDB>,
) -> ShieldedOutputSpec<DefaultDB> {
	ShieldedOutputSpec {
		wallet: spec.wallet.clone(),
		amount: spec.amount,
		token_type: spec.token_type,
	}
}

/// Shallow-clone an `UnshieldedOutputSpec`. Manual for symmetry with
/// [`clone_shielded_spec`].
pub(crate) fn clone_unshielded_spec(spec: &UnshieldedOutputSpec) -> UnshieldedOutputSpec {
	UnshieldedOutputSpec {
		wallet: spec.wallet.clone(),
		amount: spec.amount,
		token_type: spec.token_type,
	}
}

/// Side of a destination address, paired with the side-local index used to
/// pull the matching amount / token-type from the parallel CLI vectors.
enum Side {
	Shielded(usize),
	Unshielded(usize),
}

/// Translate the parallel-flag CLI shape (`--destination-address` plus per-side
/// `--*-amount` / `--*-token-type` lists) into the unified
/// [`OutputArg`] representation. The bundled-triple path then consumes the
/// result via [`resolve_outputs_from_triples`].
///
/// Per-side rules (independent for shielded vs unshielded):
///   * Amount list must be length 1 (broadcast) or N (per-destination on that side).
///   * Token-type list must be length 0 (default all-zeros), 1, or N.
///   * Mismatched counts panic with a clear message.
pub(crate) fn legacy_to_output_args(args: &SingleTxArgs) -> Vec<OutputArg> {
	use super::type_convert::convert_wallet_address;

	// First pass: classify each destination by side, remembering its
	// side-local index so the matching parallel-flag entry can be picked.
	let mut sides: Vec<Side> = Vec::with_capacity(args.destination_address.len());
	let mut n_shielded = 0usize;
	let mut n_unshielded = 0usize;
	for (idx, addr) in args.destination_address.iter().enumerate() {
		let local_addr = convert_wallet_address(addr);
		if <ShieldedWallet<DefaultDB>>::try_from(&local_addr).is_ok() {
			sides.push(Side::Shielded(n_shielded));
			n_shielded += 1;
		} else if UnshieldedWallet::try_from(&local_addr).is_ok() {
			sides.push(Side::Unshielded(n_unshielded));
			n_unshielded += 1;
		} else {
			log::error!(
				"destination address at position {} does not parse as shielded or unshielded: {:?}",
				idx,
				addr
			);
			panic!("destination_address parse error");
		}
	}

	validate_legacy_lengths(
		"shielded",
		n_shielded,
		args.shielded_amount.len(),
		args.shielded_token_type.len(),
	);
	validate_legacy_lengths(
		"unshielded",
		n_unshielded,
		args.unshielded_amount.len(),
		args.unshielded_token_type.len(),
	);

	// Second pass: build OutputArg for each destination using its side-local index.
	args.destination_address
		.iter()
		.zip(sides.iter())
		.map(|(addr, side)| {
			let (amount, token_type) = match side {
				Side::Shielded(i) => (
					pick(&args.shielded_amount, *i),
					pick_opt(&args.shielded_token_type, *i).map(|tt| tt.0.0),
				),
				Side::Unshielded(i) => (
					pick(&args.unshielded_amount, *i),
					pick_opt(&args.unshielded_token_type, *i).map(|tt| tt.0.0),
				),
			};
			OutputArg { address: addr.clone(), amount, token_type }
		})
		.collect()
}

/// Read the per-destination value from a broadcast-or-aligned vector. Caller
/// must have already validated the length with [`validate_legacy_lengths`].
fn pick<T: Copy>(values: &[T], i: usize) -> T {
	if values.len() == 1 { values[0] } else { values[i] }
}

/// Same as [`pick`], but tolerates an empty vector by returning `None`. Used
/// for optional token-type lists where omission means "use the default".
fn pick_opt<T: Copy>(values: &[T], i: usize) -> Option<T> {
	if values.is_empty() {
		None
	} else if values.len() == 1 {
		Some(values[0])
	} else {
		Some(values[i])
	}
}

fn validate_legacy_lengths(side: &str, n: usize, amount_len: usize, token_len: usize) {
	if n == 0 {
		if amount_len > 0 {
			log::warn!(
				"--{side}-amount was provided ({amount_len} value(s)) but no {side} destinations were given; ignoring"
			);
		}
		if token_len > 0 {
			log::warn!(
				"--{side}-token-type was provided ({token_len} value(s)) but no {side} destinations were given; ignoring"
			);
		}
		return;
	}

	if amount_len == 0 {
		log::error!("--{side}-amount is required when at least one {side} destination is given");
		panic!("missing --{side}-amount");
	}

	if amount_len != 1 && amount_len != n {
		log::error!(
			"--{side}-amount must be provided once (broadcast) or exactly {n} time(s) to match destinations; got {amount_len}"
		);
		panic!("--{side}-amount length mismatch");
	}

	if token_len > 1 && token_len != n {
		log::error!(
			"--{side}-token-type must be omitted, provided once (broadcast), or exactly {n} time(s) to match destinations; got {token_len}"
		);
		panic!("--{side}-token-type length mismatch");
	}
}

/// Resolve outputs from the bundled-triple CLI shape (`--output addr=...,
/// amount=...[,token=...]`). The address HRP picks shielded vs unshielded;
/// an omitted token defaults to the all-zeros token type.
pub(crate) fn resolve_outputs_from_triples(
	triples: &[OutputArg],
) -> (Vec<ShieldedOutputSpec<DefaultDB>>, Vec<UnshieldedOutputSpec>) {
	use super::type_convert::*;

	let mut shielded_outputs: Vec<ShieldedOutputSpec<DefaultDB>> = Vec::new();
	let mut unshielded_outputs: Vec<UnshieldedOutputSpec> = Vec::new();

	for (idx, triple) in triples.iter().enumerate() {
		let local_addr = convert_wallet_address(&triple.address);
		if let Ok(sw) = <ShieldedWallet<DefaultDB>>::try_from(&local_addr) {
			let token_type = match triple.token_type {
				Some(bytes) => {
					convert_shielded_token_type(midnight_node_ledger_helpers::ShieldedTokenType(
						midnight_node_ledger_helpers::HashOutput(bytes),
					))
				},
				None => ShieldedTokenType::default(),
			};
			shielded_outputs.push(ShieldedOutputSpec {
				wallet: sw,
				amount: triple.amount,
				token_type,
			});
		} else if let Ok(uw) = UnshieldedWallet::try_from(&local_addr) {
			let token_type = match triple.token_type {
				Some(bytes) => convert_unshielded_token_type(
					midnight_node_ledger_helpers::UnshieldedTokenType(
						midnight_node_ledger_helpers::HashOutput(bytes),
					),
				),
				None => UnshieldedTokenType::default(),
			};
			unshielded_outputs.push(UnshieldedOutputSpec {
				wallet: uw,
				amount: triple.amount,
				token_type,
			});
		} else {
			log::error!(
				"--output at position {} has an address that does not parse as shielded or unshielded: {:?}",
				idx,
				triple.address
			);
			panic!("--output address parse error");
		}
	}

	(shielded_outputs, unshielded_outputs)
}
