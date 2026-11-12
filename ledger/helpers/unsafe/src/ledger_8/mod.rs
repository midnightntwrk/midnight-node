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

//! Ledger generation 8 toolkit apparatus: context/intent/offer/UTXO builders and
//! wallet key management, layered on top of the shared, version-independent
//! surface re-exported from `midnight-node-ledger-helpers::ledger_8`.
//!
//! Its `ledger_9/` counterpart is a separate copy on purpose: `diff -r ledger_8
//! ledger_9` shows exactly where the two generations diverge, and an edit here
//! cannot leak into v9.

pub use midnight_node_ledger_helpers::ledger_8::*;

pub use crate::extract_tx_with_context::extract_tx_with_context_ledger_8 as extract_tx_with_context;

pub use crate::CoinSelectionStrategy;
use crate::ContractVerifyingKeyBytes;

use midnight_serialize::tagged_deserialize;

// ECDSA is only supported from ledger 9; this module is stubs that panic.
mod ecdsa;
pub use ecdsa::{SigningKeyEcdsa, VerifyingKeyEcdsa};

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

/// Builds a contract operation from a verifier key. `_ir_source` is accepted
/// for cross-version call-site compatibility but silently dropped: pre-ledger-9
/// contract operations have no on-chain IR slot.
pub fn contract_operation_new(
	vk: Option<ContractVerifyingKeyBytes>,
	_ir_source: Option<Vec<u8>>,
) -> Result<onchain_runtime::state::ContractOperation, std::io::Error> {
	let vk =
		vk.map(|b| tagged_deserialize(&mut b.0.as_slice()).expect("failed to read verifier key"));
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

pub fn transaction_signing_key(key: &base_crypto::signatures::SigningKey) -> TransactionSigningKey {
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
