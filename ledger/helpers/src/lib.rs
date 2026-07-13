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

mod utils;

pub use utils::find_dependency_version;
pub mod extract_tx_with_context;

/// Strategy for ordering candidate coins/UTXOs during input selection.
///
/// Defined at the crate root (not inside the version-specific `common` module) so that
/// `ledger_7` and `ledger_8` see the same type, allowing it to flow through the toolkit's
/// version-dispatched builders unchanged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CoinSelectionStrategy {
	/// Use the largest coins/UTXOs first. Minimizes the number of inputs.
	#[default]
	LargestFirst,
	/// Use the smallest coins/UTXOs first. Consolidates dust.
	SmallestFirst,
}

/// Struct to store serialized verifying key bytes
/// To be deserialized when constructing ContractOperations
pub struct ContractVerifyingKeyBytes(pub Vec<u8>);

#[path = "versions"]
pub mod ledger_7 {
	use crate::ContractVerifyingKeyBytes;

	pub use super::CoinSelectionStrategy;
	#[cfg(feature = "can-panic")]
	pub use super::extract_tx_with_context::extract_tx_with_context_ledger_7 as extract_tx_with_context;

	/// Ledger generation implemented by this module.
	pub const LEDGER_VERSION: u32 = 7;
	/// Workspace dependency name of the ledger crate backing this module.
	pub const CRATE_NAME: &str = "mn-ledger";
	pub use {
		base_crypto, coin_structure, ledger_storage, midnight_serialize, mn_ledger,
		onchain_runtime, transient_crypto, zkir, zswap,
	};

	// Vendored test-utilities shim for v8.
	#[allow(clippy::duplicate_mod)]
	#[path = "test_utilities_compat.rs"]
	pub mod test_utilities_local;

	#[path = "block_context/pre_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	// ECDSA is only supported from ledger 9. Pre-9 dependency versions can't represent an
	// ECDSA unshielded identity (coin-structure has no `From<ecdsa::VerifyingKey> for
	// UserAddress`, and the signature enums have no ECDSA variant), so the shared `common`
	// code compiles against these unimplemented stubs and fails loudly if ever exercised.
	#[allow(clippy::duplicate_mod)]
	mod ecdsa_unimpl;
	pub use ecdsa_unimpl::{SigningKeyEcdsa, VerifyingKeyEcdsa};

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;

	pub use base_crypto::signatures::{
		Signature as TransactionSignature, SigningKey as TransactionSigningKey,
		VerifyingKey as SignatureVerifyingKey,
	};
	use midnight_serialize::tagged_deserialize;

	/// Builds a contract operation from a verifier key. `_ir_source` is accepted
	/// for cross-version call-site compatibility but silently dropped: pre-ledger-9
	/// contract operations have no on-chain IR slot.
	pub fn contract_operation_new(
		vk: Option<ContractVerifyingKeyBytes>,
		_ir_source: Option<Vec<u8>>,
	) -> Result<onchain_runtime::state::ContractOperation, std::io::Error> {
		let vk = vk
			.map(|b| tagged_deserialize(&mut b.0.as_slice()).expect("failed to read verifier key"));
		Ok(onchain_runtime::state::ContractOperation::new(vk))
	}

	/// Wraps a verifier key in the maintenance-update enum for this ledger generation.
	/// Pre-ledger-9 ledgers expose only the `V3` (zk-stdlib v1) variant, which takes the
	/// same `transient_crypto::proofs::VerifierKey` this module deserializes.
	pub fn contract_operation_versioned_verifier_key(
		vk: Vec<u8>,
	) -> Result<mn_ledger::structure::ContractOperationVersionedVerifierKey, std::io::Error> {
		let vk: transient_crypto::proofs::VerifierKey = tagged_deserialize(&mut &vk[..])?;
		Ok(mn_ledger::structure::ContractOperationVersionedVerifierKey::V3(vk))
	}

	/// The verifier-key slot version for this ledger generation, used when *removing*
	/// a key (the entry point alone doesn't say which slot the key lives in).
	/// Pre-ledger-9 ledgers only have the `V3` slot, so removals target it. Mirrors
	/// `contract_operation_versioned_verifier_key` above.
	pub fn contract_operation_version_of(
		_op: &onchain_runtime::state::ContractOperation,
	) -> mn_ledger::structure::ContractOperationVersion {
		mn_ledger::structure::ContractOperationVersion::V3
	}

	pub fn signature_verifying_key(
		key: base_crypto::signatures::VerifyingKey,
	) -> SignatureVerifyingKey {
		key
	}

	pub fn transaction_signing_key(
		key: &base_crypto::signatures::SigningKey,
	) -> TransactionSigningKey {
		key.clone()
	}

	pub fn transaction_signature(
		signature: base_crypto::signatures::Signature,
	) -> TransactionSignature {
		signature
	}

	pub fn maintenance_verifying_key(
		key: base_crypto::signatures::VerifyingKey,
	) -> SignatureVerifyingKey {
		key
	}

	pub fn signature_verifying_key_ecdsa(_key: VerifyingKeyEcdsa) -> SignatureVerifyingKey {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn transaction_signing_key_ecdsa(_key: &SigningKeyEcdsa) -> TransactionSigningKey {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn transaction_signature_ecdsa(
		_signature: base_crypto::ecdsa::Signature,
	) -> TransactionSignature {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn maintenance_verifying_key_ecdsa(_key: VerifyingKeyEcdsa) -> SignatureVerifyingKey {
		unimplemented!("ecdsa is only supported from ledger 9")
	}
}

#[path = "versions"]
pub mod ledger_8 {
	use crate::ContractVerifyingKeyBytes;

	pub use super::CoinSelectionStrategy;
	#[cfg(feature = "can-panic")]
	pub use super::extract_tx_with_context::extract_tx_with_context_ledger_8 as extract_tx_with_context;

	/// Ledger generation implemented by this module.
	pub const LEDGER_VERSION: u32 = 8;
	/// Workspace dependency name of the ledger crate backing this module.
	pub const CRATE_NAME: &str = "mn-ledger-8";
	pub use {
		base_crypto, coin_structure, ledger_storage_ledger_8 as ledger_storage, midnight_serialize,
		mn_ledger_8 as mn_ledger, onchain_runtime_ledger_8 as onchain_runtime, transient_crypto,
		zkir, zswap_ledger_8 as zswap,
	};

	// Vendored test-utilities shim for v8.
	#[allow(clippy::duplicate_mod)]
	#[path = "test_utilities_compat.rs"]
	pub mod test_utilities_local;

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	// ECDSA is only supported from ledger 9 (see the note in `ledger_7`).
	#[allow(clippy::duplicate_mod)]
	mod ecdsa_unimpl;
	pub use ecdsa_unimpl::{SigningKeyEcdsa, VerifyingKeyEcdsa};

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;

	pub use base_crypto::signatures::{
		Signature as TransactionSignature, SigningKey as TransactionSigningKey,
		VerifyingKey as SignatureVerifyingKey,
	};
	use midnight_serialize::tagged_deserialize;

	/// Builds a contract operation from a verifier key. `_ir_source` is accepted
	/// for cross-version call-site compatibility but silently dropped: pre-ledger-9
	/// contract operations have no on-chain IR slot.
	pub fn contract_operation_new(
		vk: Option<ContractVerifyingKeyBytes>,
		_ir_source: Option<Vec<u8>>,
	) -> Result<onchain_runtime::state::ContractOperation, std::io::Error> {
		let vk = vk
			.map(|b| tagged_deserialize(&mut b.0.as_slice()).expect("failed to read verifier key"));
		Ok(onchain_runtime::state::ContractOperation::new(vk))
	}

	/// Wraps a verifier key in the maintenance-update enum for this ledger generation.
	/// Pre-ledger-9 ledgers expose only the `V3` (zk-stdlib v1) variant, which takes the
	/// same `transient_crypto::proofs::VerifierKey` this module deserializes.
	pub fn contract_operation_versioned_verifier_key(
		vk: Vec<u8>,
	) -> Result<mn_ledger::structure::ContractOperationVersionedVerifierKey, std::io::Error> {
		let vk: transient_crypto::proofs::VerifierKey = tagged_deserialize(&mut &vk[..])?;
		Ok(mn_ledger::structure::ContractOperationVersionedVerifierKey::V3(vk))
	}

	/// The verifier-key slot version for this ledger generation, used when *removing*
	/// a key (the entry point alone doesn't say which slot the key lives in).
	/// Pre-ledger-9 ledgers only have the `V3` slot, so removals target it. Mirrors
	/// `contract_operation_versioned_verifier_key` above.
	pub fn contract_operation_version_of(
		_op: &onchain_runtime::state::ContractOperation,
	) -> mn_ledger::structure::ContractOperationVersion {
		mn_ledger::structure::ContractOperationVersion::V3
	}

	pub fn signature_verifying_key(
		key: base_crypto::signatures::VerifyingKey,
	) -> SignatureVerifyingKey {
		key
	}

	pub fn transaction_signing_key(
		key: &base_crypto::signatures::SigningKey,
	) -> TransactionSigningKey {
		key.clone()
	}

	pub fn transaction_signature(
		signature: base_crypto::signatures::Signature,
	) -> TransactionSignature {
		signature
	}

	pub fn maintenance_verifying_key(
		key: base_crypto::signatures::VerifyingKey,
	) -> SignatureVerifyingKey {
		key
	}

	pub fn signature_verifying_key_ecdsa(_key: VerifyingKeyEcdsa) -> SignatureVerifyingKey {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn transaction_signing_key_ecdsa(_key: &SigningKeyEcdsa) -> TransactionSigningKey {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn transaction_signature_ecdsa(
		_signature: base_crypto::ecdsa::Signature,
	) -> TransactionSignature {
		unimplemented!("ecdsa is only supported from ledger 9")
	}

	pub fn maintenance_verifying_key_ecdsa(_key: VerifyingKeyEcdsa) -> SignatureVerifyingKey {
		unimplemented!("ecdsa is only supported from ledger 9")
	}
}

#[path = "versions"]
pub mod ledger_9 {
	use crate::ContractVerifyingKeyBytes;

	pub use super::CoinSelectionStrategy;
	#[cfg(feature = "can-panic")]
	pub use super::extract_tx_with_context::extract_tx_with_context_ledger_9 as extract_tx_with_context;

	/// Ledger generation implemented by this module.
	pub const LEDGER_VERSION: u32 = 9;
	/// Workspace dependency name of the ledger crate backing this module.
	pub const CRATE_NAME: &str = "mn-ledger-9";
	pub use {
		base_crypto, coin_structure_ledger_9 as coin_structure,
		ledger_storage_ledger_8 as ledger_storage, midnight_serialize, mn_ledger_9 as mn_ledger,
		onchain_runtime_ledger_9 as onchain_runtime, transient_crypto_ledger_9 as transient_crypto,
		zkir, zswap_ledger_9 as zswap,
	};

	use midnight_serialize::{peek_tag, tagged_deserialize};
	pub use mn_ledger::test_utilities as test_utilities_local;

	#[allow(clippy::duplicate_mod)]
	#[path = "block_context/post_ledger_8.rs"]
	mod block_context;
	pub use block_context::*;

	#[allow(clippy::duplicate_mod)]
	mod common;
	pub use common::*;

	pub use mn_ledger::structure::{
		Signature as TransactionSignature, SignatureVerifyingKey,
		SigningKey as TransactionSigningKey,
	};
	pub use onchain_runtime::state::ContractMaintenanceVerifyingKey;

	// ECDSA is natively supported from ledger 9: the signature enums carry an `ECDSA` variant
	// and coin-structure provides `From<ecdsa::VerifyingKey> for UserAddress`.
	pub use base_crypto::ecdsa::{
		SigningKey as SigningKeyEcdsa, VerifyingKey as VerifyingKeyEcdsa,
	};

	/// Builds a contract operation from a verifier key plus, from ledger 9 on,
	/// the circuit's zkir. `ir_source` is stored on-chain alongside the verifier
	/// key so the deployed contract's circuits can later be re-proven/upgraded
	/// from chain state alone; it counts toward `max_contract_metadata_size`.
	pub fn contract_operation_new(
		vk: Option<ContractVerifyingKeyBytes>,
		ir_source: Option<Vec<u8>>,
	) -> Result<onchain_runtime::state::ContractOperation, std::io::Error> {
		let ir = ir_source
			.map(|bytes| ledger_storage::arena::Sp::new(onchain_runtime::state::IrBuf(bytes)));
		let mut op = onchain_runtime::state::ContractOperation::new(None, ir);

		if let Some(vk) = vk {
			let tag = peek_tag(&mut std::io::Cursor::new(&vk.0))?;
			match tag.as_str() {
				"verifier-key[v6]" => op.v2 = Some(tagged_deserialize(&mut &vk.0[..])?),
				"verifier-key[v7]" => op.v3 = Some(tagged_deserialize(&mut &vk.0[..])?),
				_ => panic!("unknown verifier key tag: '{tag}'"),
			}
		}

		Ok(op)
	}

	/// Wraps a verifier key in the maintenance-update enum for this ledger generation.
	/// Ledger 9 accepts either a legacy 2.x (`v6`) key, stored in the `V3` slot via the
	/// crate-level (non-ledger-9-aliased) `transient_crypto` — the same 2.x
	/// `midnight-transient-crypto` build `op.v2` uses in `contract_operation_new` above —
	/// or a 3.x/zk-stdlib-v2 (`v7`) key, stored in the `V4` slot. The tag on the key file
	/// itself says which, mirroring the dispatch in `contract_operation_new`.
	pub fn contract_operation_versioned_verifier_key(
		vk: Vec<u8>,
	) -> Result<mn_ledger::structure::ContractOperationVersionedVerifierKey, std::io::Error> {
		let tag = peek_tag(&mut std::io::Cursor::new(&vk))?;
		match tag.as_str() {
			"verifier-key[v6]" => {
				let vk: ::transient_crypto::proofs::VerifierKey = tagged_deserialize(&mut &vk[..])?;
				Ok(mn_ledger::structure::ContractOperationVersionedVerifierKey::V3(vk))
			},
			"verifier-key[v7]" => {
				let vk: transient_crypto::proofs::VerifierKey = tagged_deserialize(&mut &vk[..])?;
				Ok(mn_ledger::structure::ContractOperationVersionedVerifierKey::V4(vk))
			},
			_ => panic!("unknown verifier key tag: '{tag}'"),
		}
	}

	/// The verifier-key slot version an *existing* contract operation's key actually lives
	/// in (the entry point alone doesn't say which slot). Ledger 9 keys can land in either
	/// `V3` (legacy 2.x/v6) or `V4` (3.x/v7, preferred if somehow both are set) depending on
	/// what compiled the circuit; removals must target whichever slot is populated, or they
	/// fail with `VerifierKeyNotFound`.
	pub fn contract_operation_version_of(
		op: &onchain_runtime::state::ContractOperation,
	) -> mn_ledger::structure::ContractOperationVersion {
		if op.v3.is_some() {
			mn_ledger::structure::ContractOperationVersion::V4
		} else {
			mn_ledger::structure::ContractOperationVersion::V3
		}
	}

	pub fn signature_verifying_key(
		key: base_crypto::signatures::VerifyingKey,
	) -> SignatureVerifyingKey {
		SignatureVerifyingKey::Schnorr(key)
	}

	pub fn transaction_signing_key(
		key: &base_crypto::signatures::SigningKey,
	) -> TransactionSigningKey {
		TransactionSigningKey::Schnorr(key.clone())
	}

	pub fn transaction_signature(
		signature: base_crypto::signatures::Signature,
	) -> TransactionSignature {
		TransactionSignature::Schnorr(signature)
	}

	pub fn maintenance_verifying_key(
		key: base_crypto::signatures::VerifyingKey,
	) -> ContractMaintenanceVerifyingKey {
		ContractMaintenanceVerifyingKey::Schnorr(key)
	}

	pub fn signature_verifying_key_ecdsa(
		key: base_crypto::ecdsa::VerifyingKey,
	) -> SignatureVerifyingKey {
		SignatureVerifyingKey::ECDSA(key)
	}

	pub fn transaction_signing_key_ecdsa(
		key: &base_crypto::ecdsa::SigningKey,
	) -> TransactionSigningKey {
		TransactionSigningKey::ECDSA(key.clone())
	}

	pub fn transaction_signature_ecdsa(
		signature: base_crypto::ecdsa::Signature,
	) -> TransactionSignature {
		TransactionSignature::ECDSA(signature)
	}

	pub fn maintenance_verifying_key_ecdsa(
		key: base_crypto::ecdsa::VerifyingKey,
	) -> ContractMaintenanceVerifyingKey {
		ContractMaintenanceVerifyingKey::ECDSA(key)
	}
}

pub use ledger_9 as latest;

pub mod fork;

pub use latest::*;
