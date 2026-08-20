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

mod contract_state;
mod ledger_state;
mod secret_key;
mod state_translation_v8_to_v9;
mod transaction;

pub use contract_state::*;
pub use ledger_state::*;
pub use secret_key::*;
pub use transaction::*;

use crate::{
    domain::{
        ByteArrayLenError, ByteVec, LedgerVersion, SerializedContractAddress,
        SerializedLedgerStateKey, dust::DustParameters,
    },
    error::BoxError,
};
use fastrace::trace;
use midnight_base_crypto_v1::signatures::Signature;
use midnight_ledger_v8::{
    dust::INITIAL_DUST_PARAMETERS as INITIAL_DUST_PARAMETERS_V8,
    structure::ProofMarker as ProofMarkerV8,
};
use midnight_ledger_v9::{
    dust::INITIAL_DUST_PARAMETERS as INITIAL_DUST_PARAMETERS_V9,
    structure::{ProofMarker as ProofMarkerV9, Signature as SignatureV9},
};
use midnight_serialize_v1::{Serializable, Tagged, tagged_serialize};
use midnight_transient_crypto_v2::commitment::PureGeneratorPedersen;
use midnight_transient_crypto_v3::commitment::PureGeneratorPedersen as PureGeneratorPedersenV9;
use std::{io, string::FromUtf8Error};
use thiserror::Error;

type TransactionV8<D> =
    midnight_ledger_v8::structure::Transaction<Signature, ProofMarkerV8, PureGeneratorPedersen, D>;

type IntentV8<D> =
    midnight_ledger_v8::structure::Intent<Signature, ProofMarkerV8, PureGeneratorPedersen, D>;

type TransactionV9<D> = midnight_ledger_v9::structure::Transaction<
    SignatureV9,
    ProofMarkerV9,
    PureGeneratorPedersenV9,
    D,
>;

type IntentV9<D> =
    midnight_ledger_v9::structure::Intent<SignatureV9, ProofMarkerV9, PureGeneratorPedersenV9, D>;

/// Ledger related errors.
#[derive(Debug, Error)]
pub enum Error {
    #[error("cannot load ledger state for key {}", const_hex::encode(.0))]
    LoadLedgerState(SerializedLedgerStateKey, #[source] io::Error),

    #[error("cannot serialize {0}")]
    Serialize(&'static str, #[source] io::Error),

    #[error("cannot deserialize {0}")]
    Deserialize(&'static str, #[source] io::Error),

    #[error("cannot convert {0} to UTF-8 string")]
    FromUtf8(&'static str, #[source] FromUtf8Error),

    #[error("cannot get contract state from node for address {0}")]
    GetContractState(SerializedContractAddress, #[source] BoxError),

    #[error(transparent)]
    ByteArrayLen(ByteArrayLenError),

    #[error("invalid Merkle tree collapsed update")]
    InvalidUpdate(#[source] BoxError),

    #[error("malformed transaction")]
    MalformedTransaction(#[source] BoxError),

    #[error("invalid system transaction")]
    SystemTransaction(#[source] BoxError),

    #[error("block limit exceeded during post_block_update")]
    BlockLimitExceeded(#[source] BoxError),

    #[error("cannot calculate transaction cost")]
    TransactionCost(#[source] BoxError),

    #[error("cannot translate ledger state {0} to old {1}")]
    BackwardsLedgerStateTranslation(LedgerVersion, LedgerVersion),

    #[error("translating ledger state from {0} to {1} not yet supported")]
    UnsupportedLedgerStateTranslation(LedgerVersion, LedgerVersion),

    #[error("cannot translate ledger state from {0} to {1}")]
    LedgerStateTranslation(LedgerVersion, LedgerVersion, #[source] io::Error),

    #[error("unsupported EventDetailsV8 variant: {0}")]
    UnsupportedEventVariant(String),
}

/// Extension methods for `Serializable` implementations.
pub trait SerializableExt
where
    Self: Serializable,
{
    /// Serialize this `Serializable` implementation.
    #[trace]
    fn serialize(&self) -> Result<ByteVec, io::Error> {
        let mut bytes = Vec::with_capacity(self.serialized_size());
        Serializable::serialize(self, &mut bytes)?;
        Ok(bytes.into())
    }
}

impl<T> SerializableExt for T where T: Serializable {}

/// Extension methods for `Serializable + Tagged` implementations.
pub trait TaggedSerializableExt
where
    Self: Serializable + Tagged + Sized,
{
    /// Serialize this `Serializable + Tagged` implementation.
    #[trace]
    fn tagged_serialize(&self) -> Result<ByteVec, io::Error> {
        let mut bytes = Vec::with_capacity(self.serialized_size() + 32);
        tagged_serialize(self, &mut bytes)?;
        Ok(bytes.into())
    }
}

impl<T> TaggedSerializableExt for T where T: Serializable + Tagged {}

/// Get DUST parameters for the given protocol version.
/// Returns the initial DUST parameters from the ledger specification.
/// These parameters define the economic properties of DUST generation:
/// - `night_dust_ratio`: Maximum DUST capacity per NIGHT (5 DUST per NIGHT).
/// - `generation_decay_rate`: Rate of DUST generation (~1 week to reach max).
/// - `dust_grace_period`: Maximum time window for DUST spends (3 hours).
pub fn dust_parameters(ledger_version: LedgerVersion) -> Result<DustParameters, Error> {
    let parameters = match ledger_version {
        LedgerVersion::V8 => DustParameters {
            night_dust_ratio: INITIAL_DUST_PARAMETERS_V8.night_dust_ratio,
            generation_decay_rate: INITIAL_DUST_PARAMETERS_V8.generation_decay_rate,
            dust_grace_period: INITIAL_DUST_PARAMETERS_V8.dust_grace_period.as_seconds() as u64,
        },
        LedgerVersion::V9 => DustParameters {
            night_dust_ratio: INITIAL_DUST_PARAMETERS_V9.night_dust_ratio,
            generation_decay_rate: INITIAL_DUST_PARAMETERS_V9.generation_decay_rate,
            dust_grace_period: INITIAL_DUST_PARAMETERS_V9.dust_grace_period.as_seconds() as u64,
        },
    };

    Ok(parameters)
}
