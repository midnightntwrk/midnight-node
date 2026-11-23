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
//! A validator missing one of these keys starts normally and only skips its slots or votes once
//! it is in a committee, so the check logs an ERROR per missing type at start-up instead. The
//! node keeps running: it cannot know which keys a future session will need.
//!
//! Must be given the plain node keystore, not
//! [`crate::aura_to_babe_migration_keystore::AuraToBabeMigrationKeystore`], which answers BABE
//! queries with AURA keys.

use log::Level;
use sp_core::crypto::{
	KeyTypeId,
	key_types::{AURA, BABE, BEEFY, GRANDPA},
};
use sp_keystore::Keystore;
use std::path::Path;

const LOG_TARGET: &str = "session-key-check";

/// Signature scheme of a session key type.
#[derive(Clone, Copy)]
pub enum Scheme {
	Sr25519,
	Ed25519,
	Ecdsa,
}

impl Scheme {
	fn name(self) -> &'static str {
		match self {
			Scheme::Sr25519 => "sr25519",
			Scheme::Ed25519 => "ed25519",
			Scheme::Ecdsa => "ecdsa",
		}
	}

	/// The keystore's public keys of `key_type` that decode as this scheme's keys.
	fn public_keys(self, keystore: &dyn Keystore, key_type: KeyTypeId) -> Vec<Vec<u8>> {
		match self {
			Scheme::Sr25519 => to_vecs(keystore.sr25519_public_keys(key_type)),
			Scheme::Ed25519 => to_vecs(keystore.ed25519_public_keys(key_type)),
			Scheme::Ecdsa => to_vecs(keystore.ecdsa_public_keys(key_type)),
		}
	}
}

fn to_vecs<P: AsRef<[u8]>>(keys: Vec<P>) -> Vec<Vec<u8>> {
	keys.iter().map(|k| k.as_ref().to_vec()).collect()
}

/// A session key type a validator needs, and how an operator loads it.
pub struct SessionKeyType {
	pub name: &'static str,
	pub key_type: KeyTypeId,
	pub scheme: Scheme,
	pub seed_file_var: &'static str,
}

pub const SESSION_KEY_TYPES: [SessionKeyType; 4] = [
	SessionKeyType {
		name: "AURA",
		key_type: AURA,
		scheme: Scheme::Sr25519,
		seed_file_var: "AURA_SEED_FILE",
	},
	SessionKeyType {
		name: "BABE",
		key_type: BABE,
		scheme: Scheme::Sr25519,
		seed_file_var: "BABE_SEED_FILE",
	},
	SessionKeyType {
		name: "GRANDPA",
		key_type: GRANDPA,
		scheme: Scheme::Ed25519,
		seed_file_var: "GRANDPA_SEED_FILE",
	},
	SessionKeyType {
		name: "BEEFY",
		key_type: BEEFY,
		scheme: Scheme::Ecdsa,
		seed_file_var: "BEEFY_SEED_FILE",
	},
];

fn hex_list(keys: &[Vec<u8>]) -> String {
	keys.iter()
		.map(|k| format!("0x{}", hex::encode(k)))
		.collect::<Vec<_>>()
		.join(", ")
}

/// One log line per session key type: its public keys at INFO, or an ERROR when the keystore
/// holds no usable key of that type or cannot be listed.
///
/// Only lists public keys, so no key file is opened. `keystore_path` goes into the
/// `key insert` hint.
pub fn report(keystore: &dyn Keystore, keystore_path: Option<&Path>) -> Vec<(Level, String)> {
	let keystore_path =
		keystore_path.map_or_else(|| "<keystore path>".into(), |p| p.display().to_string());
	SESSION_KEY_TYPES
		.iter()
		.map(|t| {
			let key_type = String::from_utf8_lossy(&t.key_type.0);
			// The scheme-specific listings below swallow keystore errors; this one reports them.
			let all = match keystore.keys(t.key_type) {
				Ok(keys) => keys,
				Err(err) => {
					return (
						Level::Error,
						format!(
							"Cannot list {name} keys (key type `{key_type}`) in keystore \
							 {keystore_path}: {err}",
							name = t.name,
						),
					);
				},
			};
			let usable = t.scheme.public_keys(keystore, t.key_type);
			if !usable.is_empty() {
				return (
					Level::Info,
					format!(
						"{name} key(s) (key type `{key_type}`) in keystore: {keys}",
						name = t.name,
						keys = hex_list(&usable),
					),
				);
			}
			let other_scheme = if all.is_empty() {
				String::new()
			} else {
				format!(
					" Found only key(s) of another scheme, which {name} cannot use: {keys}.",
					name = t.name,
					keys = hex_list(&all),
				)
			};
			(
				Level::Error,
				format!(
					"No {name} key (key type `{key_type}`, {scheme}) in keystore: this validator \
					 cannot use it once in a committee.{other_scheme} Load one with {var} or \
					 `midnight-node key insert --chain <chain spec> --keystore-path \
					 {keystore_path} --key-type {key_type} --scheme {scheme} --suri <secret>`",
					name = t.name,
					scheme = t.scheme.name(),
					var = t.seed_file_var,
				),
			)
		})
		.collect()
}

/// Logs the [`report`] lines.
pub fn log_session_keys(keystore: &dyn Keystore, keystore_path: Option<&Path>) {
	for (level, line) in report(keystore, keystore_path) {
		log::log!(target: LOG_TARGET, level, "{line}");
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_keystore::LocalKeystore;
	use sp_keystore::testing::MemoryKeystore;

	const PATH: &str = "/data/chains/x/keystore";

	fn full_keystore() -> MemoryKeystore {
		let keystore = MemoryKeystore::new();
		keystore.sr25519_generate_new(AURA, Some("//Alice")).unwrap();
		keystore.sr25519_generate_new(BABE, Some("//Alice//babe")).unwrap();
		keystore.ed25519_generate_new(GRANDPA, Some("//Alice")).unwrap();
		keystore.ecdsa_generate_new(BEEFY, Some("//Alice")).unwrap();
		keystore
	}

	fn levels(lines: &[(Level, String)]) -> Vec<Level> {
		lines.iter().map(|(level, _)| *level).collect()
	}

	#[test]
	fn keystore_with_every_type_logs_its_keys_and_no_error() {
		let keystore = full_keystore();
		let beefy = keystore.ecdsa_public_keys(BEEFY)[0];

		let lines = report(&keystore, Some(Path::new(PATH)));

		assert_eq!(levels(&lines), [Level::Info; 4]);
		assert!(lines[3].1.starts_with("BEEFY key(s) (key type `beef`) in keystore: 0x"));
		assert!(lines[3].1.contains(&hex::encode(beefy)));
	}

	#[test]
	fn missing_types_log_an_error_with_how_to_load_them() {
		let keystore = MemoryKeystore::new();
		keystore.sr25519_generate_new(AURA, Some("//Alice")).unwrap();
		keystore.ed25519_generate_new(GRANDPA, Some("//Alice")).unwrap();

		let lines = report(&keystore, Some(Path::new(PATH)));

		assert_eq!(levels(&lines), [Level::Info, Level::Error, Level::Info, Level::Error]);
		let babe = &lines[1].1;
		assert!(babe.starts_with("No BABE key (key type `babe`, sr25519) in keystore"));
		assert!(babe.contains("BABE_SEED_FILE"));
		assert!(babe.contains(&format!(
			"--keystore-path {PATH} --key-type babe --scheme sr25519 --suri <secret>"
		)));
	}

	#[test]
	fn key_of_another_scheme_does_not_count() {
		let keystore = MemoryKeystore::new();
		keystore.sr25519_generate_new(AURA, Some("//Alice")).unwrap();
		keystore.sr25519_generate_new(BABE, Some("//Alice//babe")).unwrap();
		keystore.ed25519_generate_new(GRANDPA, Some("//Alice")).unwrap();
		// A BEEFY key inserted as sr25519 instead of ecdsa.
		let sr25519 = keystore.sr25519_generate_new(BEEFY, Some("//Alice")).unwrap();

		let lines = report(&keystore, None);

		assert_eq!(lines[3].0, Level::Error);
		assert!(lines[3].1.contains("another scheme"));
		assert!(lines[3].1.contains(&hex::encode(sr25519)));
		assert!(lines[3].1.contains("--keystore-path <keystore path>"));
	}

	#[test]
	fn unreadable_keystore_logs_the_error_not_a_missing_key() {
		let dir = tempfile::TempDir::new().unwrap();
		let path = dir.path().join("keystore");
		let keystore = LocalKeystore::open(&path, None).unwrap();
		std::fs::remove_dir_all(&path).unwrap();

		let lines = report(&keystore, Some(&path));

		assert_eq!(levels(&lines), [Level::Error; 4]);
		assert!(lines.iter().all(|(_, line)| line.starts_with("Cannot list ")), "{lines:?}");
	}
}
