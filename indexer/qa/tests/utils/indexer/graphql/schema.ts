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

import { bech32 } from 'bech32';
import { z } from 'zod';

export const Hash64 = z
  .string()
  .length(64)
  .regex(/^[a-f0-9]+$/);
export const VarLenghtHex = z.string().regex(/^[a-f0-9]+$/);
export const BlockHeight = z.number().min(0);

export const PartialBlockSchema = z.lazy(() =>
  z.object({
    hash: Hash64,
    height: BlockHeight,
  }),
);

export const BlockSchema = z.lazy(() =>
  z.object({
    hash: Hash64,
    height: BlockHeight,
    timestamp: z.number(),
    protocolVersion: z.number(),
    author: z.string().optional(),
    ledgerParameters: z.string(),
    zswapMerkleTreeRoot: VarLenghtHex,
    dustCommitmentMerkleTreeRoot: VarLenghtHex.nullable(),
    dustGenerationMerkleTreeRoot: VarLenghtHex.nullable(),
    zswapEndIndex: z.number().int().nonnegative(),
    dustCommitmentEndIndex: z.number().int().nonnegative(),
    dustGenerationEndIndex: z.number().int().nonnegative(),
    parent: PartialBlockSchema,
    transactions: z.array(FullTransactionSchema).min(0),
  }),
);

export const UnshieldedUtxoSchema = z.object({
  owner: z.string().regex(/^mn_addr_/),
  intentHash: Hash64,
  value: z.string(),
  tokenType: z
    .string()
    .length(64)
    .regex(/^[a-f0-9]+$/),
  outputIndex: z.number(),
  ctime: z.number().nullable(),
  initialNonce: z.string(),
  registeredForDustGeneration: z.boolean(),
  createdAtTransaction: z.object({
    hash: Hash64,
    identifiers: z.array(z.string()).optional(),
  }),
  spentAtTransaction: z
    .object({
      hash: Hash64,
      identifiers: z.array(z.string()).optional(),
    })
    .nullable(),
});

// Zswap Merkle tree collapsed update schema
export const MerkleTreeCollapsedUpdateSchema = z.object({
  startIndex: z.number(),
  endIndex: z.number(),
  update: VarLenghtHex,
  protocolVersion: z.number(),
});

// Ledger event schemas
export const ZswapLedgerEventSchema = z.object({
  id: z.number(),
  raw: z.string(),
  maxId: z.number(),
  protocolVersion: z.number(),
});

export const DustLedgerEventSchema = z.object({
  id: z.number(),
  raw: z.string(),
  maxId: z.number(),
  protocolVersion: z.number(),
});

export const DustParamChangeSchema = z.object({
  __typename: z.literal('ParamChange'),
  id: z.number(),
  raw: z.string(),
  maxId: z.number(),
  protocolVersion: z.number(),
});

export const DustInitialUtxoSchema = z.object({
  __typename: z.literal('DustInitialUtxo'),
  id: z.number(),
  raw: z.string(),
  maxId: z.number(),
  protocolVersion: z.number(),
  output: z.object({
    nonce: z
      .string()
      .length(64)
      .regex(/^[a-f0-9]+$/),
  }),
});

export const DustGenerationDtimeUpdateSchema = z.object({
  __typename: z.literal('DustGenerationDtimeUpdate'),
  id: z.number(),
  raw: z.string(),
  maxId: z.number(),
  protocolVersion: z.number(),
});

export const DustSpendProcessedSchema = z.object({
  __typename: z.literal('DustSpendProcessed'),
  id: z.number(),
  raw: z.string(),
  maxId: z.number(),
  protocolVersion: z.number(),
});

export const DustLedgerEventsUnionSchema = z.discriminatedUnion('__typename', [
  DustParamChangeSchema,
  DustInitialUtxoSchema,
  DustGenerationDtimeUpdateSchema,
  DustSpendProcessedSchema,
]);

// Base transaction schema (common to both RegularTransaction and SystemTransaction)
const BaseTransactionFields = {
  id: z.number(),
  hash: Hash64,
  protocolVersion: z.number(),
  raw: VarLenghtHex,
  block: PartialBlockSchema,
  contractActions: z.array(z.any()), // Will be validated separately
  unshieldedCreatedOutputs: z.array(UnshieldedUtxoSchema),
  unshieldedSpentOutputs: z.array(z.any()), // Will be validated separately
  zswapLedgerEvents: z.array(ZswapLedgerEventSchema),
  dustLedgerEvents: z.array(DustLedgerEventSchema),
};

// RegularTransaction schema (includes additional fields)
export const RegularTransactionSchema = z.lazy(() =>
  z.object({
    ...BaseTransactionFields,
    zswapMerkleTreeRoot: z.string().regex(/^[a-f0-9]+$/),
    identifiers: z.array(z.string()),
    zswapStartIndex: z.number(),
    zswapEndIndex: z.number(),
    dustCommitmentStartIndex: z.number(),
    dustCommitmentEndIndex: z.number(),
    dustGenerationStartIndex: z.number(),
    dustGenerationEndIndex: z.number(),
    fees: z.object({
      paidFees: z.string(),
      estimatedFees: z.string(),
    }),
    transactionResult: z.object({
      status: z.enum(['SUCCESS', 'PARTIAL_SUCCESS', 'FAILURE']),
      segments: z
        .array(
          z.object({
            id: z.number(),
            success: z.boolean(),
          }),
        )
        .nullable(),
    }),
  }),
);

// SystemTransaction schema (only base fields)
export const SystemTransactionSchema = z.lazy(() => z.object(BaseTransactionFields));

// Union schema for both transaction types
export const FullTransactionSchema = z.union([RegularTransactionSchema, SystemTransactionSchema]);

// Contract related schema validation
const BaseActionSchema = z.object({
  id: z.string(),
  type: z.enum(['CALL', 'DEPLOY', 'UPDATE']),
  timestamp: z.string(),
});

const ContractCallSchema = BaseActionSchema.extend({
  type: z.literal('CALL'),
  method: z.string(),
  args: z.array(z.string()),
});

const ContractDeploySchema = BaseActionSchema.extend({
  type: z.literal('DEPLOY'),
  code: z.string(),
});

const ContractUpdateSchema = BaseActionSchema.extend({
  type: z.literal('UPDATE'),
  patch: z.string(),
});

export const ContractActionSchema = z.discriminatedUnion('type', [
  ContractCallSchema,
  ContractDeploySchema,
  ContractUpdateSchema,
]);

// Contract balance schema. tokenType is a 32-byte hex string and amount a decimal
// u128 string; enforcing the shape here makes every parse a format assertion.
export const ContractBalanceSchema = z.object({
  tokenType: z.string().regex(/^[0-9a-fA-F]{64}$/),
  amount: z.string().regex(/^[0-9]+$/),
});

// Updated contract action schemas to match current API
export const ContractDeployActionSchema = z.object({
  __typename: z.literal('ContractDeploy'),
  address: Hash64,
  state: VarLenghtHex,
  zswapState: VarLenghtHex,
  transaction: z.any(), // Reference to transaction
  unshieldedBalances: z.array(ContractBalanceSchema),
});

export const ContractCallActionSchema = z.object({
  __typename: z.literal('ContractCall'),
  address: z.string(),
  state: z.string(),
  zswapState: z.string(),
  entryPoint: z.string(),
  transaction: z.any(), // Reference to transaction
  deploy: z.any(), // Reference to deploy
  unshieldedBalances: z.array(ContractBalanceSchema),
});

export const ContractUpdateActionSchema = z.object({
  __typename: z.literal('ContractUpdate'),
  address: z.string(),
  state: z.string(),
  zswapState: z.string(),
  transaction: z.any(), // Reference to transaction
  unshieldedBalances: z.array(ContractBalanceSchema),
});

export const ContractActionUnionSchema = z.discriminatedUnion('__typename', [
  ContractDeployActionSchema,
  ContractCallActionSchema,
  ContractUpdateActionSchema,
]);

// Contract event schemas (MIP-0002 public contract log emission). HexEncoded
// fields are hex strings (optional `0x` prefix); `amount` is a decimal u128
// string. `transaction` is left as `z.any()` like the contract-action schemas
// — its shape is asserted by the transaction schemas, not here.
const HexEncoded = z.string().regex(/^(0x)?[0-9a-fA-F]+$/);
const U128Decimal = z.string().regex(/^[0-9]+$/);

const ContractEventBaseFields = {
  id: z.number(),
  raw: HexEncoded,
  maxId: z.number(),
  protocolVersion: z.number(),
  version: z.number(),
  contractAddress: HexEncoded,
  transactionId: z.number(),
  transaction: z.any(),
};

export const AddressOrContractSchema = z.object({
  kind: z.enum(['USER', 'CONTRACT']),
  userAddress: HexEncoded.nullable().optional(),
  contractAddress: HexEncoded.nullable().optional(),
});

export const ShieldedSpendEventSchema = z.object({
  __typename: z.literal('ShieldedSpendEvent'),
  ...ContractEventBaseFields,
  nullifier: HexEncoded,
});

export const ShieldedReceiveEventSchema = z.object({
  __typename: z.literal('ShieldedReceiveEvent'),
  ...ContractEventBaseFields,
  commitment: HexEncoded,
  ciphertext: HexEncoded.nullable().optional(),
  receivingContractAddress: HexEncoded.nullable().optional(),
});

export const ShieldedMintEventSchema = z.object({
  __typename: z.literal('ShieldedMintEvent'),
  ...ContractEventBaseFields,
  commitment: HexEncoded,
  domainSep: HexEncoded,
  amount: U128Decimal.nullable().optional(),
});

export const ShieldedBurnEventSchema = z.object({
  __typename: z.literal('ShieldedBurnEvent'),
  ...ContractEventBaseFields,
  nullifier: HexEncoded,
  amount: U128Decimal.nullable().optional(),
});

export const UnshieldedSpendEventSchema = z.object({
  __typename: z.literal('UnshieldedSpendEvent'),
  ...ContractEventBaseFields,
  sender: AddressOrContractSchema,
  domainSep: HexEncoded,
  tokenType: HexEncoded,
  amount: U128Decimal,
});

export const UnshieldedReceiveEventSchema = z.object({
  __typename: z.literal('UnshieldedReceiveEvent'),
  ...ContractEventBaseFields,
  recipient: AddressOrContractSchema,
  domainSep: HexEncoded,
  tokenType: HexEncoded,
  amount: U128Decimal,
});

export const UnshieldedMintEventSchema = z.object({
  __typename: z.literal('UnshieldedMintEvent'),
  ...ContractEventBaseFields,
  domainSep: HexEncoded,
  tokenType: HexEncoded,
  amount: U128Decimal,
});

export const UnshieldedBurnEventSchema = z.object({
  __typename: z.literal('UnshieldedBurnEvent'),
  ...ContractEventBaseFields,
  sender: AddressOrContractSchema,
  tokenType: HexEncoded,
  amount: U128Decimal,
});

export const PausedEventSchema = z.object({
  __typename: z.literal('PausedEvent'),
  ...ContractEventBaseFields,
});

export const UnpausedEventSchema = z.object({
  __typename: z.literal('UnpausedEvent'),
  ...ContractEventBaseFields,
});

export const MiscContractEventSchema = z.object({
  __typename: z.literal('MiscContractEvent'),
  ...ContractEventBaseFields,
  name: HexEncoded,
  payload: HexEncoded,
});

export const ContractEventUnionSchema = z.discriminatedUnion('__typename', [
  ShieldedSpendEventSchema,
  ShieldedReceiveEventSchema,
  ShieldedMintEventSchema,
  ShieldedBurnEventSchema,
  UnshieldedSpendEventSchema,
  UnshieldedReceiveEventSchema,
  UnshieldedMintEventSchema,
  UnshieldedBurnEventSchema,
  PausedEventSchema,
  UnpausedEventSchema,
  MiscContractEventSchema,
]);

// DUST Generation Status schema
const isCardanoRewardAddress = (value: string) => {
  try {
    const decoded = bech32.decode(value.toLowerCase());
    return decoded.prefix.length > 0;
  } catch {
    return false;
  }
};

const DustAddressBech32m = z.string().regex(/^mn_dust(_[a-z0-9]+)?1/, {
  message: 'must be a bech32m DustAddress (mn_dust... / mn_dust_<network>...)',
});

export const DustGenerationStatusSchema = z.object({
  cardanoRewardAddress: z
    .string()
    .refine(isCardanoRewardAddress, { message: 'Invalid Cardano reward address format' }),
  dustAddress: DustAddressBech32m.nullable(),
  registered: z.boolean(),
  nightBalance: z.string().regex(/^\d+$/),
  generationRate: z.string().regex(/^\d+$/),
  maxCapacity: z.string().regex(/^\d+$/),
  currentCapacity: z.string().regex(/^\d+$/),
  utxoTxHash: z.string().nullable(),
  utxoOutputIndex: z.number().int().nullable(),
});

// Simplified version used in subscription responses
export const UnshieldedTxEventTransactionSchema = z.object({
  id: z.number(),
  hash: z.string().regex(/^[a-f0-9]+$/),
  identifiers: z.array(z.string()),
});

export const UnshieldedTxEventTransactionRefSchema = z.object({
  hash: z.string().regex(/^[a-f0-9]+$/),
  identifiers: z.array(z.string()),
});

export const UnshieldedTransactionEventSchema = z.object({
  __typename: z.literal('UnshieldedTransaction'),
  transaction: UnshieldedTxEventTransactionSchema,
  createdUtxos: z.array(UnshieldedUtxoSchema),
  spentUtxos: z.array(UnshieldedUtxoSchema),
});

export const UnshieldedTransactionsProgressSchema = z.object({
  __typename: z.literal('UnshieldedTransactionsProgress'),
  highestTransactionId: z.number(),
});

export const UnshieldedTxSubscriptionResponseSchema = z.union([
  UnshieldedTransactionEventSchema,
  UnshieldedTransactionsProgressSchema,
]);

export const RelevantTransactionSchema = z.object({
  __typename: z.literal('RelevantTransaction'),
  transaction: z.object({
    hash: Hash64,
  }),
  zswapCollapsedUpdate: z
    .object({
      startIndex: z.number(),
      endIndex: z.number(),
      update: VarLenghtHex,
      protocolVersion: z.number(),
    })
    .nullable(),
});

export const ShieldedTransactionsProgressSchema = z.object({
  __typename: z.literal('ShieldedTransactionsProgress'),
  highestZswapEndIndex: z.number(),
  highestCheckedZswapEndIndex: z.number(),
  highestRelevantZswapEndIndex: z.number(),
});

export const ShieldedTransactionEventSchema = z.union([
  RelevantTransactionSchema,
  ShieldedTransactionsProgressSchema,
]);

// Dust Generations schemas (PR #980)
export const DustRegistrationSchema = z.object({
  dustAddress: DustAddressBech32m,
  valid: z.boolean(),
  nightBalance: z.string().regex(/^\d+$/),
  generationRate: z.string().regex(/^\d+$/),
  maxCapacity: z.string().regex(/^\d+$/),
  currentCapacity: z.string().regex(/^\d+$/),
  utxoTxHash: z.string().nullable(),
  utxoOutputIndex: z.number().int().nullable(),
});

export const DustGenerationsSchema = z.object({
  cardanoRewardAddress: z
    .string()
    .refine(isCardanoRewardAddress, { message: 'Invalid Cardano reward address format' }),
  registrations: z.array(DustRegistrationSchema),
});

export const CollapsedMerkleTreeSchema = z.object({
  startIndex: z.number(),
  endIndex: z.number(),
  update: VarLenghtHex,
  protocolVersion: z.number(),
});

export const DustGenerationsItemSchema = z.object({
  __typename: z.literal('DustGenerationsItem'),
  commitmentMtIndex: z.number(),
  generationMtIndex: z.number(),
  owner: VarLenghtHex,
  value: z.string().regex(/^\d+$/),
  initialValue: z.string().regex(/^\d+$/),
  backingNight: VarLenghtHex,
  ctime: z.number(),
  transactionId: z.number(),
  transactionHash: Hash64,
  collapsedMerkleTree: CollapsedMerkleTreeSchema.nullable(),
});

export const DustGenerationsProgressSchema = z.object({
  __typename: z.literal('DustGenerationsProgress'),
  highestIndex: z.number(),
  collapsedMerkleTree: CollapsedMerkleTreeSchema.nullable(),
});

export const DustGenerationDtimeUpdateItemSchema = z.object({
  __typename: z.literal('DustGenerationDtimeUpdateItem'),
  generationMtIndex: z.number(),
  owner: VarLenghtHex,
  nightUtxoHash: VarLenghtHex,
  newDtime: z.number(),
  transactionId: z.number(),
  transactionHash: Hash64,
  treeInsertionPath: VarLenghtHex,
});

export const DustGenerationsEventSchema = z.discriminatedUnion('__typename', [
  DustGenerationsItemSchema,
  DustGenerationsProgressSchema,
  DustGenerationDtimeUpdateItemSchema,
]);

export const DustNullifierTransactionSchema = z.object({
  nullifierLeBytes: VarLenghtHex,
  commitmentLeBytes: VarLenghtHex,
  transactionId: z.number(),
  transactionHash: Hash64,
  blockHeight: z.number(),
  blockHash: Hash64,
  transaction: z.object({ hash: Hash64 }),
});

export const ShieldedNullifierTransactionSchema = z.object({
  transactionId: z.number(),
  transactionHash: Hash64,
  blockHash: Hash64,
  blockHeight: z.number(),
  nullifier: VarLenghtHex,
  transaction: z.object({ hash: Hash64 }),
});
