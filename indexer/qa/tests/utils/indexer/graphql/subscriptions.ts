// This file is part of midnightntwrk/midnight-indexer
// Copyright (C) 2025 Midnight Foundation
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

export const SHIELDED_TRANSACTION_SUBSCRIPTION_BY_SESSION_ID = `subscription WalletSyncEventSubscription ($SESSION_ID: String){
    shieldedTransactions (sessionId: $SESSION_ID) {
        ... on ViewingUpdate {
            __typename
            index
            update {
                ... on MerkleTreeCollapsedUpdate {
                __typename
                start
                end
                update
                }
                ... on RelevantTransaction {
                __typename
                transaction {
                    hash
                }
                start
                end
                }
            }
        }
        ... on ShieldedTransactionsProgress {
            __typename
            highestIndex
            highestRelevantIndex
            highestRelevantWalletIndex
        }
    }
}`;

const UNSHIELDED_TX_SUBSCRIPTION_FRAGMENT = `    ... on UnshieldedTransaction {
        __typename
        transaction{
          id
          hash
        }
        createdUtxos{
          owner
          intentHash
          value
          tokenType
          outputIndex
          createdAtTransaction{
              hash
              identifiers
          }
          spentAtTransaction{
              hash
              identifiers
          }
        }
        spentUtxos{
          owner
          intentHash
          value
          tokenType
          outputIndex
          createdAtTransaction{
              hash
              identifiers
          }
          spentAtTransaction{
              hash
              identifiers
          }
        }
      }
      ... on UnshieldedTransactionsProgress {
        __typename
        highestTransactionId
      }`;

export const UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS_AND_TRANSACTION_ID = `subscription UnshieldedTxSubscription($ADDRESS: UnshieldedAddress, $TRANSACTION_ID: Int) {
    unshieldedTransactions(address: $ADDRESS, transactionId: $TRANSACTION_ID) {
        ${UNSHIELDED_TX_SUBSCRIPTION_FRAGMENT}
    }
}`;

export const UNSHIELDED_TX_SUBSCRIPTION_BY_ADDRESS = `subscription UnshieldedTxSubscription($ADDRESS: UnshieldedAddress) {
    unshieldedTransactions(address: $ADDRESS) {
        ${UNSHIELDED_TX_SUBSCRIPTION_FRAGMENT}
    }
}`;

export const BLOCKS_SUBSCRIPTION_FROM_LATEST_BLOCK = `subscription BlocksSubscriptionFromLatestBlock {
    blocks{
        hash
        height
        timestamp
    }
}`;

export const BLOCKS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET = `subscription BlocksSubscriptionFromBlockByOffset($OFFSET: BlockOffset) {
    blocks(offset: $OFFSET) {
        hash
        height
        timestamp
    }
}`;
