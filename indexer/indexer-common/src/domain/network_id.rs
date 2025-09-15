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

use std::{
    fmt::{self, Display},
    str::FromStr,
};
use thiserror::Error;

const UNDEPLOYED: &str = "undeployed";
const DEV_NET: &str = "devnet";
const TEST_NET: &str = "testnet";
const MAIN_NET: &str = "mainnet";

/// Clone of midnight_serialize::NetworkId for the purpose of Serde deserialization.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkId {
    Undeployed,
    DevNet,
    TestNet,
    MainNet,
}

impl Display for NetworkId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NetworkId::Undeployed => write!(f, "{UNDEPLOYED}"),
            NetworkId::DevNet => write!(f, "{DEV_NET}"),
            NetworkId::TestNet => write!(f, "{TEST_NET}"),
            NetworkId::MainNet => write!(f, "{MAIN_NET}"),
        }
    }
}

impl From<NetworkId> for String {
    fn from(network_id: NetworkId) -> Self {
        network_id.to_string()
    }
}

impl FromStr for NetworkId {
    type Err = UnknownNetworkIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        s.try_into()
    }
}

impl TryFrom<&str> for NetworkId {
    type Error = UnknownNetworkIdError;

    fn try_from(s: &str) -> Result<Self, Self::Error> {
        match s.to_lowercase().as_str() {
            UNDEPLOYED => Ok(Self::Undeployed),
            DEV_NET => Ok(Self::DevNet),
            TEST_NET => Ok(Self::TestNet),
            MAIN_NET => Ok(Self::MainNet),
            _ => Err(UnknownNetworkIdError(s.to_owned())),
        }
    }
}

#[derive(Debug, Error)]
#[error("unknown NetworkId {0}")]
pub struct UnknownNetworkIdError(String);

#[cfg(test)]
mod tests {
    use crate::domain::NetworkId;
    use assert_matches::assert_matches;

    #[test]
    fn test_network_id() {
        assert!("foo".parse::<NetworkId>().is_err());

        let network_id = NetworkId::Undeployed.to_string().parse::<NetworkId>();
        assert_matches!(network_id, Ok(NetworkId::Undeployed));

        let network_id = NetworkId::DevNet.to_string().parse::<NetworkId>();
        assert_matches!(network_id, Ok(NetworkId::DevNet));

        let network_id = NetworkId::TestNet.to_string().parse::<NetworkId>();
        assert_matches!(network_id, Ok(NetworkId::TestNet));

        let network_id = NetworkId::MainNet.to_string().parse::<NetworkId>();
        assert_matches!(network_id, Ok(NetworkId::MainNet));
    }
}
