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

//! Ledger generation 9 toolkit apparatus: context/intent/offer/UTXO builders and
//! wallet key management, layered on top of the shared, version-independent
//! surface re-exported from `midnight-node-ledger-helpers::ledger_9`.
//!
//! Its `ledger_8/` counterpart is a separate copy on purpose: `diff -r ledger_8
//! ledger_9` shows exactly where the two generations diverge, and an edit here
//! cannot leak into v8.

pub use midnight_node_ledger_helpers::ledger_9::*;

pub use crate::extract_tx_with_context::extract_tx_with_context_ledger_9 as extract_tx_with_context;

pub use crate::CoinSelectionStrategy;
use crate::ContractVerifyingKeyBytes;

use midnight_serialize::{peek_tag, tagged_deserialize};

// ECDSA is natively supported from ledger 9.
mod ecdsa;
pub use ecdsa::{SigningKeyEcdsa, VerifyingKeyEcdsa};

// Ledger-9-only ECDSA wallet tests (no v8 counterpart); see the module docs.
#[cfg(test)]
mod ecdsa_wallet_tests;

pub mod block_data;
pub mod context;
pub mod contract;
mod input;
mod intent;
mod offer;
mod output;
pub mod transaction;
mod transient;
mod unshielded_offer;
mod utxo_output;
mod utxo_spend;
pub mod wallet;

mod proving;

pub use {
	context::*, contract::*, input::*, intent::*, offer::*, output::*, proving::*, transaction::*,
	transient::*, unshielded_offer::*, utxo_output::*, utxo_spend::*, wallet::*,
};

/// Builds a contract operation from a verifier key plus, from ledger 9 on,
/// the circuit's zkir. `ir_source` is stored on-chain alongside the verifier
/// key so the deployed contract's circuits can later be re-proven/upgraded
/// from chain state alone; it counts toward `max_contract_metadata_size`.
pub fn contract_operation_new(
	vk: Option<ContractVerifyingKeyBytes>,
	ir_source: Option<Vec<u8>>,
) -> Result<onchain_runtime::state::ContractOperation, std::io::Error> {
	let ir =
		ir_source.map(|bytes| ledger_storage::arena::Sp::new(onchain_runtime::state::IrBuf(bytes)));
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

pub fn transaction_signing_key(key: &base_crypto::signatures::SigningKey) -> TransactionSigningKey {
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

/// Compatibility trait: L8 `apply` returns `WalletState<D>`, L9 returns `Result<WalletState<D>, _>`.
pub trait IntoWalletState<D: DB + Clone> {
	fn into_wallet_state(self) -> WalletState<D>;
}
impl<D: DB + Clone> IntoWalletState<D> for WalletState<D> {
	fn into_wallet_state(self) -> WalletState<D> {
		self
	}
}
impl<D: DB + Clone, E: std::fmt::Debug> IntoWalletState<D> for Result<WalletState<D>, E> {
	fn into_wallet_state(self) -> WalletState<D> {
		self.expect("wallet state apply failed")
	}
}

/// Raw zkir bytes for circuit `name` (the `zkir/{name}.bzkir` the resolver
/// loads as `ProvingKeyMaterial::ir_source`). Ledger 9+ stores these on-chain
/// in the contract operation so deployed circuits can be re-proven/upgraded
/// from chain state alone; pre-9 `contract_operation_new` ignores them.
pub async fn ir_source(resolver: &Resolver, name: &'static str) -> Option<Vec<u8>> {
	let material = resolver
		.resolve_key(KeyLocation(std::borrow::Cow::Borrowed(name)))
		.await
		.ok()??;
	Some(material.ir_source)
}

/// Resolves a circuit's verifier key by name.
pub async fn verifier_key(
	resolver: &Resolver,
	name: &'static str,
) -> Option<ContractVerifyingKeyBytes> {
	let material = resolver
		.resolve_key(KeyLocation(std::borrow::Cow::Borrowed(name)))
		.await
		.ok()??;
	Some(ContractVerifyingKeyBytes(material.verifier_key))
}

pub fn token_type_decode(input: &str) -> TokenType {
	let bytes = hex::decode(input).expect("Token value should be an hex");

	let tt_bytes: [u8; 32] = bytes.try_into().expect("Token size should be 32 bytes");

	TokenType::Shielded(ShieldedTokenType(HashOutput(tt_bytes)))
}

/// Get NetworkId from transaction bytes
pub fn network_id_from_transaction_bytes(tx_bytes: &[u8]) -> Result<String, std::io::Error> {
	let tx: FinalizedTransaction<DefaultDB> = deserialize(tx_bytes)?;
	let network_id = match tx {
		Transaction::Standard(standard_transaction) => standard_transaction.network_id,
		Transaction::ClaimRewards(claim_rewards_transaction) => {
			claim_rewards_transaction.network_id
		},
	};
	Ok(network_id)
}
