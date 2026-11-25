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

// An application that never upgraded: wallet SDK 1.x, which knows only ledger 8, proving at
// the ledger-8 proof server. Same environment and output as the suite's clients/wallet_check.mjs.
//
//   wallet_check.mjs sync                       sync and print balances
//   wallet_check.mjs send [--amount n]          an unshielded NIGHT transfer to itself

import {
  DustWallet,
  HDWallet,
  InMemoryTransactionHistoryStorage,
  PublicKey,
  Roles,
  ShieldedWallet,
  UnshieldedWallet,
  WalletEntrySchema,
  WalletFacade,
  createKeystore,
  mergeWalletEntries,
} from '@midnightntwrk/wallet-sdk';
import * as ledger from '@midnight-ntwrk/ledger-v8';
import { config, emit, intFlag, log, runCommands, txTimeoutSecs, withTimeout } from '../../../clients/lib/config.mjs';

const NIGHT = '0'.repeat(64);
const ttl = () => new Date(Date.now() + 60 * 60 * 1000);

const keys = () => {
  const hd = HDWallet.fromSeed(Uint8Array.from(Buffer.from(config.seed.replace(/^0x/, ''), 'hex')));
  if (hd.type !== 'seedOk') throw new Error(`seed rejected: ${hd.type}`);
  const derived = hd.hdWallet.selectAccount(0).selectRoles([Roles.Zswap, Roles.NightExternal, Roles.Dust]).deriveKeysAt(0);
  if (derived.type !== 'keysDerived') throw new Error(`key derivation: ${derived.type}`);
  hd.hdWallet.clear();
  return derived.keys;
};

const withWallet = async (fn) => {
  const k = keys();
  const configuration = {
    networkId: config.networkId,
    costParameters: { feeBlocksMargin: 5 },
    relayURL: new URL(config.nodeWs),
    provingServerUrl: new URL(config.proofServerOld),
    indexerClientConnection: { indexerHttpUrl: config.indexerHttp, indexerWsUrl: config.indexerWs },
    txHistoryStorage: new InMemoryTransactionHistoryStorage(WalletEntrySchema, mergeWalletEntries),
  };
  const keystore = createKeystore(k[Roles.NightExternal], configuration.networkId);
  const shieldedSecretKeys = ledger.ZswapSecretKeys.fromSeed(k[Roles.Zswap]);
  const dustSecretKey = ledger.DustSecretKey.fromSeed(k[Roles.Dust]);
  log(`starting a wallet SDK 1.x wallet on ${config.networkId}: indexer ${config.indexerHttp}, prover ${config.proofServerOld}`);
  const facade = await WalletFacade.init({
    configuration,
    shielded: (c) => ShieldedWallet(c).startWithSecretKeys(shieldedSecretKeys),
    unshielded: (c) => UnshieldedWallet(c).startWithPublicKey(PublicKey.fromKeyStore(keystore)),
    dust: (c) => DustWallet(c).startWithSecretKey(dustSecretKey, ledger.LedgerParameters.initialParameters().dust),
  });
  await facade.start(shieldedSecretKeys, dustSecretKey);
  // A heartbeat, so a stalled sync shows where it stopped.
  let last;
  const sub = facade.state().subscribe((s) => { last = s; });
  const beat = setInterval(() => log('progress', progressJson(last)), 60_000);
  try {
    const started = Date.now();
    // A wallet that cannot decode the chain may never report synced: that is a finding too.
    const state = await withTimeout(facade.waitForSyncedState(), config.syncTimeoutMs / 1000, 'wallet sync (MN_SYNC_TIMEOUT_MS)');
    return await fn({ facade, keystore, secretKeys: { shieldedSecretKeys, dustSecretKey }, state, syncMs: Date.now() - started });
  } finally {
    clearInterval(beat);
    sub.unsubscribe();
    await withTimeout(facade.stop(), 30, 'wallet stop').catch((e) => log('stop failed (ignored):', e?.message ?? e));
  }
};

const progressJson = (s) =>
  JSON.stringify({ synced: s?.isSynced, shielded: s?.shielded?.progress, unshielded: s?.unshielded?.progress, dust: s?.dust?.progress },
    (_k, v) => (typeof v === 'bigint' ? v.toString() : v));

const summarise = ({ shielded, unshielded, dust }) => ({
  shielded: { balances: shielded?.balances, progress: shielded?.progress },
  unshielded: { address: unshielded?.address, balances: unshielded?.balances, progress: unshielded?.progress },
  dust: { availableCoins: dust?.availableCoins?.length ?? 0, progress: dust?.progress },
});

runCommands({
  async sync() {
    return withWallet(async ({ state, syncMs }) => emit({ syncMs, wallet: summarise(state) }));
  },

  async send() {
    const amount = BigInt(intFlag('amount', 1, 1));
    return withWallet(async ({ facade, keystore, secretKeys, state, syncMs }) => {
      const receiverAddress = state.unshielded.address;
      log(`building an unshielded transfer of ${amount} NIGHT to itself`);
      const recipe = await facade.transferTransaction(
        [{ type: 'unshielded', outputs: [{ type: NIGHT, receiverAddress, amount }] }], secretKeys, { ttl: ttl() });
      const signed = await facade.signRecipe(recipe, (data) => keystore.signData(data));
      const tx = await facade.finalizeRecipe(signed);
      const txId = await withTimeout(facade.submitTransaction(tx), txTimeoutSecs(), 'submitTransaction (MN_TX_TIMEOUT_SECS; it may still land)');
      emit({ submitted: true, txId, amount, syncMs });
    });
  },
});
