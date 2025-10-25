// This file is part of midnight-node.
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

//! Inherent Data Provider
//!
//! This module contains all the methods and types for the inherent data interface.
//! Anything that is called from or passed to the pallet goes here.

#[cfg(feature = "std")]
pub mod cnight_observation;
#[cfg(feature = "std")]
pub mod federated_authority_observation;

#[cfg(feature = "std")]
pub use cnight_observation::{
	DEFAULT_CARDANO_BLOCK_WINDOW_SIZE, IDPCreationError,
	MidnightCNightObservationInherentDataProvider,
};
#[cfg(feature = "std")]
pub use federated_authority_observation::FederatedAuthorityInherentDataProvider;
