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
  Contract as ElectionContract_,
  PermissibleVotes,
  PrivateState,
  type Ledger,
  type Maybe,
  type MerkleTreePath,
} from './out/contract/index.js';

// Round-trips through the JSON private-state file: hex key, numeric enums, and a `ballot`
// that is null until `vote_commit` records one.
type ElectionPrivateState = {
  readonly secretKey: string;
  readonly state: PrivateState;
  readonly ballot: PermissibleVotes | null;
};

type ElectionContract = ElectionContract_<ElectionPrivateState>;
const ElectionContract = ElectionContract_;

const some = <T>(value: T): Maybe<T> => ({ is_some: true, value });
const none = <T>(placeholder: T): Maybe<T> => ({ is_some: false, value: placeholder });

// Compact serializes the value inside an absent Maybe, so its path must still be valid.
const absentMerklePath: MerkleTreePath<Uint8Array> = {
  leaf: new Uint8Array(32),
  path: Array.from({ length: 10 }, () => ({
    sibling: { field: 0n },
    goes_left: false,
  })),
};

const maybePath = (path: MerkleTreePath<Uint8Array> | undefined): Maybe<MerkleTreePath<Uint8Array>> =>
  path === undefined ? none(absentMerklePath) : some(path);

const witnesses: Contract.Contract.Witnesses<ElectionContract> = {
  // `public_key(sk)` of this is the allowlisted leaf and the authority identity.
  private$secret_key: ({ privateState }) => [
    privateState,
    new Uint8Array(Buffer.from(privateState.secretKey, 'hex')),
  ],

  private$state: ({ privateState }) => [privateState, privateState.state],

  // initial -> committed -> revealed, after a successful commit or reveal.
  private$state$advance: ({ privateState }) => {
    const next =
      privateState.state === PrivateState.initial ? PrivateState.committed : PrivateState.revealed;
    return [{ ...privateState, state: next }, []];
  },

  // Recorded at commit time so `vote_reveal` can reproduce the same commitment.
  private$vote$record: ({ privateState }, ballot) => [{ ...privateState, ballot }, []],

  private$vote: ({ privateState }) => [
    privateState,
    privateState.ballot ?? PermissibleVotes.no,
  ],

  // From the projected ledger, so each path matches the root the circuit checks.
  context$eligible_voters$path_of: (
    { privateState, ledger }: { privateState: ElectionPrivateState; ledger: Ledger },
    pk: Uint8Array,
  ) => [privateState, maybePath(ledger.eligible_voters.findPathForLeaf(pk) as MerkleTreePath<Uint8Array> | undefined)],

  context$committed_votes$path_of: (
    { privateState, ledger }: { privateState: ElectionPrivateState; ledger: Ledger },
    cm: Uint8Array,
  ) => [privateState, maybePath(ledger.committed_votes.findPathForLeaf(cm) as MerkleTreePath<Uint8Array> | undefined)],
};

const createInitialPrivateState: () => ElectionPrivateState = () => ({
  secretKey: '{{SECRET_KEY}}',
  state: PrivateState.initial,
  ballot: null,
});

export default {
  contractExecutable: CompiledContract.make<ElectionContract>(
    'ElectionContract',
    ElectionContract,
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
