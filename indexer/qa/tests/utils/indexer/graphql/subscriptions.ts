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

export const SHIELDED_TRANSACTION_SUBSCRIPTION_BY_SESSION_ID = `subscription WalletSyncEventSubscription ($SESSION_ID: String){
    shieldedTransactions (sessionId: $SESSION_ID) {
        ... on RelevantTransaction {
            __typename
            transaction {
                hash
            }
            zswapCollapsedUpdate {
                startIndex
                endIndex
                update
                protocolVersion
            }
        }
        ... on ShieldedTransactionsProgress {
            __typename
            highestZswapEndIndex
            highestCheckedZswapEndIndex
            highestRelevantZswapEndIndex
        }
    }
}`;

const UNSHIELDED_TX_SUBSCRIPTION_FRAGMENT = `    ... on UnshieldedTransaction {
        __typename
        transaction{
          id
          hash
          ... on RegularTransaction {
            identifiers
          }
        }
        createdUtxos{
          owner
          intentHash
          value
          tokenType
          outputIndex
          ctime
          initialNonce
          registeredForDustGeneration
          createdAtTransaction{
              hash
              ... on RegularTransaction {
                identifiers
              }
          }
          spentAtTransaction{
              hash
              ... on RegularTransaction {
                identifiers
              }
          }
        }
        spentUtxos{
          owner
          intentHash
          value
          tokenType
          outputIndex
          ctime
          initialNonce
          registeredForDustGeneration
          createdAtTransaction{
              hash
              ... on RegularTransaction {
                identifiers
              }
          }
          spentAtTransaction{
              hash
              ... on RegularTransaction {
                identifiers
              }
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
   blocks {
    hash
    height
    timestamp
    protocolVersion
    ledgerParameters
    zswapMerkleTreeRoot
    dustCommitmentMerkleTreeRoot
    dustGenerationMerkleTreeRoot
    zswapEndIndex
    dustCommitmentEndIndex
    dustGenerationEndIndex
    parent {
      hash
      height
    }
    transactions {
      id
      hash
      __typename
      protocolVersion
      raw
      ... on RegularTransaction {
    identifiers
  }
      unshieldedCreatedOutputs {
        owner
        intentHash
        value
        tokenType
        outputIndex
      }
    }
}
}`;

export const BLOCKS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET = `subscription BlocksSubscriptionFromBlockByOffset($OFFSET: BlockOffset) {
      blocks(offset: $OFFSET) {
    hash
    height
    timestamp
    protocolVersion
    ledgerParameters
    zswapMerkleTreeRoot
    dustCommitmentMerkleTreeRoot
    dustGenerationMerkleTreeRoot
    zswapEndIndex
    dustCommitmentEndIndex
    dustGenerationEndIndex
    parent {
      hash
      height
    }
    transactions {
      id
      hash
      __typename
      protocolVersion
      raw
      block {
        hash
        height
      }
      contractActions {
        address
        state
        zswapState
      }
      unshieldedCreatedOutputs {
        owner
        intentHash
        value
        tokenType
        outputIndex
        ctime
        initialNonce
        registeredForDustGeneration
        createdAtTransaction { hash }
        spentAtTransaction { hash }
      }
      unshieldedSpentOutputs {
        owner
        intentHash
        value
        tokenType
        outputIndex
        ctime
        initialNonce
        registeredForDustGeneration
        createdAtTransaction { hash }
        spentAtTransaction { hash }
      }
      zswapLedgerEvents {
        id
        raw
        maxId
        protocolVersion
      }
      dustLedgerEvents {
        id
        raw
        maxId
        protocolVersion
      }
      ... on RegularTransaction {
        zswapMerkleTreeRoot
        identifiers
        zswapStartIndex
        zswapEndIndex
        fees {
          paidFees
          estimatedFees
        }
        transactionResult {
          status
          segments {
            id
            success
          }
        }
      }
    }
  }
}`;

const CONTRACT_ACTION_SUBSCRIPTION_FRAGMENT = `
    __typename
    address
    ... on ContractDeploy {
        state
        zswapState
        transaction {
            hash
        }
        unshieldedBalances {
            tokenType
            amount
        }
    }
    ... on ContractCall {
        state
        zswapState
        transaction {
            hash
        }
        entryPoint
        deploy {
            address
            unshieldedBalances {
                tokenType
                amount
            }
        }
        unshieldedBalances {
            tokenType
            amount
        }
    }
    ... on ContractUpdate {
        state
        zswapState
        transaction {
            hash
        }
        unshieldedBalances {
            tokenType
            amount
        }
    }
`;

export const CONTRACT_ACTIONS_SUBSCRIPTION_FROM_LATEST_BLOCK = `subscription ContractActionsSubscriptionFromLatestBlock($ADDRESS: HexEncoded!) {
    contractActions(address: $ADDRESS) {
        ${CONTRACT_ACTION_SUBSCRIPTION_FRAGMENT}
    }
}`;

export const CONTRACT_ACTIONS_SUBSCRIPTION_FROM_BLOCK_BY_OFFSET = `subscription ContractActionsSubscriptionFromBlockByOffset($ADDRESS: HexEncoded!, $OFFSET: BlockOffset) {
    contractActions(address: $ADDRESS, offset: $OFFSET) {
        ${CONTRACT_ACTION_SUBSCRIPTION_FRAGMENT}
    }
}`;

export const DUST_LEDGER_EVENTS_SUBSCRIPTION_DEFAULT = `
  subscription DustLedgerEvents {
    dustLedgerEvents {
      __typename
      id
      raw
      maxId
      protocolVersion
      ... on DustInitialUtxo {
        output {
          nonce
        }
      }
    }
  }
`;

export const DUST_LEDGER_EVENTS_SUBSCRIPTION_FROM_ID = `
  subscription DustLedgerEvents($id: Int) {
    dustLedgerEvents(id: $id) {
      __typename
      id
      raw
      maxId
      protocolVersion
      ... on DustInitialUtxo {
        output {
          nonce
        }
      }
    }
  }
`;

export const ZSWAP_LEDGER_EVENTS_SUBSCRIPTION_DEFAULT = `
  subscription ZswapEvents {
    zswapLedgerEvents {
      id
      raw
      maxId
      protocolVersion
    }
  }
`;

export const ZSWAP_LEDGER_EVENTS_SUBSCRIPTION_FROM_ID = `
  subscription ZswapEvents($id: Int) {
    zswapLedgerEvents(id: $id) {
      id
      raw
      maxId
      protocolVersion
    }
  }
`;

export const DUST_GENERATIONS_SUBSCRIPTION = `
  subscription DustGenerations($dustAddress: DustAddress!, $blockHash: HexEncoded!, $dtimeCutoffHeight: Int!) {
    dustGenerations(dustAddress: $dustAddress, blockHash: $blockHash, dtimeCutoffHeight: $dtimeCutoffHeight) {
      ... on DustGenerationsItem {
        __typename
        commitmentMtIndex
        generationMtIndex
        owner
        value
        initialValue
        backingNight
        ctime
        transactionId
        transactionHash
        collapsedMerkleTree {
          startIndex
          endIndex
          update
          protocolVersion
        }
      }
      ... on DustGenerationsProgress {
        __typename
        highestIndex
        collapsedMerkleTree {
          startIndex
          endIndex
          update
          protocolVersion
        }
      }
      ... on DustGenerationDtimeUpdateItem {
        __typename
        generationMtIndex
        owner
        nightUtxoHash
        newDtime
        transactionId
        transactionHash
        treeInsertionPath
      }
    }
  }
`;

export const DUST_NULLIFIER_TRANSACTIONS_SUBSCRIPTION = `
  subscription DustNullifierTransactions($nullifierLeBytesPrefixes: [HexEncoded!]!, $fromBlock: Int, $toBlock: Int) {
    dustNullifierTransactions(nullifierLeBytesPrefixes: $nullifierLeBytesPrefixes, fromBlock: $fromBlock, toBlock: $toBlock) {
      nullifierLeBytes
      commitmentLeBytes
      transactionId
      transactionHash
      blockHeight
      blockHash
      transaction {
        hash
      }
    }
  }
`;

export const SHIELDED_NULLIFIER_TRANSACTIONS_SUBSCRIPTION = `
  subscription ShieldedNullifierTransactions($nullifierPrefixes: [HexEncoded!]!, $fromBlock: Int, $toBlock: Int) {
    shieldedNullifierTransactions(nullifierPrefixes: $nullifierPrefixes, fromBlock: $fromBlock, toBlock: $toBlock) {
      transactionId
      transactionHash
      blockHash
      blockHeight
      nullifier
      transaction {
        hash
      }
    }
  }
`;

// c2m-bridge event stream (#942). `from` is an event-id cursor: the subscription
// replays matching historical events with id > from, then live-tails. Omitting
// `from` streams from the beginning. There is no completion sentinel.
export const BRIDGE_EVENTS_SUBSCRIPTION_DEFAULT = `
  subscription BridgeEvents($RECIPIENT: HexEncoded, $VARIANT: BridgeEventVariant) {
    bridgeEvents(recipient: $RECIPIENT, variant: $VARIANT) {
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
  }
`;

export const BRIDGE_EVENTS_SUBSCRIPTION_FROM = `
  subscription BridgeEventsFrom($FROM: Int, $RECIPIENT: HexEncoded, $VARIANT: BridgeEventVariant) {
    bridgeEvents(from: $FROM, recipient: $RECIPIENT, variant: $VARIANT) {
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
  }
`;

// bridgeBalance emits the current balance immediately on connect, then re-emits
// on every relevant event for the address.
export const BRIDGE_BALANCE_SUBSCRIPTION = `
  subscription BridgeBalance($ADDRESS: HexEncoded!) {
    bridgeBalance(address: $ADDRESS) {
      deposited
      claimed
      balance
    }
  }
`;

// c2m-bridge pool observability stream (#944). Emits an initial snapshot on
// subscribe (newEvent = null), then a refreshed summary paired with each new
// pool-affecting event.
export const BRIDGE_POOL_UPDATES_SUBSCRIPTION = `
  subscription BridgePoolUpdates {
    bridgePoolUpdates {
      newEvent {
        __typename
      }
      pool {
        reserveTotal
        treasuryByReason {
          reason
          total
        }
        subminimumTxCount
        lastEventBlockHeight
      }
    }
  }
`;
