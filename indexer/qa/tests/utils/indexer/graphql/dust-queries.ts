// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

export const DUST_GENERATION_STATUS_BODY_FRAGMENT = `
  cardanoRewardAddress
  dustAddress
  registered
  nightBalance
  generationRate
  currentCapacity
  maxCapacity
  utxoTxHash
  utxoOutputIndex
`;

export const GET_DUST_GENERATION_STATUS = `
query GetDustGenerationStatus($CARDANO_REWARD_ADDRESSES: [CardanoRewardAddress!]!) {
  dustGenerationStatus(cardanoRewardAddresses: $CARDANO_REWARD_ADDRESSES) {
    ${DUST_GENERATION_STATUS_BODY_FRAGMENT}
  }
}`;

export const DUST_REGISTRATION_BODY_FRAGMENT = `
  dustAddress
  valid
  nightBalance
  generationRate
  currentCapacity
  maxCapacity
  utxoTxHash
  utxoOutputIndex
`;

export const GET_DUST_GENERATIONS = `
query GetDustGenerations($CARDANO_REWARD_ADDRESSES: [CardanoRewardAddress!]!) {
  dustGenerations(cardanoRewardAddresses: $CARDANO_REWARD_ADDRESSES) {
    cardanoRewardAddress
    registrations {
      ${DUST_REGISTRATION_BODY_FRAGMENT}
    }
  }
}`;

export const GET_DUST_COMMITMENT_MERKLE_TREE_UPDATE = `
query GetDustCommitmentMerkleTreeUpdate($START_INDEX: Int!, $END_INDEX: Int!) {
  dustCommitmentMerkleTreeUpdate(startIndex: $START_INDEX, endIndex: $END_INDEX) {
    startIndex
    endIndex
    update
    protocolVersion
  }
}`;

export const GET_DUST_GENERATION_MERKLE_TREE_UPDATE = `
query GetDustGenerationMerkleTreeUpdate($START_INDEX: Int!, $END_INDEX: Int!) {
  dustGenerationMerkleTreeUpdate(startIndex: $START_INDEX, endIndex: $END_INDEX) {
    startIndex
    endIndex
    update
    protocolVersion
  }
}`;
