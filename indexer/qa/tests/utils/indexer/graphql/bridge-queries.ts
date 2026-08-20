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

// GraphQL documents for the c2m-bridge query surface (#941). The BridgeUserTransfer
// inline fragment is the only concrete variant with test data on the current
// environments; the other variants are selected by __typename only until data
// exists to exercise their fields.
export const GET_BRIDGE_EVENTS = `
query BridgeEvents($RECIPIENT: HexEncoded, $VARIANT: BridgeEventVariant, $BLOCK_HEIGHT_FROM: Int, $BLOCK_HEIGHT_TO: Int, $OFFSET: Int, $LIMIT: Int) {
  bridgeEvents(recipient: $RECIPIENT, variant: $VARIANT, blockHeightFrom: $BLOCK_HEIGHT_FROM, blockHeightTo: $BLOCK_HEIGHT_TO, offset: $OFFSET, limit: $LIMIT) {
    __typename
    ... on BridgeUserTransfer {
      id
      blockHeight
      midnightTxHash
      cardanoTxHash
      amount
      recipient
    }
  }
}`;

export const GET_BRIDGE_BALANCE = `
query BridgeBalance($ADDRESS: HexEncoded!) {
  bridgeBalance(address: $ADDRESS) {
    deposited
    claimed
    balance
  }
}`;

export const GET_BRIDGE_DEPOSITS = `
query BridgeDeposits($RECIPIENT: HexEncoded!, $INCLUDE_UNAPPROVED: Boolean, $OFFSET: Int, $LIMIT: Int) {
  bridgeDeposits(recipient: $RECIPIENT, includeUnapproved: $INCLUDE_UNAPPROVED, offset: $OFFSET, limit: $LIMIT) {
    __typename
    ... on BridgeUserTransfer {
      id
      recipient
      amount
    }
  }
}`;
