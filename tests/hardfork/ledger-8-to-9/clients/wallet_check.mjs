#!/usr/bin/env node
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

// Wallet SDK checks across the fork.
//
//   wallet_check.mjs probe                                   fork schedule against the chain head
//   wallet_check.mjs sync                                    sync and print balances
//   wallet_check.mjs send [--kind shielded|unshielded] [--amount n] [--token t]
//   wallet_check.mjs dust-register                           register NIGHT for DUST generation
//   wallet_check.mjs restore [--expect-json <sync output>]   fresh wallet, balances compared

import { readFile } from 'node:fs/promises';
import { config, emit, flag, intFlag, log, runCommands } from './lib/config.mjs';
import { resolvedForks, submit, summarise, ttl, withWallet } from './lib/wallet.mjs';

const indexerHead = async () => {
  const res = await fetch(config.indexerHttp, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ query: '{ block { height hash protocolVersion } }' }),
  });
  if (!res.ok) throw new Error(`indexer ${config.indexerHttp} answered HTTP ${res.status}`);
  const block = (await res.json())?.data?.block;
  if (!block) throw new Error('indexer returned no block');
  return block;
};

// Order-independent, so two wallets that found the same tokens in another order match.
const balancesJson = (wallet, kind) =>
  JSON.stringify(Object.entries(wallet?.[kind]?.balances ?? {}).map(([t, v]) => [t, String(v)]).sort());

runCommands({
  async probe() {
    const forks = resolvedForks();
    const head = await indexerHead();
    emit({
      forks,
      chainHead: head,
      eraForHead: head.protocolVersion >= forks.v9 ? 'v9' : 'v8',
      provers: { v8: config.proofServerV8, v9: config.proofServerV9 },
    });
  },

  async sync() {
    const head = await indexerHead();
    return withWallet(({ state }) => emit({ chainHead: head, wallet: summarise(state) }));
  },

  // A transfer to the wallet's own address: the whole path, without decoding a foreign address.
  async send() {
    const kind = flag('kind', 'unshielded');
    const amount = BigInt(intFlag('amount', 1, 1));
    const tokenType = flag('token', '0'.repeat(64));
    return withWallet(async ({ facade, keystore, state }) => {
      const receiverAddress = kind === 'shielded' ? state.shielded.address : state.unshielded.address;
      log(`building a ${kind} transfer of ${amount}`);
      const recipe = await facade.transferTransaction([{ type: kind, outputs: [{ type: tokenType, receiverAddress, amount }] }], { ttl: ttl() });
      const signed = await facade.signRecipe(recipe, keystore.signDataAsync);
      log(`proving at protocol version ${recipe.protocolVersion}`);
      const txId = await submit(facade, await facade.finalizeRecipe(signed));
      emit({ submitted: true, txId, kind, amount, provedAtProtocolVersion: recipe.protocolVersion });
    });
  },

  // After the fork a native-NIGHT wallet has no DUST and pays for re-registration from
  // the DUST its NIGHT generated since.
  async 'dust-register'() {
    return withWallet(async ({ facade, keystore, state }) => {
      const utxos = state.unshielded?.availableCoins ?? [];
      if (utxos.length === 0) {
        log('FAIL: no NIGHT UTxOs to register');
        return 1;
      }
      const estimate = await facade.estimateRegistration(utxos);
      log(`registration fee estimate: ${estimate.fee}`);
      const recipe = await facade.registerNightUtxosForDustGeneration(utxos, keystore.getPublicKey(), keystore.signDataAsync);
      const txId = await submit(facade, await facade.finalizeRecipe(recipe));
      emit({ registered: true, txId, utxos: utxos.length, feeEstimate: estimate.fee });
    });
  },

  // A wallet new to this chain replays it from genesis across the fork and must reach the
  // live wallet's balances. DUST keeps generating, so it is not compared.
  async restore() {
    const expected = flag('expect-json');
    const { wallet, syncMs } = await withWallet(({ state, syncMs }) => ({ wallet: summarise(state), syncMs }));
    emit({ restoredIn: syncMs, peakRssMb: Math.round(process.resourceUsage().maxRSS / 1024), wallet });
    if (!expected) return 0;
    const want = JSON.parse(await readFile(expected, 'utf8')).wallet;
    const mismatched = ['unshielded', 'shielded'].filter((kind) => {
      const [got, wanted] = [balancesJson(wallet, kind), balancesJson(want, kind)];
      if (got !== wanted) log(`FAIL: restored ${kind} balances ${got} != live wallet's ${wanted}`);
      return got !== wanted;
    });
    return mismatched.length > 0 ? 1 : 0;
  },
});
