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

export const TRANSACTION_RESULT_BODY_FRAGMENT = `     status
      segments {
        id
        success
      }`;

export const UNSHIELDED_UTXO_BODY_FRAGMENT = `     owner
      intentHash
      value
      tokenType
      outputIndex
      createdAtTransaction {
        hash
      }
      spentAtTransaction {
        hash
      }`;

export const SYSTEM_TRANSACTION_FRAGMENT = `   ... on SystemTransaction {
    zswapLedgerEvents {
      id
      raw
      maxId
    }
    dustLedgerEvents {
      id
      raw
      maxId
    }
}`;

export const REGULAR_TRANSACTION_FRAGMENT = `   ... on RegularTransaction {
        merkleTreeRoot
        identifiers
        startIndex
        endIndex
        fees {
            paidFees
            estimatedFees
        }
        transactionResult {
            ${TRANSACTION_RESULT_BODY_FRAGMENT}
        }
        contractActions {
            address
            state
            zswapState
        }
        unshieldedCreatedOutputs {
            ${UNSHIELDED_UTXO_BODY_FRAGMENT}
        }
        unshieldedSpentOutputs {
            ${UNSHIELDED_UTXO_BODY_FRAGMENT}
        }
    }`;

export const TRANSACTION_BODY_FRAGMENT = `                   __typename
                    hash
                    protocolVersion
                    block {
                        hash
                        height
                    }
                    ${REGULAR_TRANSACTION_FRAGMENT}
                    ${SYSTEM_TRANSACTION_FRAGMENT}`;

export function getBlockSubscriptionQuery(
  blockTypeParam: string,
  blockValueParam: string,
) {
  return `subscription BlocksSubscription${blockTypeParam} {
            blocks${blockValueParam} {
                hash
                height
                timestamp
                transactions {
                    ${TRANSACTION_BODY_FRAGMENT}
                }
            }
        }`;
}
