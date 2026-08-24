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
import { Contract as TicTacToeContract_ } from './out/contract/index.js';

type TicTacToePrivateState = {
  readonly playerXSecretKey: string;
  readonly playerOSecretKey: string;
};

type TicTacToeContract = TicTacToeContract_<TicTacToePrivateState>;
const TicTacToeContract = TicTacToeContract_;

const witnesses: Contract.Contract.Witnesses<TicTacToeContract> = {
  player_secret_key: ({ privateState }, player) => {
    const secretKey = Number(player) === 1
      ? privateState.playerXSecretKey
      : privateState.playerOSecretKey;
    return [privateState, new Uint8Array(Buffer.from(secretKey, 'hex'))];
  },
};

const createInitialPrivateState: () => TicTacToePrivateState = () => ({
  playerXSecretKey: '{{PLAYER_X_SK}}',
  playerOSecretKey: '{{PLAYER_O_SK}}',
});

export default {
  contractExecutable: CompiledContract.make<TicTacToeContract>(
    'TicTacToeContract',
    TicTacToeContract,
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
