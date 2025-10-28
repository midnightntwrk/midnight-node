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

use crate::{DefaultDB, IntoWalletAddress, ShieldedWallet, UnshieldedWallet};
use clap::Args;
use midnight_node_toolkit::cli_parsers::{self as cli};

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

#[derive(Args, Clone)]
pub struct RandomAddressArgs {
	/// Target network
	#[arg(long)]
	network: String,
	/// Select if the address should be shielded or not
	#[arg(long)]
	shielded: bool,
	/// Optional seed for randomness
	#[arg(long, short, value_parser = cli::hex_str_decode::<[u8; 32]>)]
	randomness_seed: Option<[u8; 32]>,
}

pub fn execute(args: RandomAddressArgs) -> String {
	let seed: [u8; 32] = match args.randomness_seed {
		Some(seed) => {
			let mut rng = StdRng::from_seed(seed);
			rng.r#gen()
		},
		None => rand::random(),
	};

	let address = if args.shielded {
		let wallet: ShieldedWallet<DefaultDB> = ShieldedWallet::default(seed.into());

		wallet.address(&args.network)
	} else {
		let wallet = UnshieldedWallet::default(seed.into());

		wallet.address(&args.network)
	};

	address.to_bech32()
}

#[cfg(test)]
mod tests {
	use midnight_node_toolkit::cli_parsers as cli;

	use super::RandomAddressArgs;
	use test_case::test_case;

	macro_rules! test_fixture {
		($network:expr, $shielded:expr, $seed:literal) => {
			RandomAddressArgs {
				network: $network.to_string(),
				shielded: $shielded,
				randomness_seed: Some(cli::hex_str_decode($seed).unwrap()),
			}
		};
		($network:expr, $shielded:expr) => {
			RandomAddressArgs {
				network: $network.to_string(),
				shielded: $shielded,
				randomness_seed: None,
			}
		};
	}

	#[test_case(test_fixture!("devnet", true, "0000000000000000000000000000000000000000000000000000000000000001") =>
	    "mn_shield-addr_devnet1xscm9xd96mmx8u2s2mgpsk66yewrqluax7d0vgu5asff2ek8fcmsxqp00mnr3lked3qn6d06yl6lj2dw9x9azd8vlcud9f4c4kx05a2wuv7hh6r8";
		"shielded address from seed"
	)]
	#[test_case(test_fixture!("devnet", false, "0000000000000000000000000000000000000000000000000000000000000001") =>
	    "mn_addr_devnet1r9fcd53aa5vz34krw59p3zcgjtg4wtrjgny03ykxvhl7njjujvuqgeradv";
		"unshielded address from seed"
	)]
	#[test_case(test_fixture!("devnet", false) =>
	    matches addr if addr.starts_with("mn_addr");
		"unshielded without seed generates unshielded"
	)]
	#[test_case(test_fixture!("devnet", true) =>
	    matches addr if addr.starts_with("mn_shield-");
		"shielded without seed generates shielded"
	)]
	fn test_random_address(args: RandomAddressArgs) -> String {
		super::execute(args)
	}
}
