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

import { GraphQLError } from 'graphql';

export type GraphQLResponse<T> = {
  data: T | null;
  errors?: GraphQLError[];
};

export type BlockResponse = GraphQLResponse<{ block: Block }>;

export type TransactionResponse = GraphQLResponse<{ transactions: Transaction[] }>;

export type ContractActionResponse = GraphQLResponse<{ contractAction: ContractAction }>;

export interface ZswapMerkleTreeCollapsedUpdateResult {
  startIndex: number;
  endIndex: number;
  update: string;
  protocolVersion: number;
}

export type ZswapMerkleTreeCollapsedUpdateResponse = GraphQLResponse<{
  zswapMerkleTreeCollapsedUpdate: ZswapMerkleTreeCollapsedUpdateResult;
}>;

export type DustGenerationStatusResponse = GraphQLResponse<{
  dustGenerationStatus: DustGenerationStatus[];
}>;

export type BlockOffset = {
  hash?: string;
  height?: number;
};

export type TransactionOffset = {
  hash?: string;
  identifier?: string;
};

export type ContractActionOffset = {
  blockOffset?: BlockOffset;
  transactionOffset?: TransactionOffset;
};

export type UnshieldedAddress = string;

export interface Block {
  hash: string;
  height: number;
  timestamp: string;
  protocolVersion: number;
  author: string | null;
  ledgerParameters: string;
  zswapMerkleTreeRoot: string;
  dustCommitmentMerkleTreeRoot: string | null;
  dustGenerationMerkleTreeRoot: string | null;
  zswapEndIndex: number;
  dustCommitmentEndIndex: number;
  dustGenerationEndIndex: number;
  parent: Block;
  transactions: Transaction[];
}

export interface UnshieldedUtxo {
  owner: string;
  intentHash: string;
  value: string;
  ctime: number | null;
  registeredForDustGeneration: boolean;
  tokenType: string;
  outputIndex: number;
  createdAtTransaction: Transaction;
  spentAtTransaction: Transaction;
}

export type TransactionResult = {
  status: TransactionResultStatus;
  segments?: Segment[];
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

// Base Transaction interface (common to both RegularTransaction and SystemTransaction)
export interface Transaction {
  __typename: 'RegularTransaction' | 'SystemTransaction';
  id?: number;
  hash?: string;
  protocolVersion?: number;
  raw?: string;
  block?: Block;
  identifiers?: string[];
  contractActions?: ContractAction[];
  unshieldedCreatedOutputs?: UnshieldedUtxo[];
  unshieldedSpentOutputs?: UnshieldedUtxo[];
  zswapLedgerEvents?: ZswapLedgerEvent[];
  dustLedgerEvents?: DustLedgerEvent[];
}

// RegularTransaction interface (includes additional fields)
export interface RegularTransaction extends Transaction {
  identifiers?: string[];
  zswapMerkleTreeRoot?: string;
  zswapStartIndex?: number;
  zswapEndIndex?: number;
  dustCommitmentStartIndex?: number;
  dustCommitmentEndIndex?: number;
  dustGenerationStartIndex?: number;
  dustGenerationEndIndex?: number;
  fee?: string;
  fees?: TransactionFees;
  transactionResult?: TransactionResult;
}

// SystemTransaction interface (only base fields)
// eslint-disable-next-line @typescript-eslint/no-empty-object-type
export interface SystemTransaction extends Transaction {
  // No additional fields beyond the base Transaction interface
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

export interface ZswapCollapsedUpdate {
  startIndex: number;
  endIndex: number;
  update: string;
  protocolVersion: number;
}

export interface RelevantTransaction {
  __typename: 'RelevantTransaction';
  transaction: RegularTransaction;
  zswapCollapsedUpdate?: ZswapCollapsedUpdate;
}

export interface ShieldedTransactionsProgress {
  __typename: 'ShieldedTransactionsProgress';
  highestZswapEndIndex: number;
  highestCheckedZswapEndIndex: number;
  highestRelevantZswapEndIndex: number;
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

export function isUnshieldedTransaction(
  event: UnshieldedTransactionEvent,
): event is UnshieldedTransaction {
  return event.__typename === 'UnshieldedTransaction';
}

export type ContractAction = ContractDeploy | ContractCall | ContractUpdate;

export interface ContractDeploy {
  __typename: 'ContractDeploy';
  address: string;
  state: string;
  zswapState: string;
  transaction: Transaction;
  unshieldedBalances: ContractBalance[];
}

export interface ContractCall {
  __typename: 'ContractCall';
  address: string;
  state: string;
  zswapState: string;
  transaction: Transaction;
  entryPoint: string;
  deploy: ContractDeploy;
  unshieldedBalances: ContractBalance[];
}

export interface ContractUpdate {
  __typename: 'ContractUpdate';
  address: string;
  state: string;
  zswapState: string;
  transaction: Transaction;
  unshieldedBalances: ContractBalance[];
}

export interface ContractBalance {
  tokenType: string;
  amount: string;
}

export type ContractEventResponse = GraphQLResponse<{ contractEvents: ContractEvent[] }>;

/**
 * The 11 standard contract-event variants per MIP-0002 Appendix A. Mirrors the
 * `ContractEventType` GraphQL enum used by `ContractEventFilter.types`.
 */
export type ContractEventType =
  | 'SHIELDED_SPEND'
  | 'SHIELDED_RECEIVE'
  | 'SHIELDED_MINT'
  | 'SHIELDED_BURN'
  | 'UNSHIELDED_SPEND'
  | 'UNSHIELDED_RECEIVE'
  | 'UNSHIELDED_MINT'
  | 'UNSHIELDED_BURN'
  | 'PAUSED'
  | 'UNPAUSED'
  | 'MISC';

export const CONTRACT_EVENT_TYPES: ContractEventType[] = [
  'SHIELDED_SPEND',
  'SHIELDED_RECEIVE',
  'SHIELDED_MINT',
  'SHIELDED_BURN',
  'UNSHIELDED_SPEND',
  'UNSHIELDED_RECEIVE',
  'UNSHIELDED_MINT',
  'UNSHIELDED_BURN',
  'PAUSED',
  'UNPAUSED',
  'MISC',
];

/** Prefix filter on an indexed contract-event field (e.g. nullifier, tokenType). */
export interface FieldPrefixFilter {
  fieldName: string;
  prefix: string;
}

export interface ContractEventFilter {
  contractAddress: string;
  types?: ContractEventType[];
  fieldPrefixes?: FieldPrefixFilter[];
  fromBlock?: number;
  toBlock?: number;
  transactionHash?: string;
}

/**
 * Tagged union for `Either<ZswapCoinPublicKey, ContractAddress>` fields
 * (UnshieldedSpend/Receive `sender`/`recipient`, UnshieldedBurn `sender`).
 * Exactly one of `userAddress` / `contractAddress` is populated per `kind`.
 */
export interface AddressOrContract {
  kind: 'USER' | 'CONTRACT';
  userAddress?: string;
  contractAddress?: string;
}

/** Fields common to every concrete contract event (the `ContractEvent` interface). */
export interface ContractEventBase {
  __typename: string;
  id: number;
  raw: string;
  maxId: number;
  protocolVersion: number;
  version: number;
  contractAddress: string;
  transactionId: number;
  transaction: Transaction;
}

export interface ShieldedSpendEvent extends ContractEventBase {
  __typename: 'ShieldedSpendEvent';
  nullifier: string;
}

export interface ShieldedReceiveEvent extends ContractEventBase {
  __typename: 'ShieldedReceiveEvent';
  commitment: string;
  ciphertext?: string | null;
  receivingContractAddress?: string | null;
}

export interface ShieldedMintEvent extends ContractEventBase {
  __typename: 'ShieldedMintEvent';
  commitment: string;
  domainSep: string;
  amount?: string | null;
}

export interface ShieldedBurnEvent extends ContractEventBase {
  __typename: 'ShieldedBurnEvent';
  nullifier: string;
  amount?: string | null;
}

export interface UnshieldedSpendEvent extends ContractEventBase {
  __typename: 'UnshieldedSpendEvent';
  sender: AddressOrContract;
  domainSep: string;
  tokenType: string;
  amount: string;
}

export interface UnshieldedReceiveEvent extends ContractEventBase {
  __typename: 'UnshieldedReceiveEvent';
  recipient: AddressOrContract;
  domainSep: string;
  tokenType: string;
  amount: string;
}

export interface UnshieldedMintEvent extends ContractEventBase {
  __typename: 'UnshieldedMintEvent';
  domainSep: string;
  tokenType: string;
  amount: string;
}

export interface UnshieldedBurnEvent extends ContractEventBase {
  __typename: 'UnshieldedBurnEvent';
  sender: AddressOrContract;
  tokenType: string;
  amount: string;
}

export interface PausedEvent extends ContractEventBase {
  __typename: 'PausedEvent';
}

export interface UnpausedEvent extends ContractEventBase {
  __typename: 'UnpausedEvent';
}

export interface MiscContractEvent extends ContractEventBase {
  __typename: 'MiscContractEvent';
  name: string;
  payload: string;
}

export type ContractEvent =
  | ShieldedSpendEvent
  | ShieldedReceiveEvent
  | ShieldedMintEvent
  | ShieldedBurnEvent
  | UnshieldedSpendEvent
  | UnshieldedReceiveEvent
  | UnshieldedMintEvent
  | UnshieldedBurnEvent
  | PausedEvent
  | UnpausedEvent
  | MiscContractEvent;

export interface DustGenerationStatus {
  cardanoRewardAddress: string;
  dustAddress?: string;
  registered: boolean;
  nightBalance: string;
  generationRate: string;
  currentCapacity: string;
  maxCapacity: string;
  utxoTxHash: string | null;
  utxoOutputIndex: number | null;
}

export interface ZswapLedgerEvent {
  id: number;
  raw: string;
  maxId: number;
  protocolVersion: number;
}

export type DustLedgerEvent =
  | {
      __typename: 'ParamChange';
      id: number;
      raw: string;
      maxId: number;
      protocolVersion: number;
    }
  | {
      __typename: 'DustInitialUtxo';
      id: number;
      raw: string;
      maxId: number;
      protocolVersion: number;
      output: {
        nonce: string;
      };
    }
  | {
      __typename: 'DustGenerationDtimeUpdate';
      id: number;
      raw: string;
      maxId: number;
      protocolVersion: number;
    }
  | {
      __typename: 'DustSpendProcessed';
      id: number;
      raw: string;
      maxId: number;
      protocolVersion: number;
    };

// Dust Generations types (PR #980)
export interface DustRegistration {
  dustAddress: string;
  valid: boolean;
  nightBalance: string;
  generationRate: string;
  maxCapacity: string;
  currentCapacity: string;
  utxoTxHash: string | null;
  utxoOutputIndex: number | null;
}

export interface DustGenerations {
  cardanoRewardAddress: string;
  registrations: DustRegistration[];
}

export type DustGenerationsResponse = GraphQLResponse<{
  dustGenerations: DustGenerations[];
}>;

// c2m-bridge query surface (#941). Only BridgeUserTransfer carries fully-populated
// fields today; other variants are discriminated by __typename until data exists.
export interface BridgeUserTransfer {
  __typename: 'BridgeUserTransfer';
  id: number;
  blockHeight: number;
  midnightTxHash: string;
  cardanoTxHash: string;
  amount: string;
  recipient: string;
}

// c2m-bridge pool observability surface (#944).
export type BridgeTreasuryReason = 'INVALID' | 'UNAPPROVED' | 'SUBMINIMAL_FLUSH';

export const BRIDGE_TREASURY_REASONS: BridgeTreasuryReason[] = [
  'INVALID',
  'UNAPPROVED',
  'SUBMINIMAL_FLUSH',
];

export interface BridgeTreasuryAggregate {
  reason: BridgeTreasuryReason;
  total: string;
}

export interface BridgePoolSummary {
  reserveTotal: string;
  treasuryByReason: BridgeTreasuryAggregate[];
  subminimumTxCount: number;
  lastEventBlockHeight: number | null;
}

export type BridgePoolSummaryResponse = GraphQLResponse<{ bridgePoolSummary: BridgePoolSummary }>;

// Inflow event lists. Only BridgeReserveTransfer carries populated fields today;
// treasury variants are discriminated by __typename until data exists.
export interface BridgeReserveTransfer {
  __typename: 'BridgeReserveTransfer';
  id: number;
  blockHeight: number;
  midnightTxHash: string;
  cardanoTxHash: string;
  amount: string;
}

export interface BridgeEventOther {
  __typename: string;
  id?: number;
  recipient?: string;
  amount?: string;
}

export type BridgeEvent = BridgeUserTransfer | BridgeReserveTransfer | BridgeEventOther;

export type BridgeEventsResponse = GraphQLResponse<{ bridgeEvents: BridgeEvent[] }>;

export type BridgeDepositsResponse = GraphQLResponse<{ bridgeDeposits: BridgeEvent[] }>;

export interface BridgeBalance {
  deposited: string;
  claimed: string;
  balance: string;
}

export type BridgeBalanceResponse = GraphQLResponse<{ bridgeBalance: BridgeBalance }>;

export type BridgeReserveInflowsResponse = GraphQLResponse<{ bridgeReserveInflows: BridgeEvent[] }>;

export type BridgeTreasuryInflowsResponse = GraphQLResponse<{
  bridgeTreasuryInflows: BridgeEvent[];
}>;

// #1304: Block.contractZswapState and the composed CCC execution-inputs read.
export type BlockContractZswapStateResponse = GraphQLResponse<{
  block: {
    hash: string;
    height: number;
    contractZswapState: string | null;
  } | null;
}>;

export type ExecutionInputsResponse = GraphQLResponse<{
  block: {
    hash: string;
    ledgerParameters: string;
    contractZswapState: string | null;
  } | null;
  contract: { state: string } | null;
}>;

// #1275: top-level Contract type and contract(address, offset) query.
export type ContractMaintenanceVerifyingKeyKind = 'SCHNORR' | 'ECDSA';

export interface ContractMaintenanceVerifyingKey {
  kind: ContractMaintenanceVerifyingKeyKind;
  key: string;
}

export interface ContractMaintenanceAuthority {
  committee: ContractMaintenanceVerifyingKey[];
  threshold: number;
  counter: number;
}

export type ContractActionTypeEnum = 'DEPLOY' | 'CALL' | 'UPDATE';

export interface ContractTypeActionRef {
  __typename: string;
  address: string;
}

export interface ContractType {
  address: string;
  state: string;
  maintenanceAuthority: ContractMaintenanceAuthority;
  actions: ContractTypeActionRef[];
}

export type ContractResponse = GraphQLResponse<{ contract: ContractType | null }>;

export interface DustCommitmentMerkleTreeUpdateResult {
  startIndex: number;
  endIndex: number;
  update: string;
  protocolVersion: number;
}

export type DustCommitmentMerkleTreeUpdateResponse = GraphQLResponse<{
  dustCommitmentMerkleTreeUpdate: DustCommitmentMerkleTreeUpdateResult;
}>;

export interface DustGenerationMerkleTreeUpdateResult {
  startIndex: number;
  endIndex: number;
  update: string;
  protocolVersion: number;
}

export type DustGenerationMerkleTreeUpdateResponse = GraphQLResponse<{
  dustGenerationMerkleTreeUpdate: DustGenerationMerkleTreeUpdateResult;
}>;

export interface CollapsedMerkleTree {
  startIndex: number;
  endIndex: number;
  update: string;
  protocolVersion: number;
}

export interface DustGenerationsItem {
  __typename: 'DustGenerationsItem';
  commitmentMtIndex: number;
  generationMtIndex: number;
  owner: string;
  value: string;
  initialValue: string;
  backingNight: string;
  ctime: number;
  transactionId: number;
  transactionHash: string;
  collapsedMerkleTree: CollapsedMerkleTree | null;
}

export interface DustGenerationsProgress {
  __typename: 'DustGenerationsProgress';
  highestIndex: number;
  collapsedMerkleTree: CollapsedMerkleTree | null;
}

export interface DustGenerationDtimeUpdateItem {
  __typename: 'DustGenerationDtimeUpdateItem';
  generationMtIndex: number;
  owner: string;
  nightUtxoHash: string;
  newDtime: number;
  transactionId: number;
  transactionHash: string;
  treeInsertionPath: string;
}

export type DustGenerationsEvent =
  DustGenerationsItem | DustGenerationsProgress | DustGenerationDtimeUpdateItem;

export interface DustNullifierTransaction {
  nullifierLeBytes: string;
  commitmentLeBytes: string;
  transactionId: number;
  transactionHash: string;
  blockHeight: number;
  blockHash: string;
  transaction: { hash: string };
}

export interface ShieldedNullifierTransaction {
  transactionId: number;
  transactionHash: string;
  blockHash: string;
  blockHeight: number;
  nullifier: string;
  transaction: { hash: string };
}

export type ViewingKey = string & { __brand: 'ViewingKey' };
