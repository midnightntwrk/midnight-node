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
    domain::{Message, Subscriber, Topic},
    infra::pub_sub::in_mem::InMemPubSub,
};
use futures::{Stream, StreamExt};
use std::fmt::Debug;
use thiserror::Error;
use tokio_stream::wrappers::BroadcastStream;

/// In memory based implementations for [Subscriber].
#[derive(Clone)]
pub struct InMemSubscriber(InMemPubSub);

impl InMemSubscriber {
    #[allow(missing_docs)]
    pub fn new(in_mem_pub_sub: InMemPubSub) -> Self {
        Self(in_mem_pub_sub)
    }
}

impl Subscriber for InMemSubscriber {
    type Error = SubscriberError;

    fn subscribe<T>(&self) -> impl Stream<Item = Result<T, Self::Error>>
    where
        T: Message,
    {
        let values = match T::TOPIC {
            Topic("BlockIndexed") => {
                let receiver = self.0.block_indexed_sender.subscribe();
                BroadcastStream::new(receiver)
            }

            Topic("WalletIndexed") => {
                let receiver = self.0.wallet_indexed_sender.subscribe();
                BroadcastStream::new(receiver)
            }

            Topic("UnshieldedUtxoIndexed") => {
                let receiver = self.0.unshielded_utxo_sender.subscribe();
                BroadcastStream::new(receiver)
            }

            // This must not happen; if it happens, we forgot to add an arm for the topic above!
            _ => panic!("unexpected topic {:?}", T::TOPIC),
        };

        values.map(|value| {
            let value = value?;
            let message = serde_json::from_value::<T>(value)?;
            Ok(message)
        })
    }
}

#[derive(Debug, Error)]
pub enum SubscriberError {
    #[error("cannot receive")]
    Receive(#[from] tokio_stream::wrappers::errors::BroadcastStreamRecvError),

    #[error("cannot JSON deserialize message")]
    Deserialize(#[from] serde_json::Error),
}
