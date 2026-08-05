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

//! Observability for the upcoming AURA-to-BABE consensus migration: tells us
//! whether a node operator has put a usable BABE key in their keystore.
//!
//! When the chain flips to BABE, a validator whose keystore holds no BABE key
//! (or the wrong one) silently stops producing blocks. Rather than find out at
//! the flip, we check ahead of time:
//!
//! - [`probe::BabeKeyProbe`] reads the BABE keys from the keystore, reads the
//!   `babe` keys of the permissioned candidates registered on Cardano, and
//!   returns the intersection.
//! - [`reporter::run`] runs the probe on an interval and publishes
//!   `midnight_babe_key_registered` (1 = a matching key is present, 0 = none).
//!
//! The probe must be given the plain node keystore, not
//! [`crate::aura_to_babe_migration_keystore::AuraToBabeMigrationKeystore`],
//! which answers BABE queries with AURA keys and would therefore report every
//! node as ready.

pub mod probe;
pub mod reporter;

const LOG_TARGET: &str = "babe-key-readiness";
