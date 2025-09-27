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

pub mod address;
pub mod cli_parsers;
pub mod genesis_generator;
pub mod indexer;
pub mod progress;
pub mod remote_prover;
pub mod sender;
pub mod serde_def;
pub mod tx_generator;
pub mod client;

use progress::{Progress, Spin};
use rand::{SeedableRng, rngs::StdRng};
use subxt::utils::H256;
use tx_generator::*;

use midnight_node_ledger_helpers::*;
use midnight_node_metadata::midnight_metadata as mn_meta;

// Conditionally define the type alias `ProofType` and `SignatureType`
#[cfg(not(feature = "erase-proof"))]
pub type ProofType = ProofMarker;

#[cfg(not(feature = "erase-proof"))]
pub type SignatureType = Signature;

#[cfg(feature = "erase-proof")]
pub type ProofType = ();

#[cfg(feature = "erase-proof")]
pub type SignagtureType = ();

pub fn hash_to_str(h: H256) -> String {
	format!("0x{}", hex::encode(h.as_bytes()))
}

// A default token used for zswap tests
pub fn t_token() -> ShieldedTokenType {
	Default::default()
}
pub fn network_as_str(id: NetworkId) -> &'static str {
	match id {
		NetworkId::MainNet => "mainnet",
		NetworkId::DevNet => "devnet",
		NetworkId::TestNet => "testnet",
		NetworkId::Undeployed => "undeployed",
		_ => panic!("unknown network id: {id:?}"),
	}
}

#[cfg(test)]
mod test {
	use crate::network_as_str;
	use midnight_node_ledger_helpers::NetworkId;

	#[test]
	fn test_network_as_str() {
		assert_eq!("mainnet", network_as_str(NetworkId::MainNet));
		assert_eq!("devnet", network_as_str(NetworkId::DevNet));
		assert_eq!("undeployed", network_as_str(NetworkId::Undeployed));
	}
}
