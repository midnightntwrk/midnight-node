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

use crate::{
    domain::{Message, Publisher},
    infra::pub_sub::nats::Config,
};
use async_nats::{Client, ConnectOptions};
use secrecy::ExposeSecret;
use thiserror::Error;

// NATS based [Publisher] implementation.
#[derive(Clone)]
pub struct NatsPublisher {
    client: Client,
}

impl NatsPublisher {
    pub async fn new(config: Config) -> Result<Self, Error> {
        let Config {
            url,
            username,
            password,
        } = config;

        let options =
            ConnectOptions::new().user_and_password(username, password.expose_secret().to_owned());
        let client = options.connect(url).await?;

        Ok(Self { client })
    }
}

impl Publisher for NatsPublisher {
    type Error = PublisherError;

    async fn publish<T>(&self, message: &T) -> Result<(), Self::Error>
    where
        T: Message + Send + Sync,
    {
        let payload = serde_json::to_vec(message)?.into();
        self.client.publish(T::TOPIC, payload).await?;
        Ok(())
    }
}

#[derive(Debug, Error)]
#[error("cannot create NATS based publisher")]
pub struct Error(#[from] async_nats::ConnectError);

#[derive(Debug, Error)]
pub enum PublisherError {
    #[error("cannot convert subject to UTF-8")]
    Utf8(#[from] std::str::Utf8Error),

    #[error("cannot JSON serialize message")]
    Serialize(#[from] serde_json::Error),

    #[error("cannot publish message")]
    Publish(#[from] async_nats::client::PublishError),
}
