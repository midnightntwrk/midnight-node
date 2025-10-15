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

use super::{DB, LedgerContext, UnshieldedTokenType, UnshieldedWallet, UtxoOutput, WalletSeed};
use std::sync::Arc;

pub struct UtxoOutputInfo<O> {
	pub value: u128,
	pub owner: O,
	pub token_type: UnshieldedTokenType,
}

pub trait BuildUtxoOutput<D: DB + Clone>: Send + Sync {
	fn build(&self, context: Arc<LedgerContext<D>>) -> UtxoOutput;
}

impl<D: DB + Clone> BuildUtxoOutput<D> for UtxoOutputInfo<WalletSeed> {
	fn build(&self, context: Arc<LedgerContext<D>>) -> UtxoOutput {
		context.with_wallet_from_seed(self.owner, |wallet| UtxoOutput {
			value: self.value,
			owner: wallet.unshielded.signing_key().verifying_key().into(),
			type_: self.token_type,
		})
	}
}

impl<D: DB + Clone> BuildUtxoOutput<D> for UtxoOutputInfo<UnshieldedWallet> {
	fn build(&self, _context: Arc<LedgerContext<D>>) -> UtxoOutput {
		UtxoOutput { value: self.value, owner: self.owner.user_address, type_: self.token_type }
	}
}
