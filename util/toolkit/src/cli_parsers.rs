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

use std::str::FromStr;

use midnight_node_ledger_helpers::*;

pub trait TokenDecode: Sized + Send + Sync + Clone {
	fn decode(token_id: [u8; 32]) -> Self;
}

impl TokenDecode for UnshieldedTokenType {
	fn decode(token_id: [u8; 32]) -> Self {
		UnshieldedTokenType(HashOutput(token_id))
	}
}

impl TokenDecode for ShieldedTokenType {
	fn decode(token_id: [u8; 32]) -> Self {
		ShieldedTokenType(HashOutput(token_id))
	}
}

pub fn token_decode<T: TokenDecode>(input: &str) -> Result<T, clap::error::Error> {
	let token_id: [u8; 32] = hex_str_decode(input)?;
	let token = T::decode(token_id);

	Ok(token)
}

pub fn wallet_seed_decode(input: &str) -> Result<WalletSeed, clap::error::Error> {
	input.parse().map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse seed: {}", e)),
		);
		err
	})
}

pub fn hex_ledger_decode<T: Deserializable + Tagged>(input: &str) -> Result<T, clap::error::Error> {
	if let Ok(addr) = hex_ledger_tagged_decode::<T>(input) {
		Ok(addr)
	} else {
		hex_ledger_untagged_decode::<T>(input)
	}
}

pub fn coin_public_decode(input: &str) -> Result<CoinPublicKey, clap::error::Error> {
	hex_ledger_decode(input)
}

pub fn contract_address_decode(input: &str) -> Result<ContractAddress, clap::error::Error> {
	hex_ledger_decode(input)
}

pub fn hex_ledger_untagged_decode<T>(input: &str) -> Result<T, clap::error::Error>
where
	T: Deserializable,
{
	let bytes = hex::decode(input).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse seed: {}", e)),
		);
		err
	})?;

	let res = <T as Deserializable>::deserialize(&mut &bytes[..], 0).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to deserialize arg: {e}")),
		);
		err
	})?;

	Ok(res)
}

pub fn hex_ledger_tagged_decode<T>(input: &str) -> Result<T, clap::error::Error>
where
	T: Deserializable + Tagged,
{
	let bytes = hex::decode(input).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse: {}", e)),
		);
		err
	})?;

	let res: T = deserialize(&mut &bytes[..]).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to deserialize arg: {e}")),
		);
		err
	})?;

	Ok(res)
}

pub fn hex_str_decode<T>(input: &str) -> Result<T, clap::error::Error>
where
	T: TryFrom<Vec<u8>, Error = Vec<u8>>,
{
	let bytes = hex::decode(input).map_err(|e| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("failed to parse seed: {}", e)),
		);
		err
	})?;

	let res: T = bytes.try_into().map_err(|e: Vec<u8>| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!(
				"incorrect length for token type string. Expected 32, got {}",
				e.len()
			)),
		);
		err
	})?;

	Ok(res)
}

pub fn wallet_address(input: &str) -> Result<WalletAddress, clap::Error> {
	WalletAddress::from_str(input).map_err(|error| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("invalid wallet address: {}", error)),
		);

		err
	})
}

pub fn utxo_id_decode(input: &str) -> Result<UtxoId, clap::Error> {
	UtxoId::from_str(input).map_err(|error| {
		let mut err = clap::Error::new(clap::error::ErrorKind::ValueValidation);
		err.insert(
			clap::error::ContextKind::Custom,
			clap::error::ContextValue::String(format!("invalid utxo id: {}", error)),
		);

		err
	})
}
