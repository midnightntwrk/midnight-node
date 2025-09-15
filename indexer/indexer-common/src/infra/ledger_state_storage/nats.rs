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

use crate::domain::{LedgerStateStorage, ProtocolVersion, ledger::SerializedLedgerState};
use async_nats::{
    ConnectError, ConnectOptions,
    jetstream::{
        self, Context as Jetstream,
        context::CreateObjectStoreError,
        object_store::{
            self, DeleteError, GetError, ListError, ObjectStore, PutError, WatcherError,
        },
    },
};
use fastrace::trace;
use futures::{StreamExt, TryStreamExt, stream};
use log::{debug, error, info};
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use std::{
    array::TryFromSliceError,
    io::{self, Cursor},
};
use thiserror::Error;
use tokio::io::AsyncReadExt;
use tokio_util::io::StreamReader;

const BUCKET_NAME: &str = "ledger_state_store";
const OBJECT_NAME: &str = "ledger_state";

/// NATS based ledger state storage implementation.
pub struct NatsLedgerStateStorage {
    ledger_state_store: ObjectStore,
}

impl NatsLedgerStateStorage {
    /// Create a new ledger state storage with the given configuration.
    pub async fn new(config: Config) -> Result<Self, Error> {
        let Config {
            url,
            username,
            password,
        } = config;

        let options =
            ConnectOptions::new().user_and_password(username, password.expose_secret().to_owned());
        let client = options.connect(url).await?;
        let jetstream = jetstream::new(client);
        let ledger_state_store = create_ledger_state_store(&jetstream).await?;

        Ok(Self { ledger_state_store })
    }

    #[trace]
    async fn current_object_name(&self) -> Result<Option<String>, LedgerStateStorageError> {
        let objects = self.ledger_state_store.list().await?;

        let names = objects
            .map_ok(|info| info.name)
            .try_collect::<Vec<_>>()
            .await?;
        let object_name = names.into_iter().max();

        Ok(object_name)
    }
}

impl LedgerStateStorage for NatsLedgerStateStorage {
    type Error = LedgerStateStorageError;

    #[trace]
    async fn load_last_index(&self) -> Result<Option<u64>, Self::Error> {
        let object_name = self.current_object_name().await?;
        debug!(object_name; "loading highest_zswap_state_index");

        match object_name {
            Some(object_name) => {
                let object = self.ledger_state_store.get(object_name).await;

                match object {
                    Ok(mut object) => {
                        let highest_zswap_state_index = object.read_u64_le().await?;

                        // We (ab)use `u64::MAX` as None!
                        Ok((highest_zswap_state_index != u64::MAX)
                            .then_some(highest_zswap_state_index))
                    }

                    Err(error) if error.kind() == object_store::GetErrorKind::NotFound => Ok(None),

                    Err(other) => Err(other)?,
                }
            }

            None => Ok(None),
        }
    }

    #[trace]
    async fn load_ledger_state(
        &self,
    ) -> Result<Option<(SerializedLedgerState, u32, ProtocolVersion)>, Self::Error> {
        let object_name = self.current_object_name().await?;
        debug!(object_name; "loading ledger state");

        match object_name {
            Some(object_name) => {
                let object = self.ledger_state_store.get(object_name).await;

                match object {
                    Ok(mut object) => {
                        let _ = object.read_u64_le().await?;
                        let block_height = object.read_u32_le().await?;
                        let protocol_version = object.read_u32_le().await?.into();
                        let mut bytes = Vec::with_capacity(object.info.size - 12);
                        object.read_to_end(&mut bytes).await?;

                        Ok(Some((bytes.into(), block_height, protocol_version)))
                    }

                    Err(error) if error.kind() == object_store::GetErrorKind::NotFound => Ok(None),

                    Err(other) => Err(other)?,
                }
            }

            None => Ok(None),
        }
    }

    #[trace]
    async fn save(
        &mut self,
        ledger_state: &SerializedLedgerState,
        block_height: u32,
        highest_zswap_state_index: Option<u64>,
        protocol_version: ProtocolVersion,
    ) -> Result<(), Self::Error> {
        info!(block_height, highest_zswap_state_index:?; "saving ledger state");

        // We (ab)use `u64::MAX` as None!
        let highest_zswap_state_index = highest_zswap_state_index.unwrap_or(u64::MAX).to_le_bytes();
        let highest_zswap_state_index = Cursor::new(highest_zswap_state_index.as_slice());

        let block_height = block_height.to_le_bytes();
        let block_height = Cursor::new(block_height.as_slice());

        let protocol_version = protocol_version.0.to_le_bytes();
        let protocol_version = Cursor::new(protocol_version.as_slice());

        let ledger_state = Cursor::new(ledger_state.as_ref());

        let object = stream::iter(
            [
                highest_zswap_state_index,
                block_height,
                protocol_version,
                ledger_state,
            ]
            .into_iter(),
        );
        let mut object = StreamReader::new(object.map(Ok::<_, io::Error>));

        self.ledger_state_store
            .put(OBJECT_NAME, &mut object)
            .await?;

        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot create storage, because cannot connect")]
    Connect(#[from] ConnectError),

    #[error("cannot create storage, because cannot create object store")]
    CreateObjectStore(#[from] CreateObjectStoreError),
}

#[derive(Debug, Error)]
pub enum LedgerStateStorageError {
    #[error("cannot convert into last index")]
    LastIndex(#[from] TryFromSliceError),

    #[error("cannot load ledger state")]
    Get(#[from] GetError),

    #[error("cannot load ledger state")]
    Read(#[from] io::Error),

    #[error("cannot save ledger state")]
    Put(#[from] PutError),

    #[error("cannot cleanup ledger state store")]
    Cleanup(CleanupError),

    #[error("trying to receive an error from the cleanup task failed")]
    CleanupErrorReceiver,

    #[error("cannot list objects")]
    List(#[from] ListError),

    #[error("cannot get next listed object")]
    Next(#[from] WatcherError),
}

/// Configuration settings for [NatsLedgerStateStorage].
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub url: String,
    pub username: String,
    pub password: SecretString,
}

#[derive(Debug, Error)]
pub enum CleanupError {
    #[error("cannot list objects")]
    List(#[from] ListError),

    #[error("cannot get next listed object")]
    Next(#[from] WatcherError),

    #[error("cannot delete object")]
    Delete(#[from] DeleteError),
}

async fn create_ledger_state_store(
    jetstream: &Jetstream,
) -> Result<ObjectStore, CreateObjectStoreError> {
    let config = object_store::Config {
        bucket: BUCKET_NAME.to_string(),
        ..Default::default()
    };

    jetstream.create_object_store(config).await
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{ByteVec, LedgerStateStorage, PROTOCOL_VERSION_000_016_000},
        infra::ledger_state_storage::nats::{Config, NatsLedgerStateStorage},
    };
    use anyhow::Context;
    use assert_matches::assert_matches;
    use std::time::{Duration, Instant};
    use testcontainers::{GenericImage, ImageExt, core::WaitFor, runners::AsyncRunner};
    use tokio::time::sleep;

    #[tokio::test]
    async fn test() -> anyhow::Result<()> {
        let nats_container = GenericImage::new("nats", "2.11.1")
            .with_wait_for(WaitFor::message_on_stderr("Server is ready"))
            .with_cmd([
                "--user",
                "indexer",
                "--pass",
                env!("APP__INFRA__LEDGER_STATE_STORAGE__PASSWORD"),
                "-js",
            ])
            .start()
            .await
            .context("start NATS container")?;

        // In spite of the above "WaitFor" NATS stubbornly rejects connections.
        let start = Instant::now();
        while reqwest::get("localhost:8222/healthz")
            .await
            .and_then(|r| r.error_for_status())
            .is_err()
            && Instant::now() - start < Duration::from_millis(1_500)
        {
            sleep(Duration::from_millis(100)).await;
        }

        let nats_port = nats_container
            .get_host_port_ipv4(4222)
            .await
            .context("get NATS port")?;
        let nats_url = format!("localhost:{nats_port}");

        let config = Config {
            url: nats_url,
            username: "indexer".to_string(),
            password: env!("APP__INFRA__LEDGER_STATE_STORAGE__PASSWORD").into(),
        };
        let mut ledger_state_storage = NatsLedgerStateStorage::new(config)
            .await
            .context("create NatsZswapStateStorage")?;

        let last_index = ledger_state_storage
            .load_last_index()
            .await
            .context("load last index")?;
        assert!(last_index.is_none());

        let ledger_state = ledger_state_storage
            .load_ledger_state()
            .await
            .context("load ledger state")?;
        assert!(ledger_state.is_none());

        let default_state = ByteVec::default();
        ledger_state_storage
            .save(&default_state, 0, None, PROTOCOL_VERSION_000_016_000)
            .await
            .context("save ledger state")?;

        let last_index = ledger_state_storage
            .load_last_index()
            .await
            .context("load last index")?;
        assert!(last_index.is_none());

        let ledger_state = ledger_state_storage
            .load_ledger_state()
            .await
            .context("load ledger state")?;
        assert_matches!(
            ledger_state,
            Some((state, 0, PROTOCOL_VERSION_000_016_000)) if state == default_state
        );

        ledger_state_storage
            .save(
                &ByteVec::default(),
                42,
                Some(42),
                PROTOCOL_VERSION_000_016_000,
            )
            .await
            .context("save ledger state")?;

        let last_index = ledger_state_storage
            .load_last_index()
            .await
            .context("load last index")?;
        assert_matches!(last_index, Some(42));

        Ok(())
    }
}
