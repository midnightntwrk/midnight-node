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

mod contract_state;
mod ledger_state;
mod secret_key;
mod transaction;

pub use contract_state::*;
pub use ledger_state::*;
pub use secret_key::*;
pub use transaction::*;

use crate::{
    domain::{ByteVec, ProtocolVersion},
    error::BoxError,
};
use fastrace::trace;
use midnight_base_crypto_v6::signatures::Signature as SignatureV6;
use midnight_ledger_v6::structure::ProofMarker as ProofMarkerV6;
use midnight_serialize_v6::{
    Serializable as SerializableV6, Tagged as TaggedV6, tagged_serialize as tagged_serialize_v6,
};
use midnight_storage_v6::DefaultDB as DefaultDBV6;
use midnight_transient_crypto_v6::commitment::PureGeneratorPedersen as PureGeneratorPedersenV6;
use std::io;
use thiserror::Error;

type TransactionV6 = midnight_ledger_v6::structure::Transaction<
    SignatureV6,
    ProofMarkerV6,
    PureGeneratorPedersenV6,
    DefaultDBV6,
>;
type IntentV6 = midnight_ledger_v6::structure::Intent<
    SignatureV6,
    ProofMarkerV6,
    PureGeneratorPedersenV6,
    DefaultDBV6,
>;

/// Ledger related errors.
#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Io(&'static str, #[source] io::Error),

    #[error("invalid protocol version {0}")]
    InvalidProtocolVersion(ProtocolVersion),

    #[error("cannot get contract state from node")]
    GetContractState(#[source] BoxError),

    #[error("serialized TokenType should have 32 bytes, but had {0}")]
    TokenTypeLen(usize),

    #[error("invalid merkle-tree collapsed update")]
    InvalidUpdate(#[source] BoxError),

    #[error("malformed transaction")]
    MalformedTransaction(#[source] BoxError),

    #[error("invalid system transaction")]
    SystemTransaction(#[source] BoxError),
}

/// Extension methods for `Serializable` implementations.
pub trait SerializableV6Ext
where
    Self: SerializableV6,
{
    /// Serialize this `Serializable` implementation.
    #[trace]
    fn serialize_v6(&self) -> Result<ByteVec, io::Error> {
        let mut bytes = Vec::with_capacity(self.serialized_size() + 32);
        SerializableV6::serialize(self, &mut bytes)?;
        Ok(bytes.into())
    }
}

impl<T> SerializableV6Ext for T where T: SerializableV6 {}

/// Extension methods for `Serializable + Tagged` implementations.
pub trait TaggedSerializableV6Ext
where
    Self: SerializableV6 + TaggedV6 + Sized,
{
    /// Serialize this `Serializable + Tagged` implementation.
    #[trace]
    fn tagged_serialize_v6(&self) -> Result<ByteVec, io::Error> {
        let mut bytes = Vec::with_capacity(self.serialized_size() + 32);
        tagged_serialize_v6(self, &mut bytes)?;
        Ok(bytes.into())
    }
}

impl<T> TaggedSerializableV6Ext for T where T: SerializableV6 + TaggedV6 {}
