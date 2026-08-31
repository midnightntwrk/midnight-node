// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import {
  Contract as DaoContract_,
  LocalState,
  type Ledger,
  type Maybe,
  type MerkleTreePath,
} from './out/contract/index.js';

// Voting is per-round, so the ballot and progress are keyed by the contract's `round` counter
// and a new round starts back at `LocalState.initial`. Everything here round-trips through the
// JSON private-state file, hence hex for the key (a Uint8Array would not survive) and plain
// objects for the maps.
type DaoPrivateState = {
  readonly secretKey: string;
  readonly ballots: Readonly<Record<string, boolean>>;
  readonly states: Readonly<Record<string, LocalState>>;
};

type DaoContract = DaoContract_<DaoPrivateState>;
const DaoContract = DaoContract_;

const roundKey = (round: bigint): string => round.toString();

const localStateFor = (privateState: DaoPrivateState, round: bigint): LocalState =>
  privateState.states[roundKey(round)] ?? LocalState.initial;

const some = <T>(value: T): Maybe<T> => ({ is_some: true, value });
const none = <T>(placeholder: T): Maybe<T> => ({ is_some: false, value: placeholder });

const witnesses: Contract.Contract.Witnesses<DaoContract> = {
  // `public_key(sk)` of this is what the contract compares against `organizer`.
  local_secret_key: ({ privateState }) => [
    privateState,
    new Uint8Array(Buffer.from(privateState.secretKey, 'hex')),
  ],

  local_state: ({ privateState, ledger }: { privateState: DaoPrivateState; ledger: Ledger }) => [
    privateState,
    localStateFor(privateState, ledger.round),
  ],

  // initial -> committed -> revealed, after a successful commit or reveal.
  local_advance_state: ({ privateState, ledger }: { privateState: DaoPrivateState; ledger: Ledger }) => {
    const key = roundKey(ledger.round);
    const current = localStateFor(privateState, ledger.round);
    const next = current === LocalState.initial ? LocalState.committed : LocalState.revealed;
    return [{ ...privateState, states: { ...privateState.states, [key]: next } }, []];
  },

  // Recorded at commit time so `vote_reveal` can reproduce the same commitment.
  local_record_vote: ({ privateState, ledger }: { privateState: DaoPrivateState; ledger: Ledger }, vote: boolean) => [
    { ...privateState, ballots: { ...privateState.ballots, [roundKey(ledger.round)]: vote } },
    [],
  ],

  local_vote_cast: ({ privateState, ledger }: { privateState: DaoPrivateState; ledger: Ledger }) => {
    const ballot = privateState.ballots[roundKey(ledger.round)];
    return [privateState, ballot === undefined ? none(false) : some(ballot)];
  },

  // Read from the projected ledger so it matches the tree whose root the circuit checks.
  local_path_of_cm: ({ privateState, ledger }: { privateState: DaoPrivateState; ledger: Ledger }, cm: Uint8Array) => {
    const path = ledger.committed_votes.findPathForLeaf(cm);
    return [
      privateState,
      path === undefined
        ? none({ leaf: new Uint8Array(32), path: [] } as unknown as MerkleTreePath<Uint8Array>)
        : some(path as MerkleTreePath<Uint8Array>),
    ];
  },
};

const createInitialPrivateState: () => DaoPrivateState = () => ({
  secretKey: '{{SECRET_KEY}}',
  ballots: {},
  states: {},
});

export default {
  contractExecutable: CompiledContract.make<DaoContract>(
    'DaoContract',
    DaoContract,
  ).pipe(
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withCompiledFileAssets('./out'),
    ContractExecutable.make,
  ),
  createInitialPrivateState,
  config: {
    keys: {
      coinPublic: '{{COIN_PUBLIC}}',
    },
    network: '{{NETWORK}}',
  },
};
