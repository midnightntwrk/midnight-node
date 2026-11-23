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

//! Start-up check that a validator's keystore holds a key of every session key type.
//!
//! Exists for the AURA-to-BABE migration (#2509); remove it once the chain runs BABE.
//!
//! A validator missing one of these keys starts normally and only skips its slots or votes once
//! it is in a committee, so the check logs an ERROR per missing type at start-up instead. The
//! node keeps running: it cannot know which keys a future session will need.
//!
//! Must be given the plain node keystore, not
//! [`crate::aura_to_babe_migration_keystore::AuraToBabeMigrationKeystore`], which answers BABE
//! queries with AURA keys.

use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_beefy::ecdsa_crypto::AuthorityId as BeefyId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::crypto::CryptoTypeId;
use sp_keystore::Keystore;
use sp_runtime::app_crypto::AppPublic;
use std::path::Path;

const LOG_TARGET: &str = "session-key-check";

/// Logs, per session key type, its public keys at INFO, or an ERROR when the keystore holds no
/// usable key of that type or cannot be listed.
pub fn log_session_keys(keystore: &dyn Keystore, keystore_path: Option<&Path>) {
	let lines = [
		check::<AuraId>(keystore, keystore_path, "AURA", "AURA_SEED_FILE"),
		check::<BabeId>(keystore, keystore_path, "BABE", "BABE_SEED_FILE"),
		check::<GrandpaId>(keystore, keystore_path, "GRANDPA", "GRANDPA_SEED_FILE"),
		check::<BeefyId>(keystore, keystore_path, "BEEFY", "BEEFY_SEED_FILE"),
	];
	for line in lines {
		match line {
			Ok(line) => log::info!(target: LOG_TARGET, "{line}"),
			Err(line) => log::error!(target: LOG_TARGET, "{line}"),
		}
	}
}

/// `Ok` with the keystore's keys of `A`'s key type, `Err` with why there is none to use.
///
/// Only lists public keys, so no key file is opened. A key counts when it decodes as `A`, which
/// checks only its length, as the keystore's own per-scheme listings do: a key inserted as ecdsa
/// instead of sr25519 is rejected, one inserted as ed25519 instead of sr25519 is not.
fn check<A: AppPublic>(
	keystore: &dyn Keystore,
	keystore_path: Option<&Path>,
	name: &str,
	seed_file_var: &str,
) -> Result<String, String> {
	let key_type = String::from_utf8_lossy(&A::ID.0);
	let keystore_path =
		keystore_path.map_or_else(|| "<keystore path>".into(), |p| p.display().to_string());
	let keys = keystore.keys(A::ID).map_err(|err| {
		format!(
			"Cannot list {name} keys (key type `{key_type}`) in keystore {keystore_path}: {err}"
		)
	})?;
	let usable: Vec<_> = keys
		.iter()
		.filter(|k| A::from_slice(k).is_ok())
		.map(|k| format!("0x{}", hex::encode(k)))
		.collect();
	if !usable.is_empty() {
		return Ok(format!(
			"{name} key(s) (key type `{key_type}`) in keystore: {}",
			usable.join(", ")
		));
	}
	let key_insert = cli_scheme(A::CRYPTO_ID)
		.map(|scheme| {
			format!(
				" or `midnight-node key insert --chain <chain spec> --keystore-path {keystore_path} \
				 --key-type {key_type} --scheme {scheme} --suri <secret>`"
			)
		})
		.unwrap_or_default();
	Err(format!(
		"No {name} key (key type `{key_type}`) in keystore: this validator cannot use it once in a \
		 committee. Load one with {seed_file_var}{key_insert}"
	))
}

/// The `key insert --scheme` value for a crypto type; `None` for one the CLI cannot insert.
fn cli_scheme(crypto_id: CryptoTypeId) -> Option<&'static str> {
	match crypto_id {
		sp_core::sr25519::CRYPTO_ID => Some("sr25519"),
		sp_core::ed25519::CRYPTO_ID => Some("ed25519"),
		sp_core::ecdsa::CRYPTO_ID => Some("ecdsa"),
		_ => None,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_keystore::LocalKeystore;
	use sp_core::crypto::key_types::{AURA, BABE, BEEFY, GRANDPA};
	use sp_keystore::testing::MemoryKeystore;

	const PATH: &str = "/data/chains/x/keystore";

	fn path() -> Option<&'static Path> {
		Some(Path::new(PATH))
	}

	#[test]
	fn present_key_is_listed() {
		let keystore = MemoryKeystore::new();
		let beefy = keystore.ecdsa_generate_new(BEEFY, Some("//Alice")).unwrap();

		let line = check::<BeefyId>(&keystore, path(), "BEEFY", "BEEFY_SEED_FILE").unwrap();

		assert_eq!(
			line,
			format!("BEEFY key(s) (key type `beef`) in keystore: 0x{}", hex::encode(beefy))
		);
	}

	#[test]
	fn every_type_is_checked_under_its_own_key_type() {
		let keystore = MemoryKeystore::new();
		keystore.sr25519_generate_new(AURA, Some("//Alice")).unwrap();
		keystore.sr25519_generate_new(BABE, Some("//Alice//babe")).unwrap();
		keystore.ed25519_generate_new(GRANDPA, Some("//Alice")).unwrap();
		keystore.ecdsa_generate_new(BEEFY, Some("//Alice")).unwrap();

		assert!(check::<AuraId>(&keystore, path(), "AURA", "AURA_SEED_FILE").is_ok());
		assert!(check::<BabeId>(&keystore, path(), "BABE", "BABE_SEED_FILE").is_ok());
		assert!(check::<GrandpaId>(&keystore, path(), "GRANDPA", "GRANDPA_SEED_FILE").is_ok());
		assert!(check::<BeefyId>(&keystore, path(), "BEEFY", "BEEFY_SEED_FILE").is_ok());
	}

	#[test]
	fn missing_key_says_how_to_load_it() {
		let keystore = MemoryKeystore::new();
		// An AURA key does not count as a BABE key.
		keystore.sr25519_generate_new(AURA, Some("//Alice")).unwrap();

		let line = check::<BabeId>(&keystore, path(), "BABE", "BABE_SEED_FILE").unwrap_err();

		assert!(line.starts_with("No BABE key (key type `babe`) in keystore"), "{line}");
		assert!(line.contains("Load one with BABE_SEED_FILE or `midnight-node key insert"));
		assert!(line.contains(&format!(
			"--keystore-path {PATH} --key-type babe --scheme sr25519 --suri <secret>`"
		)));
	}

	#[test]
	fn key_of_another_length_does_not_count() {
		let dir = tempfile::TempDir::new().unwrap();
		let keystore = LocalKeystore::open(dir.path(), None).unwrap();
		// A BEEFY key inserted as sr25519 instead of ecdsa.
		keystore.sr25519_generate_new(BEEFY, None).unwrap();

		let line = check::<BeefyId>(&keystore, None, "BEEFY", "BEEFY_SEED_FILE").unwrap_err();

		assert!(line.starts_with("No BEEFY key"), "{line}");
		assert!(line.contains("--keystore-path <keystore path>"));
	}

	#[test]
	fn unreadable_keystore_reports_the_error() {
		let dir = tempfile::TempDir::new().unwrap();
		let path = dir.path().join("keystore");
		let keystore = LocalKeystore::open(&path, None).unwrap();
		std::fs::remove_dir_all(&path).unwrap();

		let line = check::<AuraId>(&keystore, Some(&path), "AURA", "AURA_SEED_FILE").unwrap_err();

		assert!(line.starts_with("Cannot list AURA keys"), "{line}");
	}
}
