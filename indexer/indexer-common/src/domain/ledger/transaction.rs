// This file is part of midnight-indexer.
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

use crate::{
    domain::{
        ContractAction, ContractAttributes, LedgerVersion, SerializedContractAddress,
        SerializedContractState, SerializedTransactionIdentifier, TransactionHash, ViewingKey,
        ledger::{Error, SerializableExt, TransactionV8, TransactionV9},
    },
    infra::ledger_db::v1_1,
};
use fastrace::trace;
use futures::{StreamExt, TryStreamExt};
use midnight_coin_structure_v2::{coin::Info, contract::ContractAddress};
use midnight_coin_structure_v3::{
    coin::Info as InfoV9, contract::ContractAddress as ContractAddressV9,
};
use midnight_ledger_v8::structure::{
    ContractAction as ContractActionV8, StandardTransaction as StandardTransactionV8,
    SystemTransaction as LedgerSystemTransactionV8,
};
use midnight_ledger_v9::structure::{
    ContractAction as ContractActionV9, StandardTransaction as StandardTransactionV9,
    SystemTransaction as LedgerSystemTransactionV9,
};
use midnight_serialize_v1::tagged_deserialize;
use midnight_storage_core_v1::db::DB;
use midnight_transient_crypto_v2::{encryption::SecretKey, proofs::Proof};
use midnight_transient_crypto_v3::{
    encryption::SecretKey as SecretKeyV9, proofs::Proof as ProofV9,
};
use midnight_zswap_v8::Offer as OfferV8;
use midnight_zswap_v9::Offer as OfferV9;
use std::error::Error as StdError;

#[derive(Debug, Clone)]
pub enum Transaction {
    V8(TransactionV8<v1_1::LedgerDb>),
    V9(TransactionV9<v1_1::LedgerDb>),
}

impl Transaction {
    #[trace(properties = { "ledger_version": "{ledger_version}" })]
    pub fn deserialize(
        transaction: impl AsRef<[u8]>,
        ledger_version: LedgerVersion,
    ) -> Result<Self, Error> {
        let transaction = match ledger_version {
            LedgerVersion::V8 => {
                let transaction = tagged_deserialize(&mut transaction.as_ref())
                    .map_err(|error| Error::Deserialize("LedgerTransactionV8", error))?;
                Self::V8(transaction)
            }
            LedgerVersion::V9 => {
                let transaction = tagged_deserialize(&mut transaction.as_ref())
                    .map_err(|error| Error::Deserialize("LedgerTransactionV9", error))?;
                Self::V9(transaction)
            }
        };

        Ok(transaction)
    }

    /// Get the hash.
    pub fn hash(&self) -> TransactionHash {
        match self {
            Self::V8(transaction) => transaction.transaction_hash().0.0.into(),
            Self::V9(transaction) => transaction.transaction_hash().0.0.into(),
        }
    }

    /// Get the identifiers.
    pub fn identifiers(&self) -> Result<Vec<SerializedTransactionIdentifier>, Error> {
        match self {
            Self::V8(transaction) => transaction
                .identifiers()
                .map(|identifier| {
                    let identifier = identifier
                        .serialize()
                        .map_err(|error| Error::Serialize("TransactionIdentifierV8", error))?;
                    Ok(identifier)
                })
                .collect(),
            Self::V9(transaction) => transaction
                .identifiers()
                .map(|identifier| {
                    let identifier = identifier
                        .serialize()
                        .map_err(|error| Error::Serialize("TransactionIdentifierV9", error))?;
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
            Self::V8(transaction) => match transaction {
                TransactionV8::Standard(standard_transaction) => {
                    let contract_actions = futures::stream::iter(standard_transaction.actions())
                        .then(|(_, contract_action)| async {
                            match contract_action {
                                ContractActionV8::Deploy(deploy) => {
                                    let address = serialize_contract_address(deploy.address())?;
                                    let state = get_contract_state(address.clone()).await.map_err(
                                        |error| {
                                            Error::GetContractState(address.clone(), error.into())
                                        },
                                    )?;

                                    Ok::<_, Error>(ContractAction {
                                        address,
                                        state,
                                        attributes: ContractAttributes::Deploy,
                                    })
                                }

                                ContractActionV8::Call(call) => {
                                    let address = serialize_contract_address(call.address)?;
                                    let state = get_contract_state(address.clone()).await.map_err(
                                        |error| {
                                            Error::GetContractState(address.clone(), error.into())
                                        },
                                    )?;
                                    let entry_point =
                                        String::from_utf8(call.entry_point.as_ref().to_owned())
                                            .map_err(|error| {
                                                Error::FromUtf8("EntryPointBufV8", error)
                                            })?;

                                    Ok(ContractAction {
                                        address,
                                        state,
                                        attributes: ContractAttributes::Call { entry_point },
                                    })
                                }

                                ContractActionV8::Maintain(update) => {
                                    let address = serialize_contract_address(update.address)?;
                                    let state = get_contract_state(address.clone()).await.map_err(
                                        |error| {
                                            Error::GetContractState(address.clone(), error.into())
                                        },
                                    )?;

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

                TransactionV8::ClaimRewards(_) => Ok(vec![]),
            },

            Self::V9(transaction) => match transaction {
                TransactionV9::Standard(standard_transaction) => {
                    let contract_actions = futures::stream::iter(standard_transaction.actions())
                        .then(|(_, contract_action)| async {
                            match contract_action {
                                ContractActionV9::Deploy(deploy) => {
                                    let address = serialize_contract_address_v9(deploy.address())?;
                                    let state = get_contract_state(address.clone()).await.map_err(
                                        |error| {
                                            Error::GetContractState(address.clone(), error.into())
                                        },
                                    )?;

                                    Ok::<_, Error>(ContractAction {
                                        address,
                                        state,
                                        attributes: ContractAttributes::Deploy,
                                    })
                                }

                                ContractActionV9::Call(call) => {
                                    let address = serialize_contract_address_v9(call.address)?;
                                    let state = get_contract_state(address.clone()).await.map_err(
                                        |error| {
                                            Error::GetContractState(address.clone(), error.into())
                                        },
                                    )?;
                                    let entry_point =
                                        String::from_utf8(call.entry_point.as_ref().to_owned())
                                            .map_err(|error| {
                                                Error::FromUtf8("EntryPointBufV9", error)
                                            })?;

                                    Ok(ContractAction {
                                        address,
                                        state,
                                        attributes: ContractAttributes::Call { entry_point },
                                    })
                                }

                                ContractActionV9::Maintain(update) => {
                                    let address = serialize_contract_address_v9(update.address)?;
                                    let state = get_contract_state(address.clone()).await.map_err(
                                        |error| {
                                            Error::GetContractState(address.clone(), error.into())
                                        },
                                    )?;

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

                TransactionV9::ClaimRewards(_) => Ok(vec![]),
            },
        }
    }

    // Check if this transaction belongs to the given viewing key.
    pub fn relevant(&self, viewing_key: ViewingKey) -> bool {
        match self {
            Self::V8(transaction) => match transaction {
                TransactionV8::Standard(StandardTransactionV8 {
                    guaranteed_coins,
                    fallible_coins,
                    ..
                }) => {
                    let secret_key = SecretKey::from_repr(&viewing_key.expose_secret().0)
                        .expect("SecretKey can be created from repr");

                    let can_decrypt_guaranteed_coins = guaranteed_coins
                        .as_ref()
                        .map(|guaranteed_coins| can_decrypt_v8(&secret_key, guaranteed_coins))
                        .unwrap_or_default();

                    let can_decrypt_fallible_coins = || {
                        fallible_coins
                            .values()
                            .any(|fallible_coins| can_decrypt_v8(&secret_key, &fallible_coins))
                    };

                    can_decrypt_guaranteed_coins || can_decrypt_fallible_coins()
                }

                TransactionV8::ClaimRewards(_) => false,
            },

            Self::V9(transaction) => match transaction {
                TransactionV9::Standard(StandardTransactionV9 {
                    guaranteed_coins,
                    fallible_coins,
                    ..
                }) => {
                    let secret_key = SecretKeyV9::from_repr(&viewing_key.expose_secret().0)
                        .expect("SecretKey can be created from repr");

                    let can_decrypt_guaranteed_coins = guaranteed_coins
                        .as_ref()
                        .map(|guaranteed_coins| can_decrypt_v9(&secret_key, guaranteed_coins))
                        .unwrap_or_default();

                    let can_decrypt_fallible_coins = || {
                        fallible_coins
                            .values()
                            .any(|fallible_coins| can_decrypt_v9(&secret_key, &fallible_coins))
                    };

                    can_decrypt_guaranteed_coins || can_decrypt_fallible_coins()
                }

                TransactionV9::ClaimRewards(_) => false,
            },
        }
    }
}

/// Facade for `SystemTransaction` from `midnight_ledger` across supported (protocol) versions.
#[derive(Debug, Clone)]
pub enum SystemTransaction {
    V8(LedgerSystemTransactionV8),
    V9(LedgerSystemTransactionV9),
}

impl SystemTransaction {
    #[trace(properties = { "ledger_version": "{ledger_version}" })]
    pub fn deserialize(
        transaction: impl AsRef<[u8]>,
        ledger_version: LedgerVersion,
    ) -> Result<Self, Error> {
        let transaction = match ledger_version {
            LedgerVersion::V8 => {
                let transaction = tagged_deserialize(&mut transaction.as_ref())
                    .map_err(|error| Error::Deserialize("LedgerSystemTransactionV8", error))?;
                Self::V8(transaction)
            }
            LedgerVersion::V9 => {
                let transaction = tagged_deserialize(&mut transaction.as_ref())
                    .map_err(|error| Error::Deserialize("LedgerSystemTransactionV9", error))?;
                Self::V9(transaction)
            }
        };

        Ok(transaction)
    }

    /// Get the hash.
    pub fn hash(&self) -> TransactionHash {
        match self {
            Self::V8(transaction) => transaction.transaction_hash().0.0.into(),
            Self::V9(transaction) => transaction.transaction_hash().0.0.into(),
        }
    }
}

fn serialize_contract_address(
    address: ContractAddress,
) -> Result<SerializedContractAddress, Error> {
    address
        .serialize()
        .map_err(|error| Error::Serialize("ContractAddress", error))
}

fn serialize_contract_address_v9(
    address: ContractAddressV9,
) -> Result<SerializedContractAddress, Error> {
    address
        .serialize()
        .map_err(|error| Error::Serialize("ContractAddressV9", error))
}

fn can_decrypt_v8<D: DB>(key: &SecretKey, offer: &OfferV8<Proof, D>) -> bool {
    let outputs = offer.outputs.iter().filter_map(|o| o.ciphertext.clone());
    let transient = offer.transient.iter().filter_map(|o| o.ciphertext.clone());
    let mut ciphertexts = outputs.chain(transient);

    ciphertexts.any(|ciphertext| {
        key.decrypt::<Info>(&(*ciphertext).to_owned().into())
            .is_some()
    })
}

fn can_decrypt_v9<D: DB>(key: &SecretKeyV9, offer: &OfferV9<ProofV9, D>) -> bool {
    let outputs = offer.outputs.iter().filter_map(|o| o.ciphertext.clone());
    let transient = offer.transient.iter().filter_map(|o| o.ciphertext.clone());
    let mut ciphertexts = outputs.chain(transient);

    ciphertexts.any(|ciphertext| {
        key.decrypt::<InfoV9>(&(*ciphertext).to_owned().into())
            .is_some()
    })
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{LedgerVersion, ViewingKey, ledger::Transaction},
        error::BoxError,
    };
    use anyhow::Context;
    use bip32::{DerivationPath, XPrv};
    use midnight_zswap_v8::keys::{SecretKeys, Seed};
    use std::{fs, str::FromStr};

    /// Notice: The raw test data is created with `generate_txs.sh`.
    #[cfg(any(feature = "cloud", feature = "standalone"))]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_deserialize_relevant() -> Result<(), BoxError> {
        #[cfg(feature = "cloud")]
        let _postgres_container = {
            use crate::infra::{ledger_db, migrations, pool::postgres::PostgresPool};
            use sqlx::postgres::PgSslMode;
            use std::time::Duration;
            use testcontainers::{ImageExt, runners::AsyncRunner};
            use testcontainers_modules::postgres::Postgres;

            let postgres_container = Postgres::default()
                .with_db_name("indexer")
                .with_user("indexer")
                .with_password(env!("APP__INFRA__STORAGE__PASSWORD"))
                .with_tag("17.1-alpine")
                .start()
                .await
                .context("start Postgres container")?;
            let postgres_port = postgres_container
                .get_host_port_ipv4(5432)
                .await
                .context("get Postgres port")?;

            let config = crate::infra::pool::postgres::Config {
                host: "localhost".to_string(),
                port: postgres_port,
                dbname: "indexer".to_string(),
                user: "indexer".to_string(),
                password: env!("APP__INFRA__STORAGE__PASSWORD").into(),
                sslmode: PgSslMode::Prefer,
                max_connections: 10,
                idle_timeout: Duration::from_secs(60),
                max_lifetime: Duration::from_secs(5 * 60),
            };

            let pool = PostgresPool::new(config).await.context("create pool")?;
            migrations::postgres::run(&pool)
                .await
                .context("run migrations")?;

            ledger_db::init(
                ledger_db::Config {
                    cache_max_nodes: 1_024,
                },
                pool,
            );

            postgres_container
        };

        #[cfg(feature = "standalone")]
        {
            use crate::infra::{
                ledger_db, migrations,
                pool::{self, sqlite::SqlitePool},
            };

            let temp_dir = tempfile::tempdir().context("cannot create tempdir")?;
            let sqlite_file = temp_dir.path().join("indexer.sqlite").display().to_string();
            let sqlite_ledger_db_file = temp_dir
                .path()
                .join("ledger-db.sqlite")
                .display()
                .to_string();

            let pool = SqlitePool::new(pool::sqlite::Config::with_url(sqlite_file))
                .await
                .context("create pool")?;
            migrations::sqlite::run(&pool)
                .await
                .context("run migrations")?;

            ledger_db::init(ledger_db::Config {
                cache_max_nodes: 1_024,
                cnn_url: sqlite_ledger_db_file,
            })
            .await
            .expect("ledger DB can be initialized");
        }

        let transaction = fs::read(format!("{}/tests/tx_1_2_2.raw", env!("CARGO_MANIFEST_DIR")))
            .expect("transaction file can be read");
        let transaction = Transaction::deserialize(transaction, LedgerVersion::V9)
            .expect("transaction can be deserialized");

        assert!(transaction.relevant(viewing_key(1)));
        assert!(transaction.relevant(viewing_key(2)));
        assert!(!transaction.relevant(viewing_key(3)));

        let transaction = fs::read(format!("{}/tests/tx_1_2_3.raw", env!("CARGO_MANIFEST_DIR")))
            .expect("transaction file can be read");
        let transaction = Transaction::deserialize(transaction, LedgerVersion::V9)
            .expect("transaction can be deserialized");

        assert!(transaction.relevant(viewing_key(1)));
        assert!(!transaction.relevant(viewing_key(2)));
        assert!(transaction.relevant(viewing_key(3)));

        Ok(())
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
