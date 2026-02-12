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

//! Genesis verification modules for validating genesis configuration and authorization scripts.

pub mod verify_auth_script_common;
pub mod verify_federated_authority_auth_script;
pub mod verify_ics_auth_script;
pub mod verify_ledger_state_genesis;
pub mod verify_permissioned_candidates_auth_script;
