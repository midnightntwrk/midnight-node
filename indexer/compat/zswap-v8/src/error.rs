// This file is part of midnight-ledger.
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

use crate::structure::CoinCiphertext;
use coin_structure::coin::{Commitment, Nullifier};
use coin_structure::contract::ContractAddress;
use std::error::Error;
use std::fmt::{self, Display, Formatter};
use transient_crypto::merkle_tree::{InvalidIndex, InvalidUpdate, MerkleTreeDigest};
use transient_crypto::proofs::{ProvingError, VerifyingError};

#[derive(Debug, Clone, Copy)]
pub enum TransactionInvalid {
    NullifierAlreadyPresent(Nullifier),
    CommitmentAlreadyPresent(Commitment),
    UnknownMerkleRoot(MerkleTreeDigest),
    MerkleTreeError(InvalidUpdate),
}

impl Display for TransactionInvalid {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use TransactionInvalid::*;
        match self {
            NullifierAlreadyPresent(nul) => {
                write!(formatter, "double-spend attempt with nullifier {:?}", nul)
            }
            CommitmentAlreadyPresent(cm) => {
                write!(formatter, "faerie-gold attempt with commitment {:?}", cm)
            }
            UnknownMerkleRoot(root) => {
                write!(formatter, "use of unknown coin tree root {:?}", root)
            }
            MerkleTreeError(msg) => {
                write!(formatter, "merkle tree error {:?}", msg)
            }
        }
    }
}

impl Error for TransactionInvalid {}

#[derive(Debug)]
pub enum MalformedOffer {
    InvalidProof(VerifyingError),
    ContractSentCiphertext {
        address: ContractAddress,
        ciphertext: Box<CoinCiphertext>,
    },
    NonDisjointCoinMerge,
    NotNormalized,
}

impl Display for MalformedOffer {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use MalformedOffer::*;
        match self {
            InvalidProof(err) => {
                err.fmt(formatter)?;
                write!(formatter, " -- while verifying Zswap proof")
            }
            ContractSentCiphertext { address, .. } => write!(
                formatter,
                "contract {:?} was sent an output with a ciphertext",
                address
            ),
            NonDisjointCoinMerge => write!(formatter, "attempted to merge non-disjoint coin sets"),
            NotNormalized => write!(formatter, "offer is not in normal form"),
        }
    }
}

impl Error for MalformedOffer {}

#[derive(Debug)]
pub enum OfferCreationFailed {
    InvalidIndex(InvalidIndex),
    Proving(ProvingError),
    NotContractOwned,
    TreeNotRehashed,
    MerkleTreeError(InvalidUpdate),
}

impl Display for OfferCreationFailed {
    fn fmt(&self, formatter: &mut Formatter) -> fmt::Result {
        use OfferCreationFailed::*;
        match self {
            InvalidIndex(err) => write!(formatter, "{} -- write creating spend proof", err),
            Proving(err) => write!(formatter, "{} -- write creating Zswap proof", err),
            NotContractOwned => write!(
                formatter,
                "attempted to spend a user-owned output as contract owned"
            ),
            TreeNotRehashed => write!(
                formatter,
                "attempted to spend from a Merkle tree that was not rehashed"
            ),
            MerkleTreeError(msg) => write!(formatter, "merkle tree error: {msg}"),
        }
    }
}

impl Error for OfferCreationFailed {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            OfferCreationFailed::InvalidIndex(i) => Some(i),
            // Note that anyhow errors *don't* work here.
            _ => None,
        }
    }
}
