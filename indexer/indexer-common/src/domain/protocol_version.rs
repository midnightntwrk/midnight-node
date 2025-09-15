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

use derive_more::From;
use parity_scale_codec::Decode;
use serde::Deserialize;
use std::{
    fmt::{self, Display},
    num::TryFromIntError,
};
use thiserror::Error;

pub const PROTOCOL_VERSION_000_016_000: ProtocolVersion = ProtocolVersion(16_000);

/// The runtime specification version of the chain; defaults to 1, i.e. 0.0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, From)]
pub struct ProtocolVersion(pub u32);

impl ProtocolVersion {
    /// Get compatibility with given other [ProtocolVersion] based upon major and minor equality.
    pub fn is_compatible(&self, other: ProtocolVersion) -> bool {
        self.major() == other.major() && self.minor() == other.minor()
    }

    /// The major version, i.e. `1` in `1.2.3`.
    pub fn major(&self) -> u32 {
        self.0 / 1_000_000
    }

    /// The minor version, i.e. `2` in `1.2.3`.
    pub fn minor(&self) -> u32 {
        self.0 / 1_000 % 1_000
    }

    /// The patch version, i.e. `3` in `1.2.3`.
    pub fn patch(&self) -> u32 {
        self.0 % 1_000
    }
}

impl Default for ProtocolVersion {
    fn default() -> Self {
        Self(1)
    }
}

impl TryFrom<&[u8]> for ProtocolVersion {
    type Error = ScaleDecodeProtocolVersionError;

    /// Used to SCALE decode the `ProtocolVersion` from a block header from the node.
    fn try_from(mut value: &[u8]) -> Result<Self, Self::Error> {
        let value = u32::decode(&mut value)?;
        Ok(Self(value))
    }
}

impl TryFrom<i64> for ProtocolVersion {
    type Error = TryFromIntError;

    fn try_from(value: i64) -> Result<Self, Self::Error> {
        let value = u32::try_from(value)?;
        Ok(Self(value))
    }
}

impl Display for ProtocolVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let major = self.major();
        let minor = self.minor();
        let patch = self.patch();
        write!(f, "{major}.{minor}.{patch}")
    }
}

/// Error possibly returned by `ProtocolVersion::try_from<&[u8]>`.
#[derive(Debug, Error)]
#[error("cannot SCALE decode protocol version")]
pub struct ScaleDecodeProtocolVersionError(#[from] parity_scale_codec::Error);

#[cfg(test)]
mod tests {
    use crate::domain::ProtocolVersion;

    #[test]
    fn test_protocol_version_display() {
        let version = ProtocolVersion::from(13_000);
        assert_eq!(version.to_string(), "0.13.0");

        let version = ProtocolVersion::from(1_002_003);
        assert_eq!(version.to_string(), "1.2.3");

        let version = ProtocolVersion::from(666_042);
        assert_eq!(version.to_string(), "0.666.42")
    }
}
