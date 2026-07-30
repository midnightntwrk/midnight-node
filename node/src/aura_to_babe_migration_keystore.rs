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

use std::sync::Arc;

use sp_core::crypto::key_types::{AURA, BABE};
use sp_keystore::Keystore;

const LOG_TARGET: &str = "aura-to-babe-migration-keystore";

/// Keystore wrapper for the AURA-to-BABE consensus migration.
///
/// Operators are expected to insert dedicated BABE keys, but if they don't,
/// their AURA keys will be used instead.
///
/// The fallback only actually helps when the on-chain BABE authority key for
/// this validator equals its AURA public key (otherwise the AURA lookup misses
/// and the fallback does not help).
///
/// Cold path operations log WARN so operators can see they are running on the
/// AURA key and should insert a proper BABE key.
pub(crate) struct AuraToBabeMigrationKeystore<T: Keystore> {
	keystore: T,
}

impl<T: Keystore> AuraToBabeMigrationKeystore<T> {
	pub fn new(keystore: T) -> Self {
		Self { keystore }
	}

	pub fn new_arc(keystore: T) -> Arc<Self> {
		Arc::new(Self::new(keystore))
	}
}

impl<T: Keystore> Keystore for AuraToBabeMigrationKeystore<T> {
	fn sr25519_public_keys(
		&self,
		key_type: sp_runtime::KeyTypeId,
	) -> Vec<sp_core::sr25519::Public> {
		let loaded = self.keystore.sr25519_public_keys(key_type);
		if loaded.is_empty() && key_type == BABE {
			let aura = self.keystore.sr25519_public_keys(AURA);
			if !aura.is_empty() {
				log::warn!(
					target: LOG_TARGET,
					"No BABE keys in keystore; falling back to AURA keys for sr25519_public_keys. \
					 Insert a dedicated BABE key to silence this warning.",
				);
			}
			aura
		} else {
			loaded
		}
	}

	fn sr25519_generate_new(
		&self,
		key_type: sp_runtime::KeyTypeId,
		seed: Option<&str>,
	) -> Result<sp_core::sr25519::Public, sp_keystore::Error> {
		self.keystore.sr25519_generate_new(key_type, seed)
	}

	fn sr25519_sign(
		&self,
		key_type: sp_runtime::KeyTypeId,
		public: &sp_core::sr25519::Public,
		msg: &[u8],
	) -> Result<Option<sp_core::sr25519::Signature>, sp_keystore::Error> {
		if key_type == BABE && self.keystore.sr25519_public_keys(BABE).is_empty() {
			self.keystore.sr25519_sign(AURA, public, msg)
		} else {
			self.keystore.sr25519_sign(key_type, public, msg)
		}
	}

	fn sr25519_vrf_sign(
		&self,
		key_type: sp_runtime::KeyTypeId,
		public: &sp_core::sr25519::Public,
		data: &sp_core::sr25519::vrf::VrfSignData,
	) -> Result<Option<sp_core::sr25519::vrf::VrfSignature>, sp_keystore::Error> {
		if key_type == BABE && self.keystore.sr25519_public_keys(BABE).is_empty() {
			self.keystore.sr25519_vrf_sign(AURA, public, data)
		} else {
			self.keystore.sr25519_vrf_sign(key_type, public, data)
		}
	}

	fn sr25519_vrf_pre_output(
		&self,
		key_type: sp_runtime::KeyTypeId,
		public: &sp_core::sr25519::Public,
		input: &sp_core::sr25519::vrf::VrfInput,
	) -> Result<Option<sp_core::sr25519::vrf::VrfPreOutput>, sp_keystore::Error> {
		if key_type == BABE && self.keystore.sr25519_public_keys(BABE).is_empty() {
			self.keystore.sr25519_vrf_pre_output(AURA, public, input)
		} else {
			self.keystore.sr25519_vrf_pre_output(key_type, public, input)
		}
	}

	fn ed25519_public_keys(
		&self,
		key_type: sp_runtime::KeyTypeId,
	) -> Vec<sp_core::ed25519::Public> {
		self.keystore.ed25519_public_keys(key_type)
	}

	fn ed25519_generate_new(
		&self,
		key_type: sp_runtime::KeyTypeId,
		seed: Option<&str>,
	) -> Result<sp_core::ed25519::Public, sp_keystore::Error> {
		self.keystore.ed25519_generate_new(key_type, seed)
	}

	fn ed25519_sign(
		&self,
		key_type: sp_runtime::KeyTypeId,
		public: &sp_core::ed25519::Public,
		msg: &[u8],
	) -> Result<Option<sp_core::ed25519::Signature>, sp_keystore::Error> {
		self.keystore.ed25519_sign(key_type, public, msg)
	}

	fn ecdsa_public_keys(&self, key_type: sp_runtime::KeyTypeId) -> Vec<sp_core::ecdsa::Public> {
		self.keystore.ecdsa_public_keys(key_type)
	}

	fn ecdsa_generate_new(
		&self,
		key_type: sp_runtime::KeyTypeId,
		seed: Option<&str>,
	) -> Result<sp_core::ecdsa::Public, sp_keystore::Error> {
		self.keystore.ecdsa_generate_new(key_type, seed)
	}

	fn ecdsa_sign(
		&self,
		key_type: sp_runtime::KeyTypeId,
		public: &sp_core::ecdsa::Public,
		msg: &[u8],
	) -> Result<Option<sp_core::ecdsa::Signature>, sp_keystore::Error> {
		self.keystore.ecdsa_sign(key_type, public, msg)
	}

	fn ecdsa_sign_prehashed(
		&self,
		key_type: sp_runtime::KeyTypeId,
		public: &sp_core::ecdsa::Public,
		msg: &[u8; 32],
	) -> Result<Option<sp_core::ecdsa::Signature>, sp_keystore::Error> {
		self.keystore.ecdsa_sign_prehashed(key_type, public, msg)
	}

	fn insert(&self, key_type: sp_runtime::KeyTypeId, suri: &str, public: &[u8]) -> Result<(), ()> {
		self.keystore.insert(key_type, suri, public)
	}

	fn keys(&self, key_type: sp_runtime::KeyTypeId) -> Result<Vec<Vec<u8>>, sp_keystore::Error> {
		let keys = self.keystore.keys(key_type);
		match keys {
			Ok(ks) if ks.is_empty() && key_type == BABE => {
				let aura = self.keystore.keys(AURA);
				if matches!(&aura, Ok(a) if !a.is_empty()) {
					log::warn!(
						target: LOG_TARGET,
						"No BABE keys in keystore; falling back to AURA keys for keys(). \
						 Insert a dedicated BABE key to silence this warning.",
					);
				}
				aura
			},
			_ => keys,
		}
	}

	fn has_keys(&self, public_keys: &[(Vec<u8>, sp_runtime::KeyTypeId)]) -> bool {
		for (key, key_type) in public_keys {
			if self.keystore.has_keys(&[(key.clone(), *key_type)]) {
				continue;
			}
			if *key_type == BABE && self.keystore.has_keys(&[(key.clone(), AURA)]) {
				log::warn!(
					target: LOG_TARGET,
					"No BABE key in keystore for a requested key; falling back to AURA key in has_keys. \
					 Insert a dedicated BABE key to silence this warning.",
				);
				continue;
			}
			return false;
		}
		true
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{
		Pair,
		crypto::{KeyTypeId, key_types::GRANDPA},
		sr25519,
		sr25519::vrf::{VrfInput, VrfSignData, VrfTranscript},
	};
	use sp_keystore::testing::MemoryKeystore;

	const MSG: &[u8] = b"test-message";

	/// The wrapper shares the underlying `MemoryKeystore` (it clones an inner
	/// `Arc`), so keys inserted via the returned inner handle are visible to the
	/// wrapper. Returns `(inner, wrapper)`.
	fn mk_keystore() -> (MemoryKeystore, AuraToBabeMigrationKeystore<MemoryKeystore>) {
		let inner = MemoryKeystore::new();
		let wrapper = AuraToBabeMigrationKeystore::new(inner.clone());
		(inner, wrapper)
	}

	fn gen_key(ks: &MemoryKeystore, key_type: KeyTypeId, seed: &str) -> sr25519::Public {
		ks.sr25519_generate_new(key_type, Some(seed)).unwrap()
	}

	fn raw(public: &sr25519::Public) -> Vec<u8> {
		AsRef::<[u8]>::as_ref(public).to_vec()
	}

	// --- sr25519_public_keys ------------------------------------------------

	#[test]
	fn public_keys_returns_babe_keys_when_present() {
		let (inner, keystore) = mk_keystore();
		let babe = gen_key(&inner, BABE, "//Babe-seed");
		let _aura = gen_key(&inner, AURA, "//Aura-seed");

		let keys = keystore.sr25519_public_keys(BABE);
		assert_eq!(keys, vec![babe]);
	}

	#[test]
	fn public_keys_falls_back_to_aura_when_babe_missing() {
		let (inner, keystore) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");

		assert_eq!(keystore.sr25519_public_keys(BABE), vec![aura]);
	}

	#[test]
	fn public_keys_aura_request_is_passthrough() {
		let (inner, keystore) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");
		gen_key(&inner, BABE, "//Babe-seed");

		// An AURA request must return only AURA keys, never BABE.
		assert_eq!(keystore.sr25519_public_keys(AURA), vec![aura]);
	}

	#[test]
	fn public_keys_empty_when_neither_present() {
		let (_inner, w) = mk_keystore();
		assert!(w.sr25519_public_keys(BABE).is_empty());
	}

	// --- sr25519_sign -------------------------------------------------------

	#[test]
	fn sign_babe_falls_back_to_aura_key() {
		let (inner, keystore) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");

		let sig = keystore.sr25519_sign(BABE, &aura, MSG).unwrap().unwrap();
		assert!(sr25519::Pair::verify(&sig, MSG, &aura));
	}

	#[test]
	fn sign_babe_uses_babe_key_when_present() {
		let (inner, keystore) = mk_keystore();
		let babe = gen_key(&inner, BABE, "//Babe-seed");

		let sig = keystore.sr25519_sign(BABE, &babe, MSG).unwrap().unwrap();
		assert!(sr25519::Pair::verify(&sig, MSG, &babe));
	}

	/// Regression: signing with a non-BABE key type must use that key type, not
	/// hardcode BABE (which would return `None` for a valid AURA request).
	#[test]
	fn sign_with_aura_key_type_works() {
		let (inner, w) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");

		let sig = w.sr25519_sign(AURA, &aura, MSG).unwrap().unwrap();
		assert!(sr25519::Pair::verify(&sig, MSG, &aura));
	}

	#[test]
	fn sign_babe_unknown_key_returns_none() {
		let (inner, w) = mk_keystore();
		gen_key(&inner, AURA, "//Aura-seed");
		let stranger = sr25519::Pair::from_string("//Stranger", None).unwrap().public();

		assert!(w.sr25519_sign(BABE, &stranger, MSG).unwrap().is_none());
	}

	// --- VRF ----------------------------------------------------------------

	#[test]
	fn vrf_sign_falls_back_to_aura_key() {
		let (inner, w) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");
		let data: VrfSignData = VrfTranscript::new(b"label", &[(b"domain", b"data")]).into();

		let sig = w.sr25519_vrf_sign(BABE, &aura, &data).unwrap();
		assert!(sig.is_some(), "VRF sign must fall back to the AURA key");
	}

	#[test]
	fn vrf_pre_output_falls_back_to_aura_key() {
		let (inner, w) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");
		let input = VrfInput::new(b"label", &[(b"domain", b"data")]);

		let pre = w.sr25519_vrf_pre_output(BABE, &aura, &input).unwrap();
		assert!(pre.is_some(), "VRF pre-output must fall back to the AURA key");
	}

	// --- keys ---------------------------------------------------------------

	#[test]
	fn keys_falls_back_to_aura_when_babe_missing() {
		let (inner, w) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");

		assert_eq!(w.keys(BABE).unwrap(), vec![raw(&aura)]);
	}

	#[test]
	fn keys_returns_babe_keys_when_present() {
		let (inner, w) = mk_keystore();
		let babe = gen_key(&inner, BABE, "//Babe-seed");
		gen_key(&inner, AURA, "//Aura-seed");

		assert_eq!(w.keys(BABE).unwrap(), vec![raw(&babe)]);
	}

	// --- has_keys -----------------------------------------------------------

	#[test]
	fn has_keys_falls_back_to_aura_for_babe() {
		let (inner, w) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");

		assert!(w.has_keys(&[(raw(&aura), BABE)]));
		assert!(w.has_keys(&[(raw(&aura), AURA)]));
	}

	#[test]
	fn has_keys_missing_babe_returns_false() {
		let (_inner, w) = mk_keystore();
		let stranger = sr25519::Pair::from_string("//Stranger", None).unwrap().public();

		assert!(!w.has_keys(&[(raw(&stranger), BABE)]));
	}

	/// Regression: a missing NON-BABE key must return `false`. An earlier version
	/// only returned `false` for missing BABE keys and reported `true` otherwise.
	#[test]
	fn has_keys_missing_non_babe_returns_false() {
		let (_inner, w) = mk_keystore();
		assert!(!w.has_keys(&[(vec![1u8; 32], GRANDPA)]));
	}

	#[test]
	fn has_keys_mixed_missing_non_babe_returns_false() {
		let (inner, w) = mk_keystore();
		let aura = gen_key(&inner, AURA, "//Aura-seed");

		// First entry is satisfied via the AURA fallback; the second is a
		// missing non-BABE key and must force the whole result to false.
		assert!(!w.has_keys(&[(raw(&aura), BABE), (vec![9u8; 32], GRANDPA)]));
	}

	// --- non-sr25519 passthrough -------------------------------------------

	#[test]
	fn ed25519_public_keys_passthrough() {
		let (inner, w) = mk_keystore();
		let grandpa = inner.ed25519_generate_new(GRANDPA, Some("//Grandpa")).unwrap();

		assert_eq!(w.ed25519_public_keys(GRANDPA), vec![grandpa]);
	}

	/// The AURA fallback is scoped to sr25519; a BABE request on the ed25519
	/// interface must not borrow the sr25519 AURA key.
	#[test]
	fn ed25519_babe_request_does_not_fall_back() {
		let (inner, w) = mk_keystore();
		gen_key(&inner, AURA, "//Aura-seed");

		assert!(w.ed25519_public_keys(BABE).is_empty());
	}
}
