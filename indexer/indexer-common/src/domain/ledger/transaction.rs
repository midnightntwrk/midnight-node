// This file is part of midnight-indexer.
// Copyright (C) 2025 Midnight Foundation
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

use crate::domain::{
    ByteArray, ByteVec, PROTOCOL_VERSION_000_016_000, ProtocolVersion, ViewingKey,
    ledger::{Error, SerializableV6Ext, TaggedSerializableV6Ext, TransactionV6},
};
use fastrace::trace;
use futures::{StreamExt, TryStreamExt};
use midnight_coin_structure_v6::{
    coin::Info as InfoV6, contract::ContractAddress as ContractAddressV6,
};
use midnight_ledger_v6::structure::{
    ContractAction as ContractActionV6, StandardTransaction as StandardTransactionV6,
    SystemTransaction as LedgerSystemTransactionV6,
};
use midnight_serialize_v6::tagged_deserialize as tagged_deserialize_v6;
use midnight_storage_v6::DefaultDB as DefaultDBV6;
use midnight_transient_crypto_v6::{
    encryption::SecretKey as SecretKeyV6, proofs::Proof as ProofV6,
};
use midnight_zswap_v6::Offer as OfferV6;
use serde::Serialize;
use std::error::Error as StdError;

pub type SerializedContractAddress = ByteVec;
pub type SerializedContractEntryPoint = ByteVec;
pub type SerializedContractState = ByteVec;
pub type SerializedTransactionIdentifier = ByteVec;
pub type TransactionHash = ByteArray<32>;

/// Facade for `Transaction` from `midnight_ledger` across supported (protocol) versions.
#[derive(Debug, Clone)]
pub enum Transaction {
    V6(TransactionV6),
}

impl Transaction {
    /// Deserialize the given serialized transaction using the given protocol version.
    #[trace(properties = { "protocol_version": "{protocol_version}" })]
    pub fn deserialize(
        transaction: impl AsRef<[u8]>,
        protocol_version: ProtocolVersion,
    ) -> Result<Self, Error> {
        if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
            let transaction = tagged_deserialize_v6(&mut transaction.as_ref())
                .map_err(|error| Error::Io("cannot deserialize LedgerTransactionV6", error))?;
            Ok(Self::V6(transaction))
        } else {
            Err(Error::InvalidProtocolVersion(protocol_version))
        }
    }

    /// Get the hash.
    pub fn hash(&self) -> TransactionHash {
        match self {
            Self::V6(transaction) => transaction.transaction_hash().0.0.into(),
        }
    }

    /// Get the identifiers.
    pub fn identifiers(&self) -> Result<Vec<SerializedTransactionIdentifier>, Error> {
        match self {
            Self::V6(transaction) => transaction
                .identifiers()
                .map(|identifier| {
                    let identifier = identifier.tagged_serialize_v6().map_err(|error| {
                        Error::Io("cannot serialize TransactionIdentifierV6", error)
                    })?;
                    Ok(identifier)
                })
                .collect(),
        }
    }

    /// Get the contract actions; this involves node calls.
    #[trace]
    pub async fn contract_actions<E, F>(
        &self,
        get_contract_state: impl Fn(SerializedContractAddress) -> F,
    ) -> Result<Vec<ContractAction>, Error>
    where
        E: StdError + 'static + Send + Sync,
        F: Future<Output = Result<SerializedContractState, E>>,
    {
        match self {
            Self::V6(transaction) => match transaction {
                TransactionV6::Standard(standard_transaction) => {
                    let contract_actions = futures::stream::iter(standard_transaction.actions())
                        .then(|(_, contract_action)| async {
                            match contract_action {
                                ContractActionV6::Deploy(deploy) => {
                                    let address = serialize_contract_address(deploy.address())?;
                                    let state = get_contract_state(address.clone())
                                        .await
                                        .map_err(|error| Error::GetContractState(error.into()))?;

                                    Ok::<_, Error>(ContractAction {
                                        address,
                                        state,
                                        attributes: ContractAttributes::Deploy,
                                    })
                                }

                                ContractActionV6::Call(call) => {
                                    let address = serialize_contract_address(call.address)?;
                                    let state = get_contract_state(address.clone())
                                        .await
                                        .map_err(|error| Error::GetContractState(error.into()))?;
                                    let entry_point =
                                        call.entry_point.serialize_v6().map_err(|error| {
                                            Error::Io("cannot serialize EntryPointBufV6", error)
                                        })?;

                                    Ok(ContractAction {
                                        address,
                                        state,
                                        attributes: ContractAttributes::Call { entry_point },
                                    })
                                }

                                ContractActionV6::Maintain(update) => {
                                    let address = serialize_contract_address(update.address)?;
                                    let state = get_contract_state(address.clone())
                                        .await
                                        .map_err(|error| Error::GetContractState(error.into()))?;

                                    Ok(ContractAction {
                                        address,
                                        state,
                                        attributes: ContractAttributes::Update,
                                    })
                                }
                            }
                        })
                        .try_collect::<Vec<_>>()
                        .await?;

                    Ok(contract_actions)
                }

                TransactionV6::ClaimRewards(_) => Ok(vec![]),
            },
        }
    }

    /// Get the structure of this transaction for fees calculation.
    pub fn structure(&self, size: usize) -> TransactionStructure {
        match self {
            Self::V6(transaction) => match transaction {
                TransactionV6::Standard(standard_transaction) => {
                    let contract_action_count = standard_transaction.actions().count();
                    let identifier_count = transaction.identifiers().count();

                    let segment_count = if contract_action_count > 1 || identifier_count > 2 {
                        2
                    } else {
                        1
                    };
                    let estimated_input_count = identifier_count.max(1);
                    let estimated_output_count = (identifier_count + 1).max(1);
                    let has_contract_operations = contract_action_count > 0;

                    TransactionStructure {
                        segment_count,
                        estimated_input_count,
                        estimated_output_count,
                        has_contract_operations,
                        size,
                    }
                }

                TransactionV6::ClaimRewards(_) => TransactionStructure {
                    segment_count: 1,
                    estimated_input_count: 1,
                    estimated_output_count: 1,
                    has_contract_operations: false,
                    size,
                },
            },
        }
    }

    // Check if this transaction belongs to the given viewing key.
    pub fn relevant(&self, viewing_key: ViewingKey) -> bool {
        match self {
            Self::V6(transaction) => match transaction {
                TransactionV6::Standard(StandardTransactionV6 {
                    guaranteed_coins,
                    fallible_coins,
                    ..
                }) => {
                    let secret_key = SecretKeyV6::from_repr(&viewing_key.expose_secret().0)
                        .expect("SecretKeyV6 can be created from repr");

                    let can_decrypt_guaranteed_coins = guaranteed_coins
                        .as_ref()
                        .map(|guaranteed_coins| can_decrypt_v6(&secret_key, guaranteed_coins))
                        .unwrap_or_default();

                    let can_decrypt_fallible_coins = || {
                        fallible_coins
                            .values()
                            .any(|fallible_coins| can_decrypt_v6(&secret_key, &fallible_coins))
                    };

                    can_decrypt_guaranteed_coins || can_decrypt_fallible_coins()
                }

                TransactionV6::ClaimRewards(_) => false,
            },
        }
    }
}

/// A contract action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContractAction {
    pub address: SerializedContractAddress,
    pub state: SerializedContractState,
    pub attributes: ContractAttributes,
}

/// Attributes for a specific contract action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ContractAttributes {
    Deploy,
    Call {
        entry_point: SerializedContractEntryPoint,
    },
    Update,
}

/// Transaction structure for fees calculation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransactionStructure {
    pub segment_count: usize,
    pub estimated_input_count: usize,
    pub estimated_output_count: usize,
    pub has_contract_operations: bool,
    pub size: usize,
}

/// Facade for `SystemTransaction` from `midnight_ledger` across supported (protocol) versions.
#[derive(Debug, Clone)]
pub enum SystemTransaction {
    V6(LedgerSystemTransactionV6),
}

impl SystemTransaction {
    /// Deserialize the given serialized transaction using the given protocol version.
    #[trace(properties = { "protocol_version": "{protocol_version}" })]
    pub fn deserialize(
        transaction: impl AsRef<[u8]>,
        protocol_version: ProtocolVersion,
    ) -> Result<Self, Error> {
        if protocol_version.is_compatible(PROTOCOL_VERSION_000_016_000) {
            let transaction =
                tagged_deserialize_v6(&mut transaction.as_ref()).map_err(|error| {
                    Error::Io("cannot deserialize LedgerSystemTransactionV6", error)
                })?;
            Ok(Self::V6(transaction))
        } else {
            Err(Error::InvalidProtocolVersion(protocol_version))
        }
    }

    /// Get the hash.
    pub fn hash(&self) -> TransactionHash {
        match self {
            Self::V6(transaction) => transaction.transaction_hash().0.0.into(),
        }
    }
}

fn serialize_contract_address(
    address: ContractAddressV6,
) -> Result<SerializedContractAddress, Error> {
    address
        .tagged_serialize_v6()
        .map_err(|error| Error::Io("cannot serialize ContractAddressV6", error))
}

fn can_decrypt_v6(key: &SecretKeyV6, offer: &OfferV6<ProofV6, DefaultDBV6>) -> bool {
    let outputs = offer.outputs.iter().filter_map(|o| o.ciphertext.clone());
    let transient = offer.transient.iter().filter_map(|o| o.ciphertext.clone());
    let mut ciphertexts = outputs.chain(transient);

    ciphertexts.any(|ciphertext| {
        key.decrypt::<InfoV6>(&(*ciphertext).to_owned().into())
            .is_some()
    })
}

#[cfg(test)]
mod tests {
    use crate::domain::{PROTOCOL_VERSION_000_016_000, ViewingKey, ledger::Transaction};
    use bip32::{DerivationPath, XPrv};
    use midnight_zswap_v6::keys::{SecretKeys, Seed};
    use std::{fs, str::FromStr};

    /// Notice: The raw test data is created with `generate_txs.sh`.
    #[test]
    #[ignore = "temporarily disabled until we can create shielded transactions on the node again"]
    fn test_deserialize_relevant() {
        let transaction = fs::read(format!("{}/tests/tx_1_2_2.raw", env!("CARGO_MANIFEST_DIR")))
            .expect("transaction file can be read");
        let transaction = Transaction::deserialize(transaction, PROTOCOL_VERSION_000_016_000)
            .expect("transaction can be deserialized");

        assert!(transaction.relevant(viewing_key(1)));
        assert!(transaction.relevant(viewing_key(2)));
        assert!(!transaction.relevant(viewing_key(3)));

        let transaction = fs::read(format!("{}/tests/tx_1_2_3.raw", env!("CARGO_MANIFEST_DIR")))
            .expect("transaction file can be read");
        let transaction = Transaction::deserialize(transaction, PROTOCOL_VERSION_000_016_000)
            .expect("transaction can be deserialized");

        assert!(transaction.relevant(viewing_key(1)));
        assert!(transaction.relevant(viewing_key(2)));
        assert!(!transaction.relevant(viewing_key(3)));
    }

    fn viewing_key(n: u8) -> ViewingKey {
        let mut seed = [0; 32];
        seed[31] = n;

        let derivation_path =
            DerivationPath::from_str("m/44'/2400'/0'/3/0").expect("derivation path can be created");
        let derived_seed: [u8; 32] = XPrv::derive_from_path(seed, &derivation_path)
            .expect("key can be derived")
            .private_key()
            .to_bytes()
            .into();

        SecretKeys::from(Seed::from(derived_seed))
            .encryption_secret_key
            .repr()
            .into()
    }
}
