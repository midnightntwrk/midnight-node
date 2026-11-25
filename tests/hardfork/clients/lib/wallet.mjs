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

// A wallet built the way an application builds it: shielded, unshielded and DUST wallets
// behind the facade, syncing from the indexer and proving at the proof server of each
// ledger version. The SDK reads the chain with the old ledger below the fork's protocol
// version and with the new one from it.

import {
  DustWallet,
  InMemoryTransactionHistoryStorage,
  mergeWalletEntries,
  PublicKey,
  ShieldedWallet,
  UnshieldedWallet,
  WalletEntrySchema,
  WalletFacade,
  WalletSeeds,
  createKeystore,
} from '@midnightntwrk/wallet-sdk';
import { makeIndexerChainVersionProbe } from '@midnightntwrk/wallet-sdk-capabilities/chainVersion';
import { config, eras, log, txTimeoutSecs, withTimeout } from './config.mjs';

export const ttl = () => new Date(Date.now() + 60 * 60 * 1000);

const walletConfiguration = () => {
  const indexerClientConnection = { indexerHttpUrl: config.indexerHttp, indexerWsUrl: config.indexerWs };
  return {
    networkId: config.networkId,
    costParameters: { feeBlocksMargin: 5 },
    relayURL: new URL(config.nodeWs),
    indexerClientConnection,
    provers: eras.each(
      { kind: 'server', url: new URL(config.proofServerOld) },
      { kind: 'server', url: new URL(config.proofServerNew) },
    ),
    txHistoryStorage: new InMemoryTransactionHistoryStorage(WalletEntrySchema, mergeWalletEntries),
    chainVersionProbe: makeIndexerChainVersionProbe({ indexerClientConnection }),
  };
};

export const resolvedForks = () => WalletFacade.resolveConfiguration(walletConfiguration()).forks;

const startWallet = async (snapshot) => {
  const configuration = walletConfiguration();
  const seeds = WalletSeeds.fromMasterSeed(Uint8Array.from(Buffer.from(config.seed.replace(/^0x/, ''), 'hex')));
  // Schnorr signs on both sides of the fork.
  const keystore = createKeystore({ kind: 'schnorr', secret: seeds.unshielded }, configuration.networkId);
  log(`starting wallet on ${config.networkId}${snapshot ? ' from a snapshot' : ''}: indexer ${config.indexerHttp}, provers ${config.proofServerOld} / ${config.proofServerNew}`);
  const facade = await WalletFacade.init({
    configuration,
    shielded: (c) => (snapshot ? ShieldedWallet(c).restore(snapshot.shielded) : ShieldedWallet(c).startWithSeed(seeds.shielded)),
    unshielded: (c) => (snapshot ? UnshieldedWallet(c).restore(snapshot.unshielded) : UnshieldedWallet(c).startWithPublicKey(PublicKey.fromKeyStore(keystore))),
    dust: (c) => (snapshot ? DustWallet(c).restore(snapshot.dust) : DustWallet(c).startWithSeed(seeds.dust)),
  });
  await facade.start(seeds);
  return { facade, keystore };
};

// What an application persists between sessions: the three wallets' serialized state.
export const serializeWallets = async (facade) => ({
  shielded: await facade.shielded.serializeState(),
  unshielded: await facade.unshielded.serializeState(),
  dust: await facade.dust.serializeState(),
});

export const withWallet = async (fn, { snapshot } = {}) => {
  const { facade, keystore } = await startWallet(snapshot);
  try {
    const started = Date.now();
    const state = await syncedState(facade);
    return await fn({ facade, keystore, state, syncMs: Date.now() - started });
  } finally {
    await withTimeout(facade.stop(), 30, 'wallet stop').catch((e) => log('stop failed (ignored):', e?.message ?? e));
  }
};

// A stuck indexer must fail the check, not hang it.
const syncedState = (facade) =>
  withTimeout(facade.waitForSyncedState(), config.syncTimeoutMs / 1000, 'wallet sync (MN_SYNC_TIMEOUT_MS)');

// Right after a sync across the fork the three wallets can still be crossing it, and a
// transaction built then is refused as built for the other side. This waits until all of
// them have settled on one protocol version.
export const settledState = (facade) =>
  withTimeout(
    new Promise((resolve) => {
      const subscription = facade.state().subscribe((state) => {
        if (state.protocol?._tag !== 'Settled') return;
        queueMicrotask(() => subscription.unsubscribe());
        resolve(state);
      });
    }),
    config.syncTimeoutMs / 1000,
    'wallets settling on one protocol version (MN_SYNC_TIMEOUT_MS)',
  );

// A transaction the pool accepts but never includes must fail the check, not hang it.
export const submit = (facade, tx) =>
  withTimeout(facade.submitTransaction(tx), txTimeoutSecs(), 'submitTransaction (MN_TX_TIMEOUT_SECS; it may still land)');

export const summarise = ({ shielded, unshielded, dust }) => ({
  shielded: {
    address: shielded?.address,
    coinPublicKey: shielded?.coinPublicKey,
    balances: shielded?.balances,
    progress: shielded?.progress,
  },
  unshielded: { address: unshielded?.address, balances: unshielded?.balances, progress: unshielded?.progress },
  dust: {
    address: dust?.address,
    publicKey: dust?.publicKey,
    availableCoins: dust?.availableCoins?.length ?? 0,
    totalCoins: dust?.totalCoins?.length ?? 0,
    progress: dust?.progress,
  },
});

// The wallet's transaction history, counted by the ledger era each entry was made in.
export const historySummary = async (facade, forks) => {
  const entries = await facade.getAllFromTxHistory();
  const era = (e) => (e.protocolVersion === undefined ? 'unknown' : eras.of(e.protocolVersion, forks));
  const count = (key) => entries.reduce((acc, e) => ({ ...acc, [key(e)]: (acc[key(e)] ?? 0) + 1 }), {});
  return {
    total: entries.length,
    byEra: count(era),
    byLifecycle: count((e) => e.lifecycle?.status ?? 'unknown'),
    byStatus: count((e) => e.status ?? 'none'),
  };
};
