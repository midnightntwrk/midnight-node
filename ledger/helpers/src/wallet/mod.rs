// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
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

use crate::{
	DB, DUMMY_TRANSACTION_COST_MODEL, LedgerState, ProofKind, SignatureKind, StandardTransaction,
	Transaction, TransactionWithContext, UserAddress, Utxo, WalletSeed,
};

mod hd;
mod shielded;
mod unshielded;

pub use hd::*;
pub use shielded::*;
pub use unshielded::*;

#[derive(Clone, Debug)]
pub struct Wallet<D: DB + Clone> {
	pub root_seed: WalletSeed,
	pub shielded: ShieldedWallet<D>,
	pub unshielded: UnshieldedWallet,
}

impl<D: DB + Clone> Wallet<D> {
	pub fn default(root_seed: WalletSeed) -> Self {
		let shielded = ShieldedWallet::default(root_seed);
		let unshielded = UnshieldedWallet::default(root_seed);

		Self { root_seed, shielded, unshielded }
	}

	pub fn update_state_from_tx<S: SignatureKind<D>, P: ProofKind<D>>(
		&mut self,
		tx: TransactionWithContext<S, P, D>,
	) {
		if let Transaction::Standard(StandardTransaction {
			guaranteed_coins: Some(ref guaranteed_coins),
			..
		}) = tx.into()
		{
			self.shielded.state =
				self.shielded.state.apply(self.shielded.secret_keys(), guaranteed_coins);
		}

		// // TODO UNSHIELDED
		// if let Transaction::ClaimMint(ref authorized_mint) = tx {
		// 	self.state = self.state.apply_mint(&self.secret_keys, &authorized_mint.mint);
		// }
	}

	pub fn unshielded_utxos(&self, ledger_state: &LedgerState<D>) -> Vec<Utxo> {
		let address = UserAddress::from(self.unshielded.verifying_key.clone());
		ledger_state
			.utxo
			.utxos
			.0
			.iter()
			.filter(|utxo| utxo.0.owner == address)
			.map(|utxo| (*utxo.0).clone())
			.collect()
	}

	pub fn increment_seed(s: &str) -> String {
		let num = u128::from_str_radix(s, 2).expect("Invalid wallet seed");
		let result = num + 1;
		let width = s.len();
		format!("{:0width$b}", result, width = width)
	}

	/// Duplicate of function in midnight-wallet
	/// See:
	/// - https://github.com/input-output-hk/midnight-wallet/blob/8cac3b5ad8534b73c7fd9060a5540c3f17104e2b/wallet-core/src/main/scala/io/iohk/midnight/wallet/core/TransactionBalancer.scala#L233
	pub fn calculate_fee(num_inputs: usize, num_outputs: usize) -> u128 {
		let cost_model = DUMMY_TRANSACTION_COST_MODEL;
		let input_fee = cost_model.input_fee_overhead() * num_inputs as u128;
		let output_fee = cost_model.output_fee_overhead() * num_outputs as u128;

		let overhead_fee = cost_model.input_fee_overhead() * (2 * cost_model.output_fee_overhead());

		input_fee + output_fee + overhead_fee
	}

	pub fn wallet_seed_decode(input: &str) -> WalletSeed {
		let wallet_seed_bytes: [u8; 32] = Self::hex_str_decode(input);
		WalletSeed::from(wallet_seed_bytes)
	}

	fn hex_str_decode<T>(input: &str) -> T
	where
		T: TryFrom<Vec<u8>, Error = Vec<u8>>,
	{
		let bytes = hex::decode(input)
			.unwrap_or_else(|_| panic!("failed to parse wallet seed: {:?}", input));

		let res: T = bytes
			.try_into()
			.unwrap_or_else(|_| panic!("failed to parse wallet seed: {:?}", input));

		res
	}
}
