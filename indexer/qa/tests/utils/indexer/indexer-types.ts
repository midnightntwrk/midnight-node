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

import { GraphQLError } from 'graphql';

export type GraphQLResponse<T> = {
  data: T | null;
  errors?: GraphQLError[];
};

export type BlockResponse = GraphQLResponse<{ block: Block }>;

export type TransactionResponse = GraphQLResponse<{ transactions: Transaction[] }>;

export type UnshieldedUtxoResponse = GraphQLResponse<{ unshieldedUtxos: UnshieldedUtxo[] }>;

export type BlockOffset = {
  hash?: string;
  height?: number;
};

export type TransactionOffset = {
  hash?: string;
  identifier?: string;
};

export type UnshieldedAddress = string;

export interface Block {
  hash: string;
  height: number;
  timestamp: string;
  protocolVersion: number;
  parent: Block;
  transactions: Transaction[];
}

export interface UnshieldedUtxo {
  owner: string;
  intentHash: string;
  value: string;
  tokenType: string;
  outputIndex: number;
  createdAtTransaction: Transaction;
  spentAtTransaction: Transaction;
}

export type TransactionResult = {
  status: TransactionResultStatus;
  segments: Segment;
};

export enum TransactionResultStatus {
  SUCCESS = 'SUCCESS',
  PARTIAL_SUCCESS = 'PARTIAL_SUCCESS',
  FAILURE = 'FAILURE',
}

export interface Segment {
  id: number;
  success: boolean;
}

export interface TransactionFees {
  paidFees: string;
  estimatedFees: string;
}

export interface Transaction {
  hash: string;
  id: number;
  identifiers?: string[];
  block?: Block;
  raw?: string;
  protocolVersion?: number;
  transactionResult?: TransactionResult;
  fees?: TransactionFees;
  merkleTreeRoot?: string;
  unshieldedCreatedOutputs?: UnshieldedUtxo[];
  unshieldedSpentOutputs?: UnshieldedUtxo[];
}

export type ShieldedTransactionsEvent = ViewingUpdate | ShieldedTransactionsProgress;

export interface ViewingUpdate {
  __typename: 'ViewingUpdate';
  index: number;
  update: ZswapChainStateUpdate[];
}

export type ZswapChainStateUpdate = MerkleTreeCollapsedUpdate | RelevantTransaction;

export interface MerkleTreeCollapsedUpdate {
  __typename: 'MerkleTreeCollapsedUpdate';
  start: number;
  end: number;
  update: string;
  protocolVersion: number;
}

export interface RelevantTransaction {
  __typename: 'RelevantTransaction';
  transaction: Transaction;
  start: number;
  end: number;
}

export interface ShieldedTransactionsProgress {
  __typename: 'ShieldedTransactionsProgress';
  highestIndex: number;
  highestRelevantIndex: number;
  highestRelevantWalletIndex: number;
}

export type UnshieldedTransactionEvent = UnshieldedTransaction | UnshieldedTransactionsProgress;

export interface UnshieldedTransactionsProgress {
  __typename: 'UnshieldedTransactionsProgress';
  highestTransactionId: number;
}

export interface UnshieldedTransaction {
  __typename: 'UnshieldedTransaction';
  transaction: Transaction;
  createdUtxos: UnshieldedUtxo[];
  spentUtxos: UnshieldedUtxo[];
}

export type ViewingKey = string & { __brand: 'ViewingKey' };
