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

// Selection set shared by the contractEvents query and subscription. ContractEvent
// is a polymorphic interface; clients discriminate on `__typename` and read the
// per-variant payload fields via inline fragments. PausedEvent / UnpausedEvent carry
// no payload beyond the interface fields, so they need no fragment.
const CONTRACT_EVENT_SELECTION = `
    __typename
    id
    raw
    maxId
    protocolVersion
    version
    contractAddress
    transactionId
    transaction {
      hash
    }
    ... on ShieldedSpendEvent {
      nullifier
    }
    ... on ShieldedReceiveEvent {
      commitment
      ciphertext
      receivingContractAddress
    }
    ... on ShieldedMintEvent {
      commitment
      domainSep
      amount
    }
    ... on ShieldedBurnEvent {
      nullifier
      amount
    }
    ... on UnshieldedSpendEvent {
      sender {
        kind
        userAddress
        contractAddress
      }
      domainSep
      tokenType
      amount
    }
    ... on UnshieldedReceiveEvent {
      recipient {
        kind
        userAddress
        contractAddress
      }
      domainSep
      tokenType
      amount
    }
    ... on UnshieldedMintEvent {
      domainSep
      tokenType
      amount
    }
    ... on UnshieldedBurnEvent {
      sender {
        kind
        userAddress
        contractAddress
      }
      tokenType
      amount
    }
    ... on MiscContractEvent {
      name
      payload
    }
`;

export const GET_CONTRACT_EVENTS = `
  query ContractEvents($FILTER: ContractEventFilter!, $LIMIT: Int, $OFFSET: Int) {
    contractEvents(filter: $FILTER, limit: $LIMIT, offset: $OFFSET) {
${CONTRACT_EVENT_SELECTION}
    }
  }
`;

export const CONTRACT_EVENTS_SUBSCRIPTION = `
  subscription ContractEventsSubscription($FILTER: ContractEventFilter!, $ID: Int) {
    contractEvents(filter: $FILTER, id: $ID) {
${CONTRACT_EVENT_SELECTION}
    }
  }
`;
