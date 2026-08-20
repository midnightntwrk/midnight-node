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

use core::fmt;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SidechainStatusResponse {
    pub mainchain: ChainInfo,
    pub sidechain: ChainInfo,
}

impl Display for SidechainStatusResponse {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(f, "Sidechain Status:")?;
        writeln!(f, "  Mainchain:")?;
        writeln!(f, "{}", self.mainchain)?;
        writeln!(f, "  Sidechain:")?;
        writeln!(f, "{}", self.sidechain)
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ChainInfo {
    pub epoch: u32,
    pub next_epoch_timestamp: i64,
    pub slot: u32,
}

impl Display for ChainInfo {
    fn fmt(&self, f: &mut Formatter) -> std::fmt::Result {
        writeln!(f, "    Epoch: {}", self.epoch)?;
        writeln!(f, "    Next Epoch Timestamp: {}", self.next_epoch_timestamp)?;
        writeln!(f, "    Slot: {}", self.slot)
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SPORegistrationResponse {
    pub d_parameter: DParameter,
    pub permissioned_candidates: serde_json::Value,
    pub candidate_registrations: HashMap<String, Vec<CandidateRegistration>>,
}

impl Display for SPORegistrationResponse {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(f, "SPO Registration Response:")?;
        writeln!(f, "  DParameter:")?;
        writeln!(f, "{}", self.d_parameter)?;
        writeln!(
            f,
            "  Permissioned Candidates: {:?}",
            self.permissioned_candidates
        )?;
        writeln!(f, "  Candidate Registrations:")?;
        for (key, registrations) in &self.candidate_registrations {
            writeln!(f, "    {}:", key)?;
            for reg in registrations {
                writeln!(f, "{}", reg)?;
            }
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct DParameter {
    pub num_permissioned_candidates: u32,
    pub num_registered_candidates: u32,
}

impl Display for DParameter {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(
            f,
            "    Num Permissioned Candidates: {}",
            self.num_permissioned_candidates
        )?;
        writeln!(
            f,
            "    Num Registered Candidates: {}",
            self.num_registered_candidates
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CandidateRegistration {
    pub sidechain_pub_key: String,
    pub sidechain_account_id: String,
    pub mainchain_pub_key: String,
    pub cross_chain_pub_key: String,
    pub keys: CandidateKeys,
    pub sidechain_signature: String,
    pub mainchain_signature: String,
    pub cross_chain_signature: String,
    pub utxo: Utxo,
    pub is_valid: bool,
    pub invalid_reasons: Option<InvalidReasons>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CandidateKeys {
    pub gran: String,
    pub aura: String,
}

impl Display for CandidateRegistration {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(f, "      Sidechain Pub Key: {}", self.sidechain_pub_key)?;
        writeln!(
            f,
            "      Sidechain Account ID: {}",
            self.sidechain_account_id
        )?;
        writeln!(f, "      Mainchain Pub Key: {}", self.mainchain_pub_key)?;
        writeln!(f, "      Cross Chain Pub Key: {}", self.cross_chain_pub_key)?;
        writeln!(f, "      Aura Pub Key: {}", self.keys.aura)?;
        writeln!(f, "      Grandpa Pub Key: {}", self.keys.gran)?;
        writeln!(f, "      Sidechain Signature: {}", self.sidechain_signature)?;
        writeln!(f, "      Mainchain Signature: {}", self.mainchain_signature)?;
        writeln!(
            f,
            "      Cross Chain Signature: {}",
            self.cross_chain_signature
        )?;
        writeln!(f, "      UTXO:")?;
        writeln!(f, "{}", self.utxo)?;
        writeln!(f, "      Is Valid: {}", self.is_valid)?;

        if let Some(reasons) = &self.invalid_reasons {
            writeln!(f, "      Invalid Reasons: {}", reasons)?;
        }

        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Utxo {
    pub utxo_id: String,
    pub epoch_number: u32,
    pub block_number: u32,
    pub slot_number: u64,
    pub tx_index_within_block: u32,
}

impl Display for Utxo {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        writeln!(f, "        UTXO ID: {}", self.utxo_id)?;
        writeln!(f, "        Epoch Number: {}", self.epoch_number)?;
        writeln!(f, "        Block Number: {}", self.block_number)?;
        writeln!(f, "        Slot Number: {}", self.slot_number)?;
        writeln!(
            f,
            "        Tx Index Within Block: {}",
            self.tx_index_within_block
        )
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum InvalidReasons {
    StakeError {
        #[serde(rename = "StakeError")]
        stake_error: String,
    },
}

impl Display for InvalidReasons {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            InvalidReasons::StakeError { stake_error } => {
                write!(f, "Stake Error: {}", stake_error)
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct EpochCommitteeResponse {
    pub sidechain_epoch: u64,
    pub committee: Vec<CommitteeMember>,
}

impl Display for EpochCommitteeResponse {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        writeln!(f, "Epoch: {}", self.sidechain_epoch)?;
        writeln!(f, "Committee Members:")?;
        for (i, member) in self.committee.iter().enumerate() {
            writeln!(f, "  {}: {}", i + 1, member.sidechain_pub_key)?;
        }
        Ok(())
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommitteeMember {
    pub sidechain_pub_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockResponse {
    pub block: Block,

    pub justifications: Vec<Vec<Vec<u8>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub header: Header,

    pub extrinsics: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Header {
    pub parent_hash: String,
    pub number: String,
    pub state_root: String,
    pub extrinsics_root: String,
    pub digest: Digest,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Digest {
    pub logs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BodyItem {
    Timestamp(TimestampExtrinsic),

    UnknownTransaction(String),

    Object(HashMap<String, serde_json::Value>),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimestampExtrinsic {
    #[serde(rename = "Timestamp")]
    pub timestamp_ms: u64,
}
