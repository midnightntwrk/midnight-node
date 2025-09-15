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

use crate::infra::pub_sub::in_mem::{publisher::InMemPublisher, subscriber::InMemSubscriber};
use log::error;
use serde_json::Value;
use tokio::{
    sync::broadcast::{self, Sender, error::RecvError},
    task,
};

/// Factory for in memory based implementations for publishers and subscribers.
#[derive(Clone)]
pub struct InMemPubSub {
    block_indexed_sender: Sender<Value>,
    wallet_indexed_sender: Sender<Value>,
    unshielded_utxo_sender: Sender<Value>,
}

impl InMemPubSub {
    /// Factory for [InMemPublisher].
    pub fn publisher(&self) -> InMemPublisher {
        InMemPublisher::new(self.clone())
    }

    /// Factory for [InMemSubscriber].
    pub fn subscriber(&self) -> InMemSubscriber {
        InMemSubscriber::new(self.clone())
    }
}

impl Default for InMemPubSub {
    fn default() -> Self {
        let (block_indexed_sender, mut block_indexed_receiver) = broadcast::channel(42);
        let (wallet_indexed_sender, mut wallet_indexed_receiver) = broadcast::channel(42);
        let (unshielded_utxo_sender, mut unshielded_utxo_receiver) = broadcast::channel(42);

        let pub_sub = InMemPubSub {
            block_indexed_sender,
            wallet_indexed_sender,
            unshielded_utxo_sender,
        };

        task::spawn(async move {
            loop {
                if let Err(RecvError::Lagged(_)) = block_indexed_receiver.recv().await {
                    error!("cannot drain block_indexed_receiver");
                    break;
                };
            }
        });

        task::spawn(async move {
            loop {
                if let Err(RecvError::Lagged(_)) = wallet_indexed_receiver.recv().await {
                    error!("cannot drain wallet_indexed_receiver");
                    break;
                };
            }
        });

        task::spawn(async move {
            loop {
                if let Err(RecvError::Lagged(_)) = unshielded_utxo_receiver.recv().await {
                    error!("cannot drain unshielded_utxo_receiver");
                    break;
                };
            }
        });

        pub_sub
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{BlockIndexed, Publisher, Subscriber, WalletIndexed},
        infra::pub_sub::in_mem::InMemPubSub,
    };
    use assert_matches::assert_matches;
    use futures::StreamExt;
    use std::{error::Error as StdError, time::Duration};
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_publish_subscribe() -> Result<(), Box<dyn StdError>> {
        let pub_sub = InMemPubSub::default();
        sleep(Duration::from_millis(50)).await; //testing if IN_MEM_PUB_SUB doesn't get dropped

        let block_indexed = BlockIndexed {
            height: 123,
            max_transaction_id: None,
            caught_up: false,
        };
        let publish_block_res = pub_sub.publisher().publish(&block_indexed).await;

        assert!(publish_block_res.is_ok());

        let subscriber = pub_sub.subscriber();
        let mut messages = subscriber.subscribe::<WalletIndexed>();

        let wallet_indexed = WalletIndexed {
            session_id: [0; 32].into(),
        };
        pub_sub.publisher().publish(&wallet_indexed).await?;

        let message = messages.next().await;
        assert_matches!(message, Some(Ok(message)) if message == wallet_indexed);

        Ok(())
    }
}
