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

use std::str::FromStr;

use bip39::Mnemonic;
use error::UpgraderError;
use subxt::{
	OnlineClient, SubstrateConfig,
	dynamic::{self, Value},
	tx::Payload,
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
	log::info!("Executing runtime upgrade via federated authority.");

	// Create a new API client
	let api = OnlineClient::<SubstrateConfig>::from_insecure_url(rpc_url).await?;

	// Authority member keypairs
	// Technical Committee members: Alice, Bob
	let alice = Keypair::from_uri(&SecretUri::from_str("//Alice")?)?;
	let bob = Keypair::from_uri(&SecretUri::from_str("//Bob")?)?;
	// Council members: Dave, Eve
	let dave = Keypair::from_uri(&SecretUri::from_str("//Dave")?)?;
	let eve = Keypair::from_uri(&SecretUri::from_str("//Eve")?)?;

	let council_keypairs = vec![dave, eve];
	let tc_keypairs = vec![alice, bob];

	// Step 1: Compute the code hash and encode the authorize_upgrade call
	let code_hash = sp_crypto_hashing::blake2_256(code);
	log::info!("Code hash: 0x{}", hex::encode(code_hash));

	let authorize_upgrade_call =
		dynamic::tx("System", "authorize_upgrade", vec![Value::from_bytes(&code_hash)]);
	let encoded_call = authorize_upgrade_call.encode_call_data(&api.metadata()).map_err(|e| {
		UpgraderError::GovernanceError(governance_helpers::GovernanceError::CallDecodeError(
			format!("{:?}", e),
		))
	})?;

	// Step 2: Execute authorize_upgrade through governance
	governance_helpers::execute_governance_call(
		&api,
		&encoded_call,
		&council_keypairs,
		&tc_keypairs,
	)
	.await?;

	log::info!("authorize_upgrade executed with Root origin!");

	// Step 3: Apply the authorized upgrade
	log::info!("Applying authorized upgrade...");
	let apply_upgrade_call =
		dynamic::tx("System", "apply_authorized_upgrade", vec![Value::from_bytes(code)]);

	let apply_events = api
		.tx()
		.sign_and_submit_then_watch_default(&apply_upgrade_call, signer)
		.await?
		.wait_for_finalized_success()
		.await?;

	// Verify upgrade was successful
	let mut success = false;
	for event in apply_events.iter() {
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

	log::info!("Runtime upgrade completed successfully!");
	Ok(())
}
