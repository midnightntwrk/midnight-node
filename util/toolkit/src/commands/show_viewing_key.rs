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

use crate::cli_parsers as cli;
use clap::Args;
use midnight_node_ledger_helpers::{DefaultDB, DerivationPath, Role, ShieldedWallet, WalletSeed};
#[derive(Args)]
pub struct ShowViewingKeyArgs {
	/// Target network
	#[arg(long)]
	network: String,

	/// Wallet seed
	#[arg(long, value_parser = cli::wallet_seed_decode)]
	seed: WalletSeed,
}

pub fn execute(args: ShowViewingKeyArgs) -> String {
	let derivation_path = DerivationPath::default_for_role(Role::Zswap);

	ShieldedWallet::<DefaultDB>::from_path(args.seed, &derivation_path)
		.expect("invalid Zswap derivation path")
		.viewing_key(&args.network)
}

#[cfg(test)]
mod test {
	use super::{ShowViewingKeyArgs, cli::wallet_seed_decode, execute};
	use test_case::test_case;

	#[test_case(
        "undeployed",
        "0000000000000000000000000000000000000000000000000000000000000001",
        "mn_shield-esk_undeployed1dlyj7u8juj68fd4psnkqhjxh32sec0q480vzswg8kd485e2kljcs9ete5h";
        "test undeployed with 0...01 seed"
    )]
	#[test_case(
        "devnet",
        "0000000000000000000000000000000000000000000000000000000000000002",
        "mn_shield-esk_devnet1w0dctw9zhe2ffqw4s5qks7rnl29wy5mhl957fv9nnhtxulent80q5dejklr";
        "test devnet with 0...02 seed"
    )]
	#[test_case(
        "testnet",
        "0000000000000000000000000000000000000000000000000000000000000003",
        "mn_shield-esk_testnet1wvd5v04ykt59gglxknsdxpwwkhhhj8d6h3ghpkgdhdsszap2p53qkprdkd8";
        "test testnet with 0...03 seed"
    )]
	fn test_show_viewing_key(network: &str, seed: &str, viewing_key: &str) {
		let args = ShowViewingKeyArgs {
			network: network.to_string(),
			seed: wallet_seed_decode(seed).expect("should return wallet seet"),
		};

		let actual_vk = execute(args);
		assert_eq!(viewing_key, actual_vk);
	}
}
