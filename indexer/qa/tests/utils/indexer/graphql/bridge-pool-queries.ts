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

// GraphQL documents for the c2m-bridge pool observability surface (#944):
// bridgePoolSummary (optionally at a block), and the reserve/treasury inflow
// event lists. Only the BridgeReserveTransfer inline fragment is expanded on
// the inflow lists; the treasury variants are discriminated by __typename until
// data exists to exercise their fields.
export const GET_BRIDGE_POOL_SUMMARY = `
query BridgePoolSummary($AT_BLOCK: Int) {
  bridgePoolSummary(atBlock: $AT_BLOCK) {
    reserveTotal
    treasuryByReason {
      reason
      total
    }
    subminimumTxCount
    lastEventBlockHeight
  }
}`;

export const GET_BRIDGE_RESERVE_INFLOWS = `
query BridgeReserveInflows($BLOCK_HEIGHT_FROM: Int, $BLOCK_HEIGHT_TO: Int, $OFFSET: Int, $LIMIT: Int) {
  bridgeReserveInflows(blockHeightFrom: $BLOCK_HEIGHT_FROM, blockHeightTo: $BLOCK_HEIGHT_TO, offset: $OFFSET, limit: $LIMIT) {
    __typename
    ... on BridgeReserveTransfer {
      id
      blockHeight
      midnightTxHash
      cardanoTxHash
      amount
    }
  }
}`;

export const GET_BRIDGE_TREASURY_INFLOWS = `
query BridgeTreasuryInflows($REASON: BridgeTreasuryReason, $BLOCK_HEIGHT_FROM: Int, $BLOCK_HEIGHT_TO: Int, $OFFSET: Int, $LIMIT: Int) {
  bridgeTreasuryInflows(reason: $REASON, blockHeightFrom: $BLOCK_HEIGHT_FROM, blockHeightTo: $BLOCK_HEIGHT_TO, offset: $OFFSET, limit: $LIMIT) {
    __typename
  }
}`;
