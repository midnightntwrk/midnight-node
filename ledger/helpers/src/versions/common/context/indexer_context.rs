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

use std::marker::PhantomData;

use async_trait::async_trait;

use super::super::{
	BindingKind, BlockContext, ContractAddress, ContractState, DB, LedgerParameters,
	PedersenDowngradeable, ProofKind, Resolver, Serializable, SignatureKind, Storable, Tagged,
	Timestamp, Transaction, Utxo, Wallet, WalletSeed, ZswapChainState,
};
use super::BuilderContext;

/// An indexer-backed [`BuilderContext`] that answers builder queries via indexer GraphQL queries
/// instead of replaying every block into a local [`super::super::LedgerState`] (see issue #1186).
///
/// This is a stub: its only purpose for now is to prove that [`BuilderContext`] is implementable by
/// something other than [`super::super::LedgerContext`]. If any method below cannot be expressed
/// without leaking `LedgerState` internals, the trait surface is wrong — fix the trait, not this
/// stub. The real implementation (HTTP/gRPC client, query mapping) belongs to a later stage.
///
/// `PhantomData<fn() -> D>` keeps the type `Send + Sync + 'static` regardless of `D`.
pub struct IndexerContext<D: DB + Clone> {
	_marker: PhantomData<fn() -> D>,
}

impl<D: DB + Clone> IndexerContext<D> {
	pub fn new() -> Self {
		Self { _marker: PhantomData }
	}
}

impl<D: DB + Clone> Default for IndexerContext<D> {
	fn default() -> Self {
		Self::new()
	}
}

#[async_trait]
impl<D: DB + Clone> BuilderContext<D> for IndexerContext<D> {
	fn with_wallet_from_seed<F, R>(&self, _seed: WalletSeed, _f: F) -> R
	where
		F: FnOnce(&mut Wallet<D>) -> R,
	{
		todo!("indexer: client-side wallet store, not yet implemented")
	}

	fn with_wallets_from_seeds<F, R>(
		&self,
		_origin_seed: WalletSeed,
		_destination_seed: WalletSeed,
		_f: F,
	) -> R
	where
		F: FnOnce(&mut Wallet<D>, &mut Wallet<D>) -> R,
	{
		todo!("indexer: client-side wallet store, not yet implemented")
	}

	async fn latest_block_context(&self) -> BlockContext {
		todo!("indexer: R6 — block() query")
	}

	async fn ledger_parameters(&self) -> LedgerParameters {
		todo!("indexer: R1 — Block.ledgerParameters blob")
	}

	async fn network_id(&self) -> String {
		// R2: the indexer has no network-id field; the real impl reads it from stored config.
		todo!("indexer: R2 — network id from stored config (indexer has no field)")
	}

	async fn unshielded_utxos(&self, _seed: WalletSeed) -> Vec<(Utxo, Timestamp)> {
		todo!("indexer: R3 — unshieldedTransactions query")
	}

	async fn zswap_state(&self) -> ZswapChainState<D> {
		todo!("indexer: R4 — merkle update stream")
	}

	async fn contract_state(&self, _address: ContractAddress) -> Option<ContractState<D>> {
		todo!("indexer: R5 — contractAction(address).state blob")
	}

	async fn resolver(&self) -> &'static Resolver {
		todo!("indexer: client-side resolver, not yet implemented")
	}

	async fn update_resolver(&self, _resolver: &'static Resolver) {
		todo!("indexer: client-side resolver, not yet implemented")
	}

	fn well_formed<S, P, B>(
		&self,
		_tx: &Transaction<S, P, B, D>,
		_now: Timestamp,
	) -> std::result::Result<(), Box<dyn std::error::Error + Send + Sync>>
	where
		S: SignatureKind<D>,
		P: ProofKind<D> + Storable<D>,
		B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D> + Tagged,
	{
		// R7: an indexer has no full LedgerState to validate against; the node re-validates on
		// submission, so the indexer-backed builder treats the tx as well-formed here.
		todo!("indexer: R7 — no local state; node re-validates on submit")
	}
}
