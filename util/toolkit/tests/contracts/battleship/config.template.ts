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
  Contract as BattleshipContract_,
  type Board,
  type Committable,
} from './out/contract/index.js';

// The hidden ship position and the nonce blinding its commitment. This round-trips
// through the JSON private-state file, so no Uint8Array or bigint: hex and decimal.
type BattleshipPrivateState = {
  readonly secretKey: string;
  readonly boardNonce: string;
  readonly boardPosition: string;
};

type BattleshipContract = BattleshipContract_<BattleshipPrivateState>;
const BattleshipContract = BattleshipContract_;

const localBoard = (privateState: BattleshipPrivateState): Committable<Board> => ({
  nonce: BigInt(privateState.boardNonce),
  contents: { position: BigInt(privateState.boardPosition) },
});

const witnesses: Contract.Contract.Witnesses<BattleshipContract> = {
  // `red_pk(sk)`/`blue_pk(sk)` of this is the on-chain player identity.
  local_secret_key: ({ privateState }) => [
    privateState,
    new Uint8Array(Buffer.from(privateState.secretKey, 'hex')),
  ],

  // Re-proven against the on-chain commitment on every guess, concede and withdraw.
  local_board: ({ privateState }) => [privateState, localBoard(privateState)],

  // `start` writes the committed board back, so later calls open the same commitment.
  local_set_board: ({ privateState }, board) => [
    {
      ...privateState,
      boardNonce: board.nonce.toString(),
      boardPosition: board.contents.position.toString(),
    },
    [],
  ],

  // Deterministic: the configured nonce is the one `local_set_board` stores. Red and
  // Blue are seeded differently so their commitments differ.
  fresh_nonce: ({ privateState }) => [privateState, BigInt(privateState.boardNonce)],
};

const createInitialPrivateState: () => BattleshipPrivateState = () => ({
  secretKey: '{{SECRET_KEY}}',
  boardNonce: '{{BOARD_NONCE}}',
  // No ship yet; `start` overwrites this with the committed position.
  boardPosition: '0',
});

export default {
  contractExecutable: CompiledContract.make<BattleshipContract>(
    'BattleshipContract',
    BattleshipContract,
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
