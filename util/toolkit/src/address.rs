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

use bech32::Bech32m;
use thiserror::Error;

use midnight_node_ledger_helpers::*;

#[derive(Error, Debug)]
pub enum MidnightAddressError {
	#[error("prefix first part != 'mn'")]
	PrefixInvalidConstant,
	#[error("prefix missing type")]
	PrefixMissingType,
}

#[derive(Debug, Clone)]
pub struct MidnightAddress {
	pub type_: String,
	pub network: Option<String>,
	pub data: Vec<u8>,
}

impl MidnightAddress {
	pub fn decode(encoded_data: &str) -> Result<Self, MidnightAddressError> {
		let (hrp, data) = bech32::decode(encoded_data).expect("Failed while bech32 decoding");
		let prefix_parts = hrp.as_str().split('_').collect::<Vec<&str>>();
		prefix_parts
			.first()
			.filter(|c| *c == &"mn")
			.ok_or(MidnightAddressError::PrefixInvalidConstant)?;
		let type_ = prefix_parts.get(1).ok_or(MidnightAddressError::PrefixMissingType)?.to_string();
		let network = prefix_parts.get(2).map(|s| s.to_string());

		Ok(Self { type_, network, data })
	}

	pub fn encode(&self) -> String {
		let network_str = match &self.network {
			Some(network) => format!("_{}", network),
			None => "".to_string(),
		};

		bech32::encode::<Bech32m>(
			bech32::Hrp::parse(&format!("mn_{}{}", self.type_, network_str))
				.expect("Failed while bech32 parsing"),
			&self.data,
		)
		.expect("Failed while bech32 encoding")
	}

	pub fn from_wallet<D: DB + Clone>(wallet: &Wallet<D>, network: NetworkId) -> Self {
		let network_str = match network {
			NetworkId::MainNet => None,
			NetworkId::DevNet => Some("dev".to_string()),
			NetworkId::TestNet => Some("test".to_string()),
			NetworkId::Undeployed => Some("undeployed".to_string()),
			_ => None,
		};

		let coin_pub_key = wallet.shielded.secret_keys().coin_public_key().0.0;
		let mut enc_pub_key = Vec::new();
		Serializable::serialize(&wallet.shielded.secret_keys().enc_public_key(), &mut enc_pub_key)
			.expect("Failed serializing secret keys");

		Self {
			type_: "shield-addr".to_string(),
			network: network_str,
			data: [&coin_pub_key[..], &enc_pub_key[..]].concat(),
		}
	}
}

impl TryFrom<&MidnightAddress> for NetworkId {
	type Error = String;

	fn try_from(value: &MidnightAddress) -> Result<Self, Self::Error> {
		match value.network {
			Some(ref network) => match network.as_str() {
				"dev" => Ok(NetworkId::DevNet),
				"test" => Ok(NetworkId::TestNet),
				"undeployed" => Ok(NetworkId::Undeployed),
				_ => Err(network.to_string()),
			},
			None => Ok(NetworkId::MainNet),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use bech32::{Bech32m, Hrp};

	#[test]
	fn test_parse() {
		let encoded_str = bech32::encode::<Bech32m>(
			Hrp::parse("mn_shield-addr_test").expect("Failed while bech32 parsing"),
			&[1, 2, 3],
		)
		.expect("Failed while bech32 encoding");
		let address =
			MidnightAddress::decode(&encoded_str).expect("Failed while decoding `MidnightAddress");
		assert_eq!(address.type_, "shield-addr".to_string());
		assert_eq!(address.network, Some("test".to_string()));
		assert_eq!(address.data, vec![1u8, 2u8, 3u8]);
	}
}
