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
import { CompactTypeBytes, persistentHash } from '@midnight-ntwrk/compact-runtime';
import { Contract as ShieldedPoolContract_, type Ledger } from './out/contract/index.js';

type Coin = { readonly nonce: string; readonly opening: string };
type ShieldedPoolPrivateState = {
  readonly secretKey: string;
  readonly coins: readonly Coin[];
  readonly nextCoin: number;
};

type ShieldedPoolContract = ShieldedPoolContract_<ShieldedPoolPrivateState>;
const ShieldedPoolContract = ShieldedPoolContract_;

const bytes = (hex: string): Uint8Array => new Uint8Array(Buffer.from(hex, 'hex'));
const hex = (value: Uint8Array): string => Buffer.from(value).toString('hex');

const fill = (byte: number): string => byte.toString(16).padStart(2, '0').repeat(32);
const coinAt = (index: number): Coin => ({ nonce: fill(0x10 + index), opening: fill(0x40 + index) });

const asCoinInfo = (coin: Coin) => ({
  nonce: { bytes: bytes(coin.nonce) },
  opening: { bytes: bytes(coin.opening) },
});
const sameCoin = (left: Coin, right: Coin) =>
  left.nonce === right.nonce && left.opening === right.opening;

const zkPublicKey = (secretKey: string): Uint8Array =>
  persistentHash(new CompactTypeBytes(32), bytes(secretKey));

const witnesses: Contract.Contract.Witnesses<ShieldedPoolContract> = {
  private$zk_secret_key: ({ privateState }) => [
    privateState,
    { bytes: bytes(privateState.secretKey) },
  ],

  private$zk_public_key: ({ privateState }) => [
    privateState,
    { bytes: zkPublicKey(privateState.secretKey) },
  ],

  // Both mint and spend consume an index.
  context$new_coin_info: ({ privateState }) => [
    { ...privateState, nextCoin: privateState.nextCoin + 1 },
    asCoinInfo(coinAt(privateState.nextCoin)),
  ],

  private$add_coin: ({ privateState }, coin) => {
    const added: Coin = { nonce: hex(coin.nonce.bytes), opening: hex(coin.opening.bytes) };
    return [{ ...privateState, coins: [...privateState.coins, added] }, []];
  },

  private$remove_coin: ({ privateState }, coin) => {
    const spent: Coin = { nonce: hex(coin.nonce.bytes), opening: hex(coin.opening.bytes) };
    return [{ ...privateState, coins: privateState.coins.filter((c) => !sameCoin(c, spent)) }, []];
  },

  context$path_of: (
    { privateState, ledger }: { privateState: ShieldedPoolPrivateState; ledger: Ledger },
    cm: { bytes: Uint8Array },
  ) => {
    const path = ledger.commitments.findPathForLeaf(cm);
    if (path === undefined) {
      throw new Error(`no merkle path for commitment ${hex(cm.bytes)}`);
    }
    return [privateState, path];
  },

  // Test fixture only: this public payload is not encrypted.
  context$encrypt: ({ privateState }, pk, coin) => [
    privateState,
    new Uint8Array([...pk, ...coin.nonce.bytes, ...coin.opening.bytes]),
  ],
};

const createInitialPrivateState: () => ShieldedPoolPrivateState = () => ({
  secretKey: '{{SECRET_KEY}}',
  coins: [],
  nextCoin: 0,
});

export default {
  contractExecutable: CompiledContract.make<ShieldedPoolContract>(
    'ShieldedPoolContract',
    ShieldedPoolContract,
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
