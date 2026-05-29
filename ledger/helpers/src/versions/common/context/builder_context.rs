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

use async_trait::async_trait;

use super::super::{
	BindingKind, BlockContext, ContractAddress, ContractState, DB, LedgerParameters,
	PedersenDowngradeable, ProofKind, Resolver, Serializable, SignatureKind, Storable, Tagged,
	Timestamp, Transaction, Utxo, Wallet, WalletSeed, ZswapChainState,
};

type BuildResult<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

/// Abstraction over the ledger context that transaction builders interact with.
///
/// The backend is either a local [`super::super::LedgerContext`] (which owns a
/// [`super::super::LedgerState`]) or, in the future, an indexer-backed client that answers the
/// same queries without replaying every block locally (see issue #1186).
#[async_trait]
pub trait BuilderContext<D: DB + Clone>: Send + Sync + 'static {
	/// Operate on a single wallet identified by seed.
	fn with_wallet_from_seed<F, R>(&self, seed: WalletSeed, f: F) -> R
	where
		F: FnOnce(&mut Wallet<D>) -> R;

	/// Operate on two wallets identified by origin and destination seeds.
	fn with_wallets_from_seeds<F, R>(
		&self,
		origin_seed: WalletSeed,
		destination_seed: WalletSeed,
		f: F,
	) -> R
	where
		F: FnOnce(&mut Wallet<D>, &mut Wallet<D>) -> R;

	/// The most recent block context (block time etc.).
	async fn latest_block_context(&self) -> BlockContext;

	/// Current ledger parameters (fee/cost model, dust params, TTLs).
	async fn ledger_parameters(&self) -> LedgerParameters;

	/// The chain's network identifier.
	async fn network_id(&self) -> String;

	/// All unshielded UTXOs owned by the wallet for `seed`, with their creation time.
	async fn unshielded_utxos(&self, seed: WalletSeed) -> Vec<(Utxo, Timestamp)>;

	/// The global shielded (zswap) chain state.
	async fn zswap_state(&self) -> ZswapChainState<D>;

	/// The on-chain state of the contract at `address`, if it exists.
	async fn contract_state(&self, address: ContractAddress) -> Option<ContractState<D>>;

	/// The resolver currently used for proving.
	async fn resolver(&self) -> &'static Resolver;

	/// Replace the resolver used for proving.
	async fn update_resolver(&self, resolver: &'static Resolver);

	/// Validate that `tx` is well-formed against the current ledger state.
	///
	/// The local backend checks against its [`super::super::LedgerState`]; an indexer-backed
	/// backend has no full state to check against and relies on the node validating on submission.
	fn well_formed<S, P, B>(&self, tx: &Transaction<S, P, B, D>, now: Timestamp) -> BuildResult<()>
	where
		S: SignatureKind<D>,
		P: ProofKind<D> + Storable<D>,
		B: Storable<D> + Serializable + PedersenDowngradeable<D> + BindingKind<S, P, D> + Tagged;
}
