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

// A coin is its nonce and opening; the wallet is what the owner can still spend. All of it
// round-trips through the JSON private-state file, hence hex throughout.
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

// Counter-derived rather than random, so a run is reproducible and the test knows which
// coin `mint` created.
const fill = (byte: number): string => byte.toString(16).padStart(2, '0').repeat(32);
const coinAt = (index: number): Coin => ({ nonce: fill(0x10 + index), opening: fill(0x40 + index) });

const asCoinInfo = (coin: Coin) => ({
  nonce: { bytes: bytes(coin.nonce) },
  opening: { bytes: bytes(coin.opening) },
});
const sameCoin = (left: Coin, right: Coin) =>
  left.nonce === right.nonce && left.opening === right.opening;

// Must match the contract's `derive_zk_public_key`, or a minted coin is unspendable.
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

  context$new_coin_info: ({ privateState }) => [
    privateState,
    asCoinInfo(coinAt(privateState.nextCoin)),
  ],

  // Runs after `context$new_coin_info`, so it advances the counter that produced the coin.
  private$add_coin: ({ privateState }, coin) => {
    const added: Coin = { nonce: hex(coin.nonce.bytes), opening: hex(coin.opening.bytes) };
    return [
      { ...privateState, coins: [...privateState.coins, added], nextCoin: privateState.nextCoin + 1 },
      [],
    ];
  },

  private$remove_coin: ({ privateState }, coin) => {
    const spent: Coin = { nonce: hex(coin.nonce.bytes), opening: hex(coin.opening.bytes) };
    return [{ ...privateState, coins: privateState.coins.filter((c) => !sameCoin(c, spent)) }, []];
  },

  // From the projected ledger, so the path matches the root the circuit checks. This
  // witness returns a bare path, not a Maybe, so a missing leaf has to throw.
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

  // The contract stores this and never opens it, so any bytes will do.
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
