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

use bip39::Mnemonic;
use error::UpgraderError;
use subxt::{
	OnlineClient, SubstrateConfig,
	dynamic::{self, Value},
};
use subxt_signer::SecretUri;
use subxt_signer::sr25519::Keypair;

pub mod error;

pub fn get_signer(key_str: &str) -> Result<Keypair, UpgraderError> {
	// Supports seed phrases
	if key_str.contains('/') {
		let uri = SecretUri::from_str(key_str)?;
		Ok(Keypair::from_uri(&uri)?)
	} else {
		let phrase = Mnemonic::parse(key_str)?;
		Ok(Keypair::from_phrase(&phrase, None)?)
	}
}

pub async fn execute_upgrade(
	rpc_url: &str,
	signer: &Keypair,
	code: &[u8],
) -> Result<(), UpgraderError> {
	log::info!("Executing runtime upgrade.");

	// Create a new API client
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(rpc_url).await?;

	// Construct the `set_code` call dynamically
	let set_code_call =
		dynamic::tx("System", "set_code", vec![Value::from_bytes(code)]).into_value();

	let weight_value = Value::named_composite(vec![
		("ref_time", Value::from(0_u64)),
		("proof_size", Value::from(0_u64)),
	]);

	// Construct the `sudo_unchecked_weight` call dynamically
	let sudo_set_code_tx =
		dynamic::tx("Sudo", "sudo_unchecked_weight", vec![set_code_call, weight_value]);

	log::info!("Sending code upgrade transaction...");
	// Submit the transaction
	let events = api
		.tx()
		.sign_and_submit_then_watch_default(&sudo_set_code_tx, signer)
		.await?
		.wait_for_finalized_success()
		.await?;

	// Find the `System::CodeUpdated` event dynamically
	let mut success = false;
	for event in events.iter() {
		let event = event?;
		if event.pallet_name() == "System" && event.variant_name() == "CodeUpdated" {
			log::info!("Code update success: {:?}", event);
			success = true;
			break;
		}
	}
	if !success {
		return Err(UpgraderError::CodeUpgradeFailed);
	}

	Ok(())
}
