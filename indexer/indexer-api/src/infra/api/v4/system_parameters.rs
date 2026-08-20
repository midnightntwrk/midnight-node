// This file is part of midnight-indexer.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::{
    domain::system_parameters as domain,
    infra::api::v4::{HexEncodable, HexEncoded},
};
use async_graphql::SimpleObject;

/// System parameters at a specific block height.
#[derive(Debug, SimpleObject)]
pub struct SystemParameters {
    /// The D-parameter controlling validator committee composition.
    pub d_parameter: DParameter,

    /// The current Terms and Conditions, if any have been set.
    pub terms_and_conditions: Option<TermsAndConditions>,
}

/// The D-parameter controlling validator committee composition.
#[derive(Debug, SimpleObject)]
#[graphql(name = "DParameter")]
pub struct DParameter {
    /// Number of permissioned candidates.
    pub num_permissioned_candidates: u16,

    /// Number of registered candidates.
    pub num_registered_candidates: u16,
}

/// Terms and Conditions agreement.
#[derive(Debug, SimpleObject)]
pub struct TermsAndConditions {
    /// The hex-encoded hash of the Terms and Conditions document.
    pub hash: HexEncoded,

    /// The URL where the Terms and Conditions can be found.
    pub url: String,
}

/// D-parameter change record for history queries.
#[derive(Debug, SimpleObject)]
#[graphql(name = "DParameterChange")]
pub struct DParameterChange {
    /// The block height where this parameter became effective.
    pub block_height: u32,

    /// The hex-encoded block hash where this parameter became effective.
    pub block_hash: HexEncoded,

    /// The UNIX timestamp when this parameter became effective.
    pub timestamp: u64,

    /// Number of permissioned candidates.
    pub num_permissioned_candidates: u16,

    /// Number of registered candidates.
    pub num_registered_candidates: u16,
}

/// Terms and Conditions change record for history queries.
#[derive(Debug, SimpleObject)]
pub struct TermsAndConditionsChange {
    /// The block height where this T&C version became effective.
    pub block_height: u32,

    /// The hex-encoded block hash where this T&C version became effective.
    pub block_hash: HexEncoded,

    /// The UNIX timestamp when this T&C version became effective.
    pub timestamp: u64,

    /// The hex-encoded hash of the Terms and Conditions document.
    pub hash: HexEncoded,

    /// The URL where the Terms and Conditions can be found.
    pub url: String,
}

impl From<domain::DParameter> for DParameter {
    fn from(value: domain::DParameter) -> Self {
        DParameter {
            num_permissioned_candidates: value.num_permissioned_candidates,
            num_registered_candidates: value.num_registered_candidates,
        }
    }
}

impl From<domain::TermsAndConditions> for TermsAndConditions {
    fn from(value: domain::TermsAndConditions) -> Self {
        TermsAndConditions {
            hash: value.hash.hex_encode(),
            url: value.url,
        }
    }
}

impl From<domain::DParameter> for DParameterChange {
    fn from(value: domain::DParameter) -> Self {
        DParameterChange {
            block_height: value.block_height,
            block_hash: value.block_hash.hex_encode(),
            timestamp: value.timestamp,
            num_permissioned_candidates: value.num_permissioned_candidates,
            num_registered_candidates: value.num_registered_candidates,
        }
    }
}

impl From<domain::TermsAndConditions> for TermsAndConditionsChange {
    fn from(value: domain::TermsAndConditions) -> Self {
        TermsAndConditionsChange {
            block_height: value.block_height,
            block_hash: value.block_hash.hex_encode(),
            timestamp: value.timestamp,
            hash: value.hash.hex_encode(),
            url: value.url,
        }
    }
}
