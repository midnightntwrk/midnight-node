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
import { Contract as DaoContract_, LocalState } from './out/contract/index.js';

type DaoPrivateState = {
  readonly secretKey: string;
  readonly state: LocalState;
  readonly vote: boolean | null;
};

type DaoContract = DaoContract_<DaoPrivateState>;
const DaoContract = DaoContract_;

const emptyPath = {
  leaf: new Uint8Array(32),
  path: Array.from({ length: 10 }, () => ({ sibling: { field: 0n }, goes_left: false })),
};

const witnesses: Contract.Contract.Witnesses<DaoContract> = {
  local_secret_key: ({ privateState }) => [
    privateState,
    new Uint8Array(Buffer.from(privateState.secretKey, 'hex')),
  ],
  local_state: ({ privateState }) => [privateState, privateState.state],
  local_advance_state: ({ privateState }) => [
    { ...privateState, state: (privateState.state + 1) as LocalState },
    [],
  ],
  local_record_vote: ({ privateState }, vote) => [{ ...privateState, vote }, []],
  local_vote_cast: ({ privateState }) => [
    privateState,
    { is_some: privateState.vote !== null, value: privateState.vote ?? false },
  ],
  local_path_of_cm: ({ ledger, privateState }, cm) => {
    const path = ledger.committed_votes.findPathForLeaf(cm);
    return [privateState, path ? { is_some: true, value: path } : { is_some: false, value: emptyPath }];
  },
};

const createInitialPrivateState: () => DaoPrivateState = () => ({
  secretKey: '{{SECRET_KEY}}',
  state: LocalState.initial,
  vote: null,
});

export default {
  contractExecutable: CompiledContract.make<DaoContract>('DaoContract', DaoContract).pipe(
    CompiledContract.withWitnesses(witnesses),
    CompiledContract.withCompiledFileAssets('./out'),
    ContractExecutable.make,
  ),
  createInitialPrivateState,
  config: {
    keys: { coinPublic: '{{COIN_PUBLIC}}' },
    network: '{{NETWORK}}',
  },
};
