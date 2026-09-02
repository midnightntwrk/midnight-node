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

use crate::domain::{
    storage::NoopStorage,
    system_parameters::{DParameter, TermsAndConditions},
};

/// System parameters storage abstraction.
#[trait_variant::make(Send)]
pub trait SystemParametersStorage: Clone + Send + Sync + 'static {
    /// Get Terms and Conditions at or before the given block height.
    async fn get_terms_and_conditions_at(
        &self,
        block_height: u32,
    ) -> Result<Option<TermsAndConditions>, sqlx::Error>;

    /// Get D-Parameter at or before the given block height.
    async fn get_d_parameter_at(
        &self,
        block_height: u32,
    ) -> Result<Option<DParameter>, sqlx::Error>;

    /// Get all Terms and Conditions history.
    async fn get_terms_and_conditions_history(
        &self,
    ) -> Result<Vec<TermsAndConditions>, sqlx::Error>;

    /// Get all D-Parameter history.
    async fn get_d_parameter_history(&self) -> Result<Vec<DParameter>, sqlx::Error>;
}

#[allow(unused_variables)]
impl SystemParametersStorage for NoopStorage {
    async fn get_terms_and_conditions_at(
        &self,
        block_height: u32,
    ) -> Result<Option<TermsAndConditions>, sqlx::Error> {
        Ok(None)
    }

    async fn get_d_parameter_at(
        &self,
        block_height: u32,
    ) -> Result<Option<DParameter>, sqlx::Error> {
        Ok(None)
    }

    async fn get_terms_and_conditions_history(
        &self,
    ) -> Result<Vec<TermsAndConditions>, sqlx::Error> {
        Ok(vec![])
    }

    async fn get_d_parameter_history(&self) -> Result<Vec<DParameter>, sqlx::Error> {
        Ok(vec![])
    }
}
