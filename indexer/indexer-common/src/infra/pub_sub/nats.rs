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

pub mod publisher;
pub mod subscriber;

use crate::domain::Topic;
use async_nats::{Subject, subject::ToSubject};
use secrecy::SecretString;
use serde::Deserialize;

/// Configuration for NATS pub-sub.
#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    pub url: String,
    pub username: String,
    pub password: SecretString,
}

impl ToSubject for Topic {
    fn to_subject(&self) -> Subject {
        format!("pub-sub.{}", self.0).into()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{Publisher, SessionId, Subscriber, WalletIndexed},
        error::BoxError,
        infra::pub_sub::nats::{Config, publisher::NatsPublisher, subscriber::NatsSubscriber},
    };
    use anyhow::Context;
    use futures::{StreamExt, TryStreamExt};
    use std::{
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
        time::{Duration, Instant},
    };
    use testcontainers::{GenericImage, ImageExt, core::WaitFor, runners::AsyncRunner};
    use tokio::{task, time::sleep};

    #[tokio::test]
    async fn test_publish_subscribe() -> Result<(), BoxError> {
        let nats_container = GenericImage::new("nats", "2.11.1")
            .with_wait_for(WaitFor::message_on_stderr("Server is ready"))
            .with_cmd([
                "--user",
                "indexer",
                "--pass",
                env!("APP__INFRA__PUB_SUB__PASSWORD"),
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
            url: nats_url.clone(),
            username: "indexer".to_string(),
            password: env!("APP__INFRA__PUB_SUB__PASSWORD").into(),
        };

        let subscriber = NatsSubscriber::new(config.clone())
            .await
            .context("create NatsSubscriber")?;
        let wallet_indexed_messages = subscriber.subscribe::<WalletIndexed>();

        let wallet_indexed = WalletIndexed::from(SessionId::from([0; 32]));
        let message_received = Arc::new(AtomicBool::new(false));

        task::spawn({
            let publisher = NatsPublisher::new(config)
                .await
                .context("create NatsPublisher")?;
            let message_received = message_received.clone();
            async move {
                while !message_received.load(Ordering::Relaxed) {
                    sleep(Duration::from_millis(100)).await;
                    publisher
                        .publish(&wallet_indexed)
                        .await
                        .expect("can publish");
                }
            }
        });

        let messages = wallet_indexed_messages
            .take(1)
            .try_collect::<Vec<_>>()
            .await
            .context("collect messages")?;
        message_received.store(true, Ordering::Relaxed);
        assert_eq!(messages, [wallet_indexed]);

        Ok(())
    }
}
