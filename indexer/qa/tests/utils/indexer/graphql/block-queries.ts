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

import { TRANSACTION_BODY_FRAGMENT } from './transaction-queries';

export const GET_ZSWAP_MERKLE_TREE_COLLAPSED_UPDATE = `
query ZswapMerkleTreeCollapsedUpdate($START_INDEX: Int!, $END_INDEX: Int!) {
  zswapMerkleTreeCollapsedUpdate(startIndex: $START_INDEX, endIndex: $END_INDEX) {
    startIndex
    endIndex
    update
    protocolVersion
  }
}`;

export const GET_LATEST_BLOCK = `
query GetLatestBlock{
  block{
    hash
    protocolVersion
    height
    timestamp
    author
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
    transactions{
      ${TRANSACTION_BODY_FRAGMENT}
    }
  }
}`;

// #1304: the contract's global zswap commitment tree filtered to one contract,
// resolved from a block's ledger state (nullable; null when the contract does
// not exist as of that block). Latest block when OFFSET is omitted.
export const GET_BLOCK_CONTRACT_ZSWAP_STATE = `
query BlockContractZswapState($ADDRESS: HexEncoded!, $OFFSET: BlockOffset) {
  block(offset: $OFFSET) {
    hash
    height
    contractZswapState(address: $ADDRESS)
  }
}`;

// The composed cross-contract-calls (CCC) execution-inputs read documented on
// the #1304 field: contract state, its zswap state and the ledger parameters,
// all anchored to one block.
export const GET_EXECUTION_INPUTS = `
query ExecutionInputs($ADDRESS: HexEncoded!) {
  block {
    hash
    ledgerParameters
    contractZswapState(address: $ADDRESS)
  }
  contract(address: $ADDRESS) {
    state
  }
}`;

export const GET_BLOCK_BY_OFFSET = `
query GetBlock($OFFSET: BlockOffset!){
  block (offset: $OFFSET){
    hash
    protocolVersion
    height
    timestamp
    author
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
    transactions{
      ${TRANSACTION_BODY_FRAGMENT}
    }
  }
}`;
