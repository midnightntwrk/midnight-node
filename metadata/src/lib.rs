// This file is part of midnight-node.
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

// Generate an interface that we can use from the node's metadata.
#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.21.0.scale")]
pub mod midnight_metadata_0_21_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_0.22.0.scale")]
pub mod midnight_metadata_0_22_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_1.0.0.scale")]
pub mod midnight_metadata_1_0_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_2.0.0.scale")]
pub mod midnight_metadata_2_0_0 {}

#[subxt::subxt(runtime_metadata_path = "static/midnight_metadata_2.1.0.scale")]
pub mod midnight_metadata_2_1_0 {}

pub use midnight_metadata_2_1_0 as midnight_metadata_latest;
