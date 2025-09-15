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

export const TRANSACTION_BODY_FRAGMENT = `   id
    hash
    protocolVersion
    merkleTreeRoot
        identifiers
    fees {
      paidFees
      estimatedFees
    }
    block {
      hash
      height
    }
    transactionResult {
      ${TRANSACTION_RESULT_BODY_FRAGMENT}
    }
    unshieldedCreatedOutputs {
      ${UNSHIELDED_UTXO_BODY_FRAGMENT}
    }
    unshieldedSpentOutputs {
      ${UNSHIELDED_UTXO_BODY_FRAGMENT}
    }`;

export const GET_TRANSACTION_BY_OFFSET = `query GetTransactionByOffset($OFFSET: TransactionOffset!){
  transactions(offset: $OFFSET){
    ${TRANSACTION_BODY_FRAGMENT}
  }
}`;
