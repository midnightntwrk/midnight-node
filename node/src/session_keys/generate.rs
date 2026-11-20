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

//! Generates a fresh set of session keys straight into the node's keystore by running a
//! runtime's own `SessionKeysGenerationApi`, so the key set is whatever that runtime defines.

use midnight_node_runtime::opaque::Block;
use midnight_primitives_session_info::SessionKeysGenerationApi;
use parity_scale_codec::{Decode, Encode};
use sc_executor::{DEFAULT_HEAP_ALLOC_STRATEGY, WasmExecutor};
use sc_keystore::LocalKeystore;
use sidechain_domain::CROSS_CHAIN_KEY_TYPE_ID;
use sp_api::RuntimeApiInfo;
use sp_core::{
	crypto::{KeyTypeId, SecretString},
	ecdsa,
	traits::{CallContext, CodeExecutor, RuntimeCode, WrappedRuntimeCode},
};
use sp_keystore::{KeystoreExt, KeystorePtr};
use sp_state_machine::BasicExternalities;
use sp_version::RuntimeVersion;
use std::{
	collections::BTreeSet,
	path::{Path, PathBuf},
	sync::Arc,
};

use crate::service::HostFunctions;

/// The `SessionKeysGenerationApi` version whose signature this tool calls.
const GENERATION_API_VERSION: u32 = 1;

pub type CodeHash = [u8; 32];

/// A generated key's type, raw public key and proof of possession.
type GeneratedKey = (KeyTypeId, Vec<u8>, Vec<u8>);

/// The message of the password check signature. Domain-separated, so the signature means
/// nothing to any other protocol that `crch` signs for; it is also dropped right away.
const PASSWORD_CHECK_MESSAGE: &[u8] =
	b"midnight-node generate-session-keys: keystore password check";

/// Public keys of a generated session key set.
#[derive(Debug)]
pub struct GeneratedKeys {
	pub cross_chain: ecdsa::Public,
	pub keys: Vec<(KeyTypeId, Vec<u8>)>,
	/// Each key's proof of possession for `cross_chain`, as the runtime makes it.
	pub signatures: Vec<(KeyTypeId, Vec<u8>)>,
	pub code_hash: CodeHash,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
	#[error("keystore directory {0} does not exist")]
	MissingKeystoreDir(PathBuf),
	#[error("keystore: {0}")]
	Keystore(String),
	#[error("expected exactly one `crch` key in the keystore, found {0}")]
	CrossChainKeyCount(usize),
	#[error("the keystore cannot sign with its `crch` key: wrong keystore password? ({0})")]
	WrongPassword(String),
	#[error("the runtime does not run with this node's host functions: {0}")]
	Instantiation(String),
	#[error(
		"this tool needs a runtime with SessionKeysGenerationApi version {}, the given one has {:?}",
		GENERATION_API_VERSION,
		.0
	)]
	UnsupportedApiVersion(Option<u32>),
	#[error("{reason}. Nothing was deleted; files written to the keystore: {new_files:?}")]
	AfterWrite { reason: String, new_files: Vec<String> },
}

/// Generates the session keys `code` defines into the keystore at `keystore_path`.
#[cfg(test)]
pub fn generate(
	keystore_path: &Path,
	password: Option<SecretString>,
	code: &[u8],
) -> Result<GeneratedKeys, Error> {
	NodeKeystore::open(keystore_path, password)?.generate(code)
}

/// A node keystore that holds exactly one `crch` key.
pub struct NodeKeystore {
	path: PathBuf,
	keystore: KeystorePtr,
	cross_chain: ecdsa::Public,
}

impl NodeKeystore {
	pub fn open(path: &Path, password: Option<SecretString>) -> Result<Self, Error> {
		// Checked first, as `LocalKeystore::open` creates a missing directory.
		if !path.is_dir() {
			return Err(Error::MissingKeystoreDir(path.to_path_buf()));
		}
		let keystore: KeystorePtr = Arc::new(
			LocalKeystore::open(path, password).map_err(|e| Error::Keystore(e.to_string()))?,
		);
		let cross_chain = match keystore.ecdsa_public_keys(CROSS_CHAIN_KEY_TYPE_ID).as_slice() {
			[key] => *key,
			keys => return Err(Error::CrossChainKeyCount(keys.len())),
		};
		// The keystore derives keys from their stored phrase and the password, so a wrong
		// password fails this signature. The secret stays inside the keystore.
		match keystore.ecdsa_sign(CROSS_CHAIN_KEY_TYPE_ID, &cross_chain, PASSWORD_CHECK_MESSAGE) {
			Ok(Some(_)) => {},
			Ok(None) => return Err(Error::WrongPassword("no signature".into())),
			Err(e) => return Err(Error::WrongPassword(e.to_string())),
		}
		Ok(Self { path: path.to_path_buf(), keystore, cross_chain })
	}

	pub fn cross_chain(&self) -> &ecdsa::Public {
		&self.cross_chain
	}

	/// Generates the session keys `code` defines into this keystore.
	///
	/// Every check runs before anything is written. A failure after that keeps whatever was
	/// written and names the new keystore files.
	pub fn generate(self, code: &[u8]) -> Result<GeneratedKeys, Error> {
		let Self { path, keystore, cross_chain } = self;
		let mut runtime = WasmRuntime::new(code, keystore);
		let api_version = runtime
			.version()
			.map_err(Error::Instantiation)?
			.api_version(&<dyn SessionKeysGenerationApi<Block>>::ID);
		if api_version != Some(GENERATION_API_VERSION) {
			return Err(Error::UnsupportedApiVersion(api_version));
		}

		let before = file_names(&path).map_err(|e| Error::Keystore(e.to_string()))?;
		let generated =
			runtime.generate_session_keys_with_proofs(cross_chain).and_then(|generated| {
				let (keys, signatures): (Vec<_>, Vec<_>) = generated
					.into_iter()
					.map(|(key_type, public, signature)| {
						((key_type, public), (key_type, signature))
					})
					.unzip();
				let after = file_names(&path).map_err(|e| format!("listing the keystore: {e}"))?;
				let unexpected = unexpected_files(&before, &after, &keys);
				if !unexpected.is_empty() {
					return Err(format!(
						"the runtime wrote keys it does not return as session keys: {unexpected:?}"
					));
				}
				Ok((keys, signatures))
			});
		let (keys, signatures) = keep_written(&path, &before, generated)?;
		Ok(GeneratedKeys {
			cross_chain,
			keys,
			signatures,
			code_hash: sp_crypto_hashing::blake2_256(code),
		})
	}
}

/// Names of the keystore files, which are made of key types and public keys only.
fn file_names(keystore_path: &Path) -> std::io::Result<BTreeSet<String>> {
	std::fs::read_dir(keystore_path)?
		.map(|entry| Ok(entry?.file_name().to_string_lossy().into_owned()))
		.collect()
}

/// Files new since `before` that are not the keystore file of one of `keys`.
fn unexpected_files(
	before: &BTreeSet<String>,
	after: &BTreeSet<String>,
	keys: &[(KeyTypeId, Vec<u8>)],
) -> Vec<String> {
	let expected: BTreeSet<String> = keys
		.iter()
		.map(|(key_type, public)| format!("{}{}", hex::encode(key_type.0), hex::encode(public)))
		.collect();
	after
		.difference(before)
		.filter(|name| !expected.contains(*name))
		.cloned()
		.collect()
}

/// Keeps whatever was written and names the files that are new since `before`.
fn keep_written<T>(
	keystore_path: &Path,
	before: &BTreeSet<String>,
	result: Result<T, String>,
) -> Result<T, Error> {
	result.map_err(|reason| {
		let new_files = match file_names(keystore_path) {
			Ok(after) => after.difference(before).cloned().collect(),
			Err(e) => vec![format!("(listing the keystore failed: {e})")],
		};
		Error::AfterWrite { reason, new_files }
	})
}

/// Runs runtime calls of `code` with `keystore` exposed to it. Missing host functions fail
/// the instantiation.
struct WasmRuntime<'a> {
	executor: WasmExecutor<HostFunctions>,
	code: WrappedRuntimeCode<'a>,
	code_hash: Vec<u8>,
	ext: BasicExternalities,
}

impl<'a> WasmRuntime<'a> {
	fn new(code: &'a [u8], keystore: KeystorePtr) -> Self {
		let executor = WasmExecutor::<HostFunctions>::builder()
			.with_onchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.with_offchain_heap_alloc_strategy(DEFAULT_HEAP_ALLOC_STRATEGY)
			.with_allow_missing_host_functions(false)
			.build();
		let mut ext = BasicExternalities::new_empty();
		ext.register_extension(KeystoreExt(keystore));
		Self {
			executor,
			code: WrappedRuntimeCode(code.into()),
			code_hash: sp_crypto_hashing::blake2_256(code).to_vec(),
			ext,
		}
	}

	fn call<R: Decode>(&mut self, method: &str, data: &[u8]) -> Result<R, String> {
		let runtime_code = RuntimeCode {
			code_fetcher: &self.code,
			heap_pages: None,
			hash: self.code_hash.clone(),
		};
		let output = self
			.executor
			.call(&mut self.ext, &runtime_code, method, data, CallContext::Offchain)
			.0
			.map_err(|e| format!("{method}: {e}"))?;
		R::decode(&mut &output[..]).map_err(|e| format!("decoding the result of {method}: {e}"))
	}

	fn version(&mut self) -> Result<RuntimeVersion, String> {
		self.call("Core_version", &[])
	}

	fn generate_session_keys_with_proofs(
		&mut self,
		cross_chain: ecdsa::Public,
	) -> Result<Vec<GeneratedKey>, String> {
		self.call(
			"SessionKeysGenerationApi_generate_session_keys_with_proofs",
			&cross_chain.encode(),
		)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use midnight_node_runtime::{WASM_BINARY, WASM_BINARY_BLOATY};
	use sp_core::{
		crypto::key_types::{AURA, BABE},
		sr25519,
	};
	use sp_keystore::Keystore;
	use sp_runtime::traits::OpaqueKeys;
	use tempfile::TempDir;

	fn wasm() -> &'static [u8] {
		WASM_BINARY.expect("tests need the runtime wasm; build without SKIP_WASM_BUILD")
	}

	fn open(dir: &Path, password: Option<&str>) -> LocalKeystore {
		LocalKeystore::open(dir, password.map(|p| SecretString::new(p.into()))).unwrap()
	}

	/// A keystore holding `crch_count` cross-chain keys.
	fn keystore_dir(crch_count: usize) -> TempDir {
		let dir = TempDir::new().unwrap();
		let keystore = open(dir.path(), None);
		for _ in 0..crch_count {
			keystore.ecdsa_generate_new(CROSS_CHAIN_KEY_TYPE_ID, None).unwrap();
		}
		dir
	}

	fn file_names(dir: &Path) -> BTreeSet<String> {
		std::fs::read_dir(dir)
			.unwrap()
			.map(|e| e.unwrap().file_name().into_string().unwrap())
			.collect()
	}

	fn file_name(key_type: KeyTypeId, public: &[u8]) -> String {
		format!("{}{}", hex::encode(key_type.0), hex::encode(public))
	}

	/// The bloaty runtime with its declared `SessionKeysGenerationApi` version set to `version`.
	fn wasm_with_generation_api_version(version: u8) -> Vec<u8> {
		let id = <dyn SessionKeysGenerationApi<Block>>::ID;
		let declared = [&id[..], &GENERATION_API_VERSION.to_le_bytes()].concat();
		let mut code = WASM_BINARY_BLOATY.unwrap().to_vec();
		let mut patched = 0;
		for at in 0..code.len() - declared.len() {
			if code[at..at + declared.len()] == declared[..] {
				code[at + id.len()] = version;
				patched += 1;
			}
		}
		assert!(patched > 0, "SessionKeysGenerationApi declaration not found in the runtime");
		code
	}

	/// A wasm module that imports a host function no node provides.
	fn wasm_with_unknown_import() -> Vec<u8> {
		fn name(s: &str) -> Vec<u8> {
			[&[s.len() as u8][..], s.as_bytes()].concat()
		}
		fn section(id: u8, body: Vec<u8>) -> Vec<u8> {
			[vec![id, body.len() as u8], body].concat()
		}
		let types = section(1, vec![1, 0x60, 0, 0]);
		let imports = section(
			2,
			[
				vec![2],
				name("env"),
				name("memory"),
				vec![0x02, 0x00, 0x01],
				name("env"),
				name("ext_unknown_host_function_version_1"),
				vec![0x00, 0x00],
			]
			.concat(),
		);
		[b"\0asm".to_vec(), vec![1, 0, 0, 0], types, imports].concat()
	}

	fn assert_aborts(dir: &Path, code: &[u8], expected: impl Fn(&Error) -> bool) {
		let before = file_names(dir);
		let err = generate(dir, None, code).unwrap_err();
		assert!(expected(&err), "unexpected error: {err}");
		assert_eq!(file_names(dir), before, "the keystore changed");
	}

	#[test]
	fn aborts_on_a_runtime_without_the_generation_api() {
		let dir = keystore_dir(1);
		let code = substrate_test_runtime_client::runtime::wasm_binary_unwrap();
		assert_aborts(dir.path(), code, |e| matches!(e, Error::UnsupportedApiVersion(None)));
	}

	#[test]
	fn adds_one_file_per_generated_key_and_no_cross_chain_key() {
		let dir = keystore_dir(1);
		let before = file_names(dir.path());

		let generated = generate(dir.path(), None, wasm()).unwrap();

		let added: BTreeSet<_> = file_names(dir.path()).difference(&before).cloned().collect();
		let expected: BTreeSet<_> = generated
			.keys
			.iter()
			.map(|(key_type, public)| file_name(*key_type, public))
			.collect();
		assert_eq!(added, expected);
		assert_eq!(added.len(), generated.keys.len());
		assert!(!generated.keys.iter().any(|(key_type, _)| *key_type == CROSS_CHAIN_KEY_TYPE_ID));
		let key_types: Vec<_> = generated.keys.iter().map(|(key_type, _)| *key_type).collect();
		assert_eq!(key_types, midnight_node_runtime::opaque::SessionKeys::key_ids());
	}

	#[test]
	fn generates_from_compressed_and_uncompressed_code() {
		let bloaty = WASM_BINARY_BLOATY.unwrap();
		let compressed = sp_maybe_compressed_blob::compress_strongly(
			bloaty,
			sp_maybe_compressed_blob::CODE_BLOB_BOMB_LIMIT,
		)
		.unwrap();
		assert_ne!(compressed, bloaty);

		for code in [bloaty, &compressed[..]] {
			let dir = keystore_dir(1);
			let generated = generate(dir.path(), None, code).unwrap();
			let key_types: Vec<_> = generated.keys.iter().map(|(key_type, _)| *key_type).collect();
			assert_eq!(key_types, midnight_node_runtime::opaque::SessionKeys::key_ids());
		}
	}

	#[test]
	fn aborts_without_creating_a_missing_keystore_dir() {
		let parent = TempDir::new().unwrap();
		let dir = parent.path().join("keystore");

		let err = generate(&dir, None, wasm()).unwrap_err();

		assert!(matches!(err, Error::MissingKeystoreDir(_)), "{err}");
		assert!(!dir.exists());
	}

	#[test]
	fn aborts_without_a_cross_chain_key() {
		let dir = keystore_dir(0);
		assert_aborts(dir.path(), wasm(), |e| matches!(e, Error::CrossChainKeyCount(0)));
	}

	#[test]
	fn aborts_with_two_cross_chain_keys() {
		let dir = keystore_dir(2);
		assert_aborts(dir.path(), wasm(), |e| matches!(e, Error::CrossChainKeyCount(2)));
	}

	#[test]
	fn aborts_on_another_generation_api_version() {
		let dir = keystore_dir(1);
		let code = wasm_with_generation_api_version(2);
		assert_aborts(dir.path(), &code, |e| matches!(e, Error::UnsupportedApiVersion(Some(2))));
	}

	#[test]
	fn proves_possession_for_the_keystores_cross_chain_key() {
		use midnight_node_runtime::{AccountId, CrossChainPublic, opaque::SessionKeys};

		let dir = keystore_dir(1);
		let crch = open(dir.path(), None).ecdsa_public_keys(CROSS_CHAIN_KEY_TYPE_ID)[0];

		let generated = generate(dir.path(), None, wasm()).unwrap();

		assert_eq!(generated.cross_chain, crch);
		let keys: Vec<u8> = generated.keys.iter().flat_map(|(_, public)| public.clone()).collect();
		let keys = SessionKeys::decode(&mut &keys[..]).unwrap();
		let proof: Vec<u8> = generated.signatures.iter().flat_map(|(_, s)| s.clone()).collect();
		let owner = AccountId::from(CrossChainPublic::from(crch)).encode();
		assert!(keys.ownership_proof_is_valid(&owner, &proof));
	}

	#[test]
	fn aborts_on_a_missing_host_function() {
		let dir = keystore_dir(1);
		let code = wasm_with_unknown_import();
		assert_aborts(
			dir.path(),
			&code,
			|e| matches!(e, Error::Instantiation(msg) if msg.contains("ext_unknown_host_function_version_1")),
		);
	}

	#[test]
	fn a_failure_after_writing_keeps_and_names_the_new_files() {
		let dir = keystore_dir(1);
		let before = file_names(dir.path());
		let written = open(dir.path(), None).sr25519_generate_new(AURA, None).unwrap();

		let err =
			keep_written::<()>(dir.path(), &before, Err("decoding failed".into())).unwrap_err();

		let new_file = file_name(AURA, &written.0);
		assert!(
			matches!(&err, Error::AfterWrite { new_files, .. } if *new_files == vec![new_file.clone()])
		);
		assert!(err.to_string().contains(&new_file));
		assert!(dir.path().join(&new_file).exists());
	}

	#[test]
	fn reports_new_files_that_are_not_returned_session_keys() {
		let before = BTreeSet::from(["crch01".to_string(), "aura01".to_string()]);
		let keys = vec![(AURA, vec![0x02]), (BABE, vec![0x03])];
		let mut after = before.clone();
		after.extend([file_name(AURA, &[0x02]), file_name(BABE, &[0x03])]);
		assert!(unexpected_files(&before, &after, &keys).is_empty());

		after.insert("63726368aa".to_string());
		assert_eq!(unexpected_files(&before, &after, &keys), ["63726368aa"]);
	}

	#[test]
	fn aborts_on_a_wrong_password() {
		let dir = TempDir::new().unwrap();
		open(dir.path(), Some("secret"))
			.ecdsa_generate_new(CROSS_CHAIN_KEY_TYPE_ID, None)
			.unwrap();
		let before = file_names(dir.path());

		let err =
			generate(dir.path(), Some(SecretString::new("wrong".into())), wasm()).unwrap_err();

		assert!(matches!(err, Error::WrongPassword(_)), "{err}");
		assert_eq!(file_names(dir.path()), before, "the keystore changed");
	}

	#[test]
	fn keys_generated_with_a_password_are_usable_with_that_password() {
		let dir = TempDir::new().unwrap();
		open(dir.path(), Some("secret"))
			.ecdsa_generate_new(CROSS_CHAIN_KEY_TYPE_ID, None)
			.unwrap();

		let generated =
			generate(dir.path(), Some(SecretString::new("secret".into())), wasm()).unwrap();

		let (_, babe) = generated.keys.iter().find(|(key_type, _)| *key_type == BABE).unwrap();
		let babe = sr25519::Public::from_raw(babe[..].try_into().unwrap());
		assert!(
			open(dir.path(), Some("secret"))
				.sr25519_sign(BABE, &babe, b"msg")
				.unwrap()
				.is_some()
		);
		assert!(open(dir.path(), Some("wrong")).sr25519_sign(BABE, &babe, b"msg").is_err());
	}

	#[test]
	fn does_not_read_existing_key_files_other_than_crch() {
		let dir = keystore_dir(1);
		open(dir.path(), None).sr25519_generate_new(AURA, None).unwrap();
		// A directory in place of each key file keeps the name listed but fails every read,
		// even as root. Only `crch` is read, by the password check.
		let crch_prefix = hex::encode(CROSS_CHAIN_KEY_TYPE_ID.0);
		for name in file_names(dir.path()).into_iter().filter(|n| !n.starts_with(&crch_prefix)) {
			let file = dir.path().join(name);
			std::fs::remove_file(&file).unwrap();
			std::fs::create_dir(&file).unwrap();
			assert!(std::fs::read(&file).is_err());
		}

		generate(dir.path(), None, wasm()).unwrap();
	}
}
