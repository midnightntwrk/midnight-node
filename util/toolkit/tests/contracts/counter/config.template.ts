// This file is part of midnight-node.
// Copyright (C) Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { CompiledContract, ContractExecutable, type Contract } from '@midnight-ntwrk/compact-js/effect';
import { Contract as CounterContract_ } from './out/contract/index.js';

type CounterPrivateState = {
  readonly privateCounter: number;
};

type CounterContract = CounterContract_<CounterPrivateState>;
const CounterContract = CounterContract_;

const witnesses: Contract.Contract.Witnesses<CounterContract> = {
  privateIncrement: ({ privateState }) => [
    { privateCounter: privateState.privateCounter + 1 },
    [],
  ],
};

const createInitialPrivateState: () => CounterPrivateState = () => ({
  privateCounter: 0,
});

export default {
  contractExecutable: CompiledContract.make<CounterContract>(
    'CounterContract',
    CounterContract,
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
