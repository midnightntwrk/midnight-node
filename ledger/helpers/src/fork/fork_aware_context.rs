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

use crate::fork::fork_7_to_8::fork_context_7_to_8;
use crate::fork::raw_block_data::{LedgerVersion, RawBlockData, RawTransaction};

type Db7 = crate::ledger_7::DefaultDB;
type Db8 = crate::ledger_8::DefaultDB;

pub enum ForkAwareLedgerContext {
	Ledger7(crate::ledger_7::context::LedgerContext<Db7>),
	Ledger8(crate::ledger_8::context::LedgerContext<Db8>),
}

impl ForkAwareLedgerContext {
	/// Create a new context at the given ledger version.
	pub fn new(version: LedgerVersion, network_id: impl Into<String>) -> Self {
		let network_id = network_id.into();
		match version {
			LedgerVersion::Ledger7 => {
				Self::Ledger7(crate::ledger_7::context::LedgerContext::new(network_id))
			},
			LedgerVersion::Ledger8 => {
				Self::Ledger8(crate::ledger_8::context::LedgerContext::new(network_id))
			},
		}
	}

	/// Create a new context with wallet seeds at the given ledger version.
	pub fn new_from_wallet_seeds(
		version: LedgerVersion,
		network_id: impl Into<String>,
		seeds: &[crate::ledger_8::WalletSeed],
	) -> Self {
		let network_id = network_id.into();
		match version {
			LedgerVersion::Ledger7 => {
				// Convert ledger_8 WalletSeeds to ledger_7 WalletSeeds
				let seeds_7: Vec<crate::ledger_7::WalletSeed> = seeds
					.iter()
					.map(|s| {
						crate::ledger_7::WalletSeed::try_from(s.as_bytes())
							.expect("ledger seed format should be backwards compatible")
					})
					.collect();
				Self::Ledger7(crate::ledger_7::context::LedgerContext::new_from_wallet_seeds(
					network_id, &seeds_7,
				))
			},
			LedgerVersion::Ledger8 => Self::Ledger8(
				crate::ledger_8::context::LedgerContext::new_from_wallet_seeds(network_id, seeds),
			),
		}
	}

	/// Get the current ledger version.
	pub fn version(&self) -> LedgerVersion {
		match self {
			Self::Ledger7(_) => LedgerVersion::Ledger7,
			Self::Ledger8(_) => LedgerVersion::Ledger8,
		}
	}

	/// Process a raw block, handling fork transitions.
	///
	/// If the context is currently Ledger7 and the block is Ledger8,
	/// the context is automatically forked to Ledger8 first.
	pub fn update_from_block(mut self, block: &RawBlockData) -> Self {
		let block_version = block.ledger_version();

		// Handle fork transition: Ledger7 context + Ledger8 block
		if self.version() == LedgerVersion::Ledger7 && block_version == LedgerVersion::Ledger8 {
			self = self.next_fork();
		}

		match &self {
			Self::Ledger7(ctx) => update_context_7(ctx, block),
			Self::Ledger8(ctx) => update_context_8(ctx, block),
		}

		self
	}

	/// Fork the context from Ledger7 to Ledger8.
	fn next_fork(self) -> Self {
		match self {
			Self::Ledger7(ctx7) => {
				let ctx8 = fork_context_7_to_8(ctx7)
					.expect("failed to fork context from ledger 7 to ledger 8");
				Self::Ledger8(ctx8)
			},
			Self::Ledger8(_) => {
				panic!("next_fork called on Ledger8 context");
			},
		}
	}

	/// Dispatch on the ledger version, passing the inner context to the
	/// appropriate closure.
	pub fn dispatch<T>(
		self,
		f7: impl FnOnce(crate::ledger_7::context::LedgerContext<Db7>) -> T,
		f8: impl FnOnce(crate::ledger_8::context::LedgerContext<Db8>) -> T,
	) -> T {
		match self {
			Self::Ledger7(ctx) => f7(ctx),
			Self::Ledger8(ctx) => f8(ctx),
		}
	}

	/// Extract the inner Ledger7 context, consuming self.
	///
	/// Returns `None` if the context has already forked to Ledger8.
	pub fn into_ledger7(self) -> Option<crate::ledger_7::context::LedgerContext<Db7>> {
		match self {
			Self::Ledger7(ctx) => Some(ctx),
			Self::Ledger8(_) => None,
		}
	}

	/// Extract the inner Ledger8 context, consuming self.
	///
	/// Returns `None` if the context is still at Ledger7.
	pub fn into_ledger8(self) -> Option<crate::ledger_8::context::LedgerContext<Db8>> {
		match self {
			Self::Ledger8(ctx) => Some(ctx),
			Self::Ledger7(_) => None,
		}
	}
}

/// Deserialize raw transactions and update a Ledger7 context.
fn update_context_7(ctx: &crate::ledger_7::context::LedgerContext<Db7>, block: &RawBlockData) {
	use crate::ledger_7::{
		HashOutput, SerdeTransaction, SystemTransaction, Timestamp,
		midnight_serialize::tagged_deserialize,
	};

	type MnTx7 = crate::ledger_7::Transaction<
		crate::ledger_7::Signature,
		crate::ledger_7::ProofMarker,
		crate::ledger_7::PureGeneratorPedersen,
		Db7,
	>;
	type SerdeTx7 = SerdeTransaction<crate::ledger_7::Signature, crate::ledger_7::ProofMarker, Db7>;

	let mut transactions: Vec<SerdeTx7> = Vec::new();
	for raw_tx in &block.transactions {
		match raw_tx {
			RawTransaction::Midnight(bytes) => {
				let tx: MnTx7 = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 7 midnight transaction");
				transactions.push(SerdeTx7::Midnight(tx));
			},
			RawTransaction::System(bytes) => {
				let tx: SystemTransaction = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 7 system transaction");
				transactions.push(SerdeTx7::System(tx));
			},
		}
	}

	let block_context = crate::ledger_7::make_block_context(
		Timestamp::from_secs(block.tblock_secs),
		HashOutput(block.parent_block_hash),
		Timestamp::from_secs(block.last_block_time_secs),
	);

	ctx.update_from_block(
		&transactions,
		&block_context,
		block.state_root.as_ref(),
		block.state.as_ref(),
	);
}

/// Deserialize raw transactions and update a Ledger8 context.
fn update_context_8(ctx: &crate::ledger_8::context::LedgerContext<Db8>, block: &RawBlockData) {
	use crate::ledger_8::{
		HashOutput, SerdeTransaction, SystemTransaction, Timestamp,
		midnight_serialize::tagged_deserialize,
	};

	type MnTx8 = crate::ledger_8::Transaction<
		crate::ledger_8::Signature,
		crate::ledger_8::ProofMarker,
		crate::ledger_8::PureGeneratorPedersen,
		Db8,
	>;
	type SerdeTx8 = SerdeTransaction<crate::ledger_8::Signature, crate::ledger_8::ProofMarker, Db8>;

	let mut transactions: Vec<SerdeTx8> = Vec::new();
	for raw_tx in &block.transactions {
		match raw_tx {
			RawTransaction::Midnight(bytes) => {
				let tx: MnTx8 = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 8 midnight transaction");
				transactions.push(SerdeTx8::Midnight(tx));
			},
			RawTransaction::System(bytes) => {
				let tx: SystemTransaction = tagged_deserialize(&mut bytes.as_slice())
					.expect("failed to deserialize ledger 8 system transaction");
				transactions.push(SerdeTx8::System(tx));
			},
		}
	}

	let block_context = crate::ledger_8::make_block_context(
		Timestamp::from_secs(block.tblock_secs),
		HashOutput(block.parent_block_hash),
		Timestamp::from_secs(block.last_block_time_secs),
	);

	ctx.update_from_block(
		&transactions,
		&block_context,
		block.state_root.as_ref(),
		block.state.as_ref(),
	);
}
