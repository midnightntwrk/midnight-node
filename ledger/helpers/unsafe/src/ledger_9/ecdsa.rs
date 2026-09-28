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

//! ECDSA keys for ledger 9 and later.
//!
//! From ledger 9 the ledger has a native ECDSA unshielded identity: `coin-structure`
//! provides `From<ecdsa::VerifyingKey> for UserAddress` and the signature enums carry an
//! `ECDSA` variant, so the real `base_crypto` keys are used directly. The `ledger_8` copy
//! of this file is a set of panicking stubs instead.

pub use crate::ledger_9::base_crypto::ecdsa::{
	SigningKey as SigningKeyEcdsa, VerifyingKey as VerifyingKeyEcdsa,
};
