// This file is part of midnight-indexer.
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

use indexer_common::domain::{ProtocolVersion, ProtocolVersionError};
use subxt::config::substrate::{ConsensusEngineId, DigestItem, SubstrateHeader};

const VERSION_ID: ConsensusEngineId = *b"MNSV";

/// Extension methods for Substrate block headers.
pub trait SubstrateHeaderExt {
    /// Try to decode the [ProtocolVersion] from this Substrate block header.
    fn protocol_version(&self) -> Result<Option<ProtocolVersion>, ProtocolVersionError>;
}

impl<H> SubstrateHeaderExt for SubstrateHeader<H>
where
    H: subxt::config::Hash,
{
    fn protocol_version(&self) -> Result<Option<ProtocolVersion>, ProtocolVersionError> {
        self.digest
            .logs
            .iter()
            .filter_map(|item| match item {
                DigestItem::Consensus(VERSION_ID, data) => {
                    Some(ProtocolVersion::try_from(data.as_slice()))
                }

                _ => None,
            })
            .next()
            .transpose()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parity_scale_codec::Decode;
    use subxt::utils::H256;

    /// SCALE-encoded header of mainnet block 1_774_491, the last block produced by the node
    /// 0.22 runtime; its Blake2b-256 hash is the on-chain block hash
    /// 0x40e3f67939e6f53e9f36b0a19fd27d484d29c008698f3a2336c5f2529a85dd23.
    const MAINNET_HEADER_1_774_491: &str = "e23dc07f65b1194d134b6d9b3c2f7433329d0512896a1c4543048a166d4fabd96e4e6c0066b383edaece956474a21cad7e4ec177413e76d548e3bf28b9027bccf0d07bb042e5b79564f4e58339a09acb809d7317c5005dfdf032e359282a15ca803eb5b318066175726120b85cba1100000000066d637368805e2e516f21164918a9e2596aa1e4e53ba17d9e2f26630d0bd6332e29ef2b0d4c044d4e535610f055000004424545468403efac35b32306d7f8616834bd0d120d4288f6e6bfe1bf80b859a50bcc7c6417c00805617572610101447e1cc34e163696ea3d8e9d02def8ab8139c1acacc8f02c8c299b0ebd494c02b9e40da3bd66ffb585bbf84547238a5bfd1490bd6a341f5349ad9199c3edfc86";

    /// SCALE-encoded header of mainnet block 1_774_492, the first block produced by the node
    /// 1.0 runtime after the runtime upgrade on 2026-07-20; its Blake2b-256 hash is the
    /// on-chain block hash 0x380266656e11208adbcaa5ab62a6beb9c09ef86de1cb886eac721099177a1a0b.
    const MAINNET_HEADER_1_774_492: &str = "40e3f67939e6f53e9f36b0a19fd27d484d29c008698f3a2336c5f2529a85dd23724e6c0043b0bdbc7c7c66ebdee28b8a792e4ec9b8b5785ed0f9fd7b0a6bc7d91e95fb0f2678fc5f4818323105c833dbe82e79db20cc427a63b8aabb23f1b49c2fcf9beb14066175726120b95cba1100000000066d637368805e2e516f21164918a9e2596aa1e4e53ba17d9e2f26630d0bd6332e29ef2b0d4c044d4e53561040420f0004424545468403447acfb14643dba1a29f4105527ccac0412ee86baf40f8f1ad23df611b83b8e205617572610101febce836e4bf69ac6e5476086a85d77ea484fa3960ad1591e5b37152876782271e582d3f82d66aacf5c022ebaa0a74ce61be837a25b416efd666988079fc3c80";

    /// The protocol versions at the mainnet runtime upgrade boundary must both be supported:
    /// 22_000 before, 1_000_000 after. Indexer versions without node 1.0 support fail on the
    /// first post-upgrade block with `ProtocolVersionError::Unsupported(1_000_000)`.
    #[test]
    fn test_protocol_version_mainnet_runtime_upgrade() {
        let last_pre_upgrade = decode_header(MAINNET_HEADER_1_774_491);
        let first_post_upgrade = decode_header(MAINNET_HEADER_1_774_492);

        assert_eq!(last_pre_upgrade.number, 1_774_491);
        assert_eq!(first_post_upgrade.number, 1_774_492);
        assert_eq!(
            first_post_upgrade.parent_hash,
            H256(
                const_hex::decode_to_array(
                    "40e3f67939e6f53e9f36b0a19fd27d484d29c008698f3a2336c5f2529a85dd23"
                )
                .expect("valid block hash")
            )
        );

        let version = last_pre_upgrade
            .protocol_version()
            .expect("protocol version of mainnet block 1_774_491 must be supported")
            .expect("mainnet block 1_774_491 must have a MNSV digest");
        assert_eq!(u32::from(version), 22_000);

        let version = first_post_upgrade
            .protocol_version()
            .expect("protocol version of mainnet block 1_774_492 must be supported")
            .expect("mainnet block 1_774_492 must have a MNSV digest");
        assert_eq!(u32::from(version), 1_000_000);
    }

    fn decode_header(header: &str) -> SubstrateHeader<H256> {
        let header = const_hex::decode(header).expect("valid hex");
        SubstrateHeader::decode(&mut header.as_slice()).expect("SCALE decode header")
    }
}
