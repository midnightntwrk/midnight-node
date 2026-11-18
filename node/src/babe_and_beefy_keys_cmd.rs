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

//! `babe-and-beefy-keys` subcommand: makes sure the node keystore holds BABE and BEEFY keys and
//! prints them for submission to Midnight governance.
//!
//! The BEEFY key reuses the cross-chain (`crch`) seed, since both are ECDSA keys. The BABE key is
//! freshly generated when missing.

#![allow(clippy::result_large_err)]

use std::path::{Path, PathBuf};

use sc_cli::{KeystoreParams, SharedParams, SubstrateCli};
use sc_keystore::LocalKeystore;
use sc_service::config::{BasePath, KeystoreConfig};
use sp_core::{
	Pair,
	crypto::{ExposeSecret, KeyTypeId, SecretString},
	ecdsa, sr25519,
};
use sp_keystore::Keystore;

const CROSS_CHAIN: KeyTypeId = KeyTypeId(*b"crch");
const BEEFY: KeyTypeId = sp_core::crypto::key_types::BEEFY;
const BABE: KeyTypeId = sp_core::crypto::key_types::BABE;

/// Ensure BABE and BEEFY keys are present in the node keystore and print them.
#[derive(Debug, Clone, clap::Parser)]
pub struct BabeAndBeefyKeysCmd {
	#[allow(missing_docs)]
	#[clap(flatten)]
	pub shared_params: SharedParams,

	#[allow(missing_docs)]
	#[clap(flatten)]
	pub keystore_params: KeystoreParams,
}

/// A key read from (or written to) the keystore.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEntry {
	/// Secret URI as stored in the keystore file.
	pub seed: String,
	/// Raw public key bytes.
	pub public: Vec<u8>,
}

/// Result of [`ensure_babe_and_beefy_keys`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeysOutcome {
	/// No cross-chain key in the keystore; nothing was written.
	CrossChainMissing,
	/// All three keys are now present in the keystore.
	Present { cross_chain: KeyEntry, beefy: KeyEntry, babe: KeyEntry },
}

impl BabeAndBeefyKeysCmd {
	/// Run the command. Keystore resolution mirrors `key insert`.
	pub fn run<C: SubstrateCli>(&self, cli: &C) -> sc_cli::Result<()> {
		let base_path = self
			.shared_params
			.base_path()?
			.unwrap_or_else(|| BasePath::from_project("", "", &C::executable_name()));
		let chain_id = self.shared_params.chain_id(self.shared_params.is_dev());
		let chain_spec = cli.load_spec(&chain_id)?;
		let config_dir = base_path.config_dir(chain_spec.id());

		let (path, password) = match self.keystore_params.keystore_config(&config_dir)? {
			KeystoreConfig::Path { path, password } => (path, password),
			_ => unreachable!("keystore_config always returns path and password; qed"),
		};
		let path = std::path::absolute(&path).unwrap_or(path);

		match ensure_babe_and_beefy_keys(&path, password)? {
			KeysOutcome::CrossChainMissing => {
				let msg = format!(
					"'cross-chain' couldn't be found in the keystore at {}, run again with proper \
					 paths and chain-spec parameters.",
					path.display()
				);
				println!("{msg}");
				Err(sc_cli::Error::Input("cross-chain key not found in the keystore".into()))
			},
			KeysOutcome::Present { cross_chain, beefy, babe } => {
				print!("{}", render(&path, &cross_chain, &beefy, &babe));
				Ok(())
			},
		}
	}
}

/// Load the cross-chain key, then make sure BEEFY (same seed as cross-chain) and BABE (fresh
/// seed) keys exist in the keystore at `path`.
pub fn ensure_babe_and_beefy_keys(
	path: &Path,
	password: Option<SecretString>,
) -> sc_cli::Result<KeysOutcome> {
	if !path.is_dir() {
		return Ok(KeysOutcome::CrossChainMissing);
	}
	let pw = password.as_ref().map(|p| p.expose_secret().as_str());

	let Some(cross_chain) = find_key::<ecdsa::Pair>(path, CROSS_CHAIN, pw)? else {
		return Ok(KeysOutcome::CrossChainMissing);
	};

	let keystore = LocalKeystore::open(path, password.clone())?;

	let beefy = match find_key::<ecdsa::Pair>(path, BEEFY, pw)? {
		Some(key) => key,
		None => {
			let public = public_from_seed::<ecdsa::Pair>(&cross_chain.seed, pw)?;
			insert(&keystore, BEEFY, &cross_chain.seed, &public)?;
			KeyEntry { seed: cross_chain.seed.clone(), public }
		},
	};

	let babe = match find_key::<sr25519::Pair>(path, BABE, pw)? {
		Some(key) => key,
		None => {
			let (pair, phrase, _) = sr25519::Pair::generate_with_phrase(pw);
			let public = pair.public().0.to_vec();
			insert(&keystore, BABE, &phrase, &public)?;
			KeyEntry { seed: phrase, public }
		},
	};

	Ok(KeysOutcome::Present { cross_chain, beefy, babe })
}

fn insert(
	keystore: &LocalKeystore,
	key_type: KeyTypeId,
	seed: &str,
	public: &[u8],
) -> sc_cli::Result<()> {
	keystore.insert(key_type, seed, public).map_err(|()| {
		sc_cli::Error::Input(format!(
			"failed to write '{}' key to the keystore",
			key_type_name(key_type)
		))
	})
}

/// Find the single key of `key_type` in the keystore directory and check that its stored seed
/// derives the public key encoded in the file name.
///
/// Reads files directly because `LocalKeystore` does not expose stored seeds. Files are named
/// `<hex key type><hex public key>` and hold the seed as a JSON string.
fn find_key<P: Pair>(
	dir: &Path,
	key_type: KeyTypeId,
	password: Option<&str>,
) -> sc_cli::Result<Option<KeyEntry>> {
	let prefix = hex::encode(key_type.0);
	let mut found: Vec<(PathBuf, Vec<u8>)> = Vec::new();
	for entry in std::fs::read_dir(dir)? {
		let file = entry?.path();
		let Some(name) = file.file_name().and_then(|n| n.to_str()) else { continue };
		let Some(public_hex) = name.strip_prefix(&prefix) else { continue };
		let Ok(public) = hex::decode(public_hex) else { continue };
		if public.is_empty() {
			continue;
		}
		found.push((file, public));
	}

	let name = key_type_name(key_type);
	let (file, public) = match found.len() {
		0 => return Ok(None),
		1 => found.remove(0),
		_ => {
			let files: Vec<String> = found.iter().map(|(f, _)| f.display().to_string()).collect();
			return Err(sc_cli::Error::Input(format!(
				"found {} '{name}' keys in the keystore, expected at most one: {}",
				found.len(),
				files.join(", ")
			)));
		},
	};

	let contents = std::fs::read_to_string(&file)?;
	let seed: String = serde_json::from_str(&contents).map_err(|e| {
		sc_cli::Error::Input(format!("failed to parse '{name}' key file {}: {e}", file.display()))
	})?;
	let derived = public_from_seed::<P>(&seed, password)?;
	if derived != public {
		return Err(sc_cli::Error::Input(format!(
			"'{name}' key file {} holds a seed that does not match its public key \
			 (wrong keystore password?)",
			file.display()
		)));
	}
	Ok(Some(KeyEntry { seed, public }))
}

fn public_from_seed<P: Pair>(seed: &str, password: Option<&str>) -> sc_cli::Result<Vec<u8>> {
	let pair = P::from_string(seed, password)
		.map_err(|e| sc_cli::Error::Input(format!("invalid key seed in keystore: {e:?}")))?;
	Ok(pair.public().as_ref().to_vec())
}

fn key_type_name(key_type: KeyTypeId) -> &'static str {
	match key_type {
		CROSS_CHAIN => "cross-chain",
		BEEFY => "beef",
		BABE => "babe",
		_ => "unknown",
	}
}

fn render(path: &Path, cross_chain: &KeyEntry, beefy: &KeyEntry, babe: &KeyEntry) -> String {
	let hex_pub = |k: &KeyEntry| format!("0x{}", hex::encode(&k.public));
	format!(
		"\
===================== PRIVATE =====================
CROSS-CHAIN key seed: {cc_seed}
BEEFY key seed: {beefy_seed}
BABE key seed: {babe_seed}

===================== PUBLIC ======================
CROSS-CHAIN key public key: {cc_pub}
BEEFY public key: {beefy_pub}
BABE public key: {babe_pub}

The PRIVATE part has been stored in the keystore at {path}.
Do not share it with anyone.
Copy the PUBLIC part and send it to Midnight governance.
",
		cc_seed = cross_chain.seed,
		beefy_seed = beefy.seed,
		babe_seed = babe.seed,
		cc_pub = hex_pub(cross_chain),
		beefy_pub = hex_pub(beefy),
		babe_pub = hex_pub(babe),
		path = path.display(),
	)
}

#[cfg(test)]
mod tests {
	use super::*;
	use tempfile::TempDir;

	fn keystore(dir: &Path) -> LocalKeystore {
		LocalKeystore::open(dir, None).unwrap()
	}

	fn insert_crch(dir: &Path) -> (String, Vec<u8>) {
		let (pair, phrase, _) = ecdsa::Pair::generate_with_phrase(None);
		let public = pair.public().0.to_vec();
		keystore(dir).insert(CROSS_CHAIN, &phrase, &public).unwrap();
		(phrase, public)
	}

	fn count_keys(dir: &Path, key_type: KeyTypeId) -> usize {
		keystore(dir).keys(key_type).unwrap().len()
	}

	#[test]
	fn missing_keystore_dir_reports_cross_chain_missing() {
		let tmp = TempDir::new().unwrap();
		let path = tmp.path().join("does-not-exist");
		assert_eq!(
			ensure_babe_and_beefy_keys(&path, None).unwrap(),
			KeysOutcome::CrossChainMissing
		);
		assert!(!path.exists());
	}

	#[test]
	fn missing_cross_chain_writes_nothing() {
		let tmp = TempDir::new().unwrap();
		assert_eq!(
			ensure_babe_and_beefy_keys(tmp.path(), None).unwrap(),
			KeysOutcome::CrossChainMissing
		);
		assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0);
	}

	#[test]
	fn inserts_beefy_from_cross_chain_seed_and_generates_babe() {
		let tmp = TempDir::new().unwrap();
		let (crch_seed, crch_pub) = insert_crch(tmp.path());

		let KeysOutcome::Present { cross_chain, beefy, babe } =
			ensure_babe_and_beefy_keys(tmp.path(), None).unwrap()
		else {
			panic!("expected keys");
		};

		assert_eq!(cross_chain, KeyEntry { seed: crch_seed.clone(), public: crch_pub.clone() });
		assert_eq!(beefy, KeyEntry { seed: crch_seed, public: crch_pub });
		assert_eq!(
			sr25519::Pair::from_string(&babe.seed, None).unwrap().public().0.to_vec(),
			babe.public
		);

		let ks = keystore(tmp.path());
		assert!(ks.has_keys(&[(beefy.public.clone(), BEEFY), (babe.public.clone(), BABE)]));
	}

	#[test]
	fn second_run_reuses_existing_keys() {
		let tmp = TempDir::new().unwrap();
		insert_crch(tmp.path());

		let first = ensure_babe_and_beefy_keys(tmp.path(), None).unwrap();
		let second = ensure_babe_and_beefy_keys(tmp.path(), None).unwrap();
		assert_eq!(first, second);
		assert_eq!(count_keys(tmp.path(), BEEFY), 1);
		assert_eq!(count_keys(tmp.path(), BABE), 1);
	}

	#[test]
	fn existing_beefy_and_babe_are_kept() {
		let tmp = TempDir::new().unwrap();
		insert_crch(tmp.path());
		let ks = keystore(tmp.path());
		let (beefy_pair, beefy_seed, _) = ecdsa::Pair::generate_with_phrase(None);
		ks.insert(BEEFY, &beefy_seed, &beefy_pair.public().0).unwrap();
		let (babe_pair, babe_seed, _) = sr25519::Pair::generate_with_phrase(None);
		ks.insert(BABE, &babe_seed, &babe_pair.public().0).unwrap();

		let KeysOutcome::Present { beefy, babe, .. } =
			ensure_babe_and_beefy_keys(tmp.path(), None).unwrap()
		else {
			panic!("expected keys");
		};
		assert_eq!(beefy, KeyEntry { seed: beefy_seed, public: beefy_pair.public().0.to_vec() });
		assert_eq!(babe, KeyEntry { seed: babe_seed, public: babe_pair.public().0.to_vec() });
	}

	#[test]
	fn password_protected_keystore() {
		let tmp = TempDir::new().unwrap();
		let password = Some(SecretString::new("hunter2".into()));
		let (pair, phrase, _) = ecdsa::Pair::generate_with_phrase(Some("hunter2"));
		LocalKeystore::open(tmp.path(), password.clone())
			.unwrap()
			.insert(CROSS_CHAIN, &phrase, &pair.public().0)
			.unwrap();

		let KeysOutcome::Present { beefy, babe, .. } =
			ensure_babe_and_beefy_keys(tmp.path(), password.clone()).unwrap()
		else {
			panic!("expected keys");
		};
		assert_eq!(beefy.public, pair.public().0.to_vec());

		let ks = LocalKeystore::open(tmp.path(), password).unwrap();
		let babe_pub = sr25519::Public::try_from(babe.public.as_slice()).unwrap();
		assert!(ks.sr25519_sign(BABE, &babe_pub, b"msg").unwrap().is_some());

		assert!(ensure_babe_and_beefy_keys(tmp.path(), None).is_err());
	}

	#[test]
	fn multiple_cross_chain_keys_is_an_error() {
		let tmp = TempDir::new().unwrap();
		insert_crch(tmp.path());
		insert_crch(tmp.path());
		assert!(ensure_babe_and_beefy_keys(tmp.path(), None).is_err());
	}
}
