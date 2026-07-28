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

import { CompiledContract, ContractExecutable } from '@midnight-ntwrk/compact-js/effect';
import { Contract as TicTacToeContract_ } from './out/contract/index.js';

// The contract declares no witnesses and keeps all state on-chain, so the private state is empty.
type TicTacToePrivateState = Record<string, never>;

type TicTacToeContract = TicTacToeContract_<TicTacToePrivateState>;
const TicTacToeContract = TicTacToeContract_;

const createInitialPrivateState: () => TicTacToePrivateState = () => ({});

export default {
  contractExecutable: CompiledContract.make<TicTacToeContract>(
    'TicTacToeContract',
    TicTacToeContract,
  ).pipe(
    CompiledContract.withVacantWitnesses,
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
