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

//! Unimplemented ECDSA key stubs for pre-ledger-9 generations.
//!
//! Ledger 7 and 8 have no native ECDSA unshielded identity: their `coin-structure` provides no
//! `From<ecdsa::VerifyingKey> for UserAddress` conversion and their signature types carry no
//! ECDSA variant. The shared `common` wallet code is generic over the signature scheme and must
//! still compile against these generations, so it references these drop-in stub types in place
//! of the real `base_crypto::ecdsa` keys.
//!
//! Every operation panics. The wallet's ECDSA variant can only be built via
//! [`SigningKeyEcdsa::from_bytes`], which panics first, so the (de)serialization bodies are never
//! reached. The toolkit additionally guards against selecting ECDSA on pre-9 ledgers, giving a
//! clear error rather than a panic deep in these stubs.

use super::base_crypto::ecdsa::Signature;
use super::coin_structure::coin::UserAddress;
use super::midnight_serialize::{Deserializable, Serializable, Tagged};
use std::borrow::Cow;
use std::io;

#[derive(Clone, Debug)]
pub struct VerifyingKeyEcdsa;

#[derive(Clone, Debug)]
pub struct SigningKeyEcdsa;

impl SigningKeyEcdsa {
	pub fn from_bytes(_bytes: &[u8]) -> Result<Self, Box<dyn std::error::Error>> {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn verifying_key(&self) -> VerifyingKeyEcdsa {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn sign(&self, _msg: &[u8]) -> Signature {
		unimplemented!("ecdsa is only supported from ledger 9")
	}
}

impl VerifyingKeyEcdsa {
	pub fn verify(&self, _msg: &[u8], _signature: &Signature) -> bool {
		unimplemented!("ecdsa is only supported from ledger 9")
	}
}

impl From<VerifyingKeyEcdsa> for UserAddress {
	fn from(_value: VerifyingKeyEcdsa) -> Self {
		unimplemented!("ecdsa is only supported from ledger 9")
	}
}

// Trait impls so the `UnshieldedWalletKeys` derives (`Serializable`/`Deserializable`/`Tagged`)
// hold on ledger 7/8. `tag`/`tag_unique_factor` return real values because they may be consulted
// while computing the enclosing wallet's tag; the (de)serialization bodies are unreachable (see
// the module docs).
macro_rules! unimpl_serialize {
	($ty:ty, $tag:literal) => {
		impl Serializable for $ty {
			fn serialize(&self, _writer: &mut impl io::Write) -> io::Result<()> {
				unimplemented!("ecdsa is only supported from ledger 9")
			}
			fn serialized_size(&self) -> usize {
				unimplemented!("ecdsa is only supported from ledger 9")
			}
		}
		impl Deserializable for $ty {
			fn deserialize(_reader: &mut impl io::Read, _recursion_depth: u32) -> io::Result<Self> {
				unimplemented!("ecdsa is only supported from ledger 9")
			}
		}
		impl Tagged for $ty {
			fn tag() -> Cow<'static, str> {
				Cow::Borrowed($tag)
			}
			fn tag_unique_factor() -> String {
				$tag.into()
			}
		}
	};
}

unimpl_serialize!(VerifyingKeyEcdsa, "ecdsa-verifying-key-unimpl[v1]");
unimpl_serialize!(SigningKeyEcdsa, "ecdsa-signing-key-unimpl[v1]");
