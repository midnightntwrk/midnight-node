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

use indexer_common::domain::NetworkId;
use std::{
    error::Error as StdError,
    sync::{Arc, atomic::AtomicBool},
};

/// API abstraction.
#[trait_variant::make(Send)]
pub trait Api
where
    Self: 'static,
{
    type Error: StdError + Send + Sync + 'static;

    /// Serve the API.
    async fn serve(
        self,
        network_id: NetworkId,
        caught_up: Arc<AtomicBool>,
    ) -> Result<(), Self::Error>;
}
