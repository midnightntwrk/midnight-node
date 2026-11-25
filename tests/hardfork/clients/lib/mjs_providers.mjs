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

// The Midnight.js providers on the same chain, indexer and proof servers as the wallet.
// Midnight.js routes each transaction handle to the handler of its ledger era.

import { indexerPublicDataProvider } from '@midnight-ntwrk/midnight-js-indexer-public-data-provider';
import { httpClientProofProvider } from '@midnight-ntwrk/midnight-js-http-client-proof-provider';
import { NodeZkConfigProvider } from '@midnight-ntwrk/midnight-js-node-zk-config-provider';
import { levelPrivateStateProvider } from '@midnight-ntwrk/midnight-js-level-private-state-provider';
import { createMidnightProviderFromHandlers, createWalletProviderFromHandlers } from '@midnight-ntwrk/midnight-js-types';
import { ProtocolVersion, WalletTransaction } from '@midnightntwrk/wallet-sdk';
import { Either } from 'effect';
import path from 'node:path';
import { messagesOf } from './cli.mjs';
import { config, eras, log } from './config.mjs';
import { submit as submitWithTimeout, ttl as defaultTtl } from './wallet.mjs';

const unwrap = (handle, range) => {
  const result = WalletTransaction.unwrapWithin(handle, range);
  if (Either.isLeft(result)) throw new Error(`wallet refused the transaction: ${result.left?.message ?? String(result.left)}`);
  return result.right;
};

const hex = (key) => key?.toHexString?.() ?? key;

const walletArms = (facade, state, forks) => {
  const forkVersion = forks[eras.new];
  const sides = eras.each(
    { version: config.preForkSpec, epoch: ProtocolVersion.epochOf(config.preForkSpec, forkVersion) },
    { version: forkVersion, epoch: ProtocolVersion.epochOf(forkVersion, forkVersion) },
  );
  // The same ledger copies the wallet SDK runs, so the wallet takes the decoded objects.
  const ledgers = eras.ledgers();
  const decoders = eras.each(ledgers[eras.old].Transaction, ledgers[eras.new].Transaction);
  const adopt = (stage, tx) => WalletTransaction.adopt(stage, tx.deserialize(decoders), sides[tx.era].version);
  const balance = async (tx, ttl) => {
    const recipe = await facade.balanceUnboundTransaction(adopt('Unbound', tx), { ttl: ttl ?? defaultTtl() });
    return unwrap(await facade.finalizeRecipe(recipe), sides[tx.era].epoch);
  };
  // Midnight.js redacts a provider's error, so the node's verdict is logged here.
  const submit = (tx) =>
    submitWithTimeout(facade, adopt('Finalized', tx)).catch((err) => {
      for (const m of new Set(messagesOf(err))) log(`  submit refused: ${m.slice(0, 300)}`);
      throw err;
    });
  return {
    walletProvider: createWalletProviderFromHandlers({
      getCoinPublicKey: () => hex(state.shielded.coinPublicKey),
      getEncryptionPublicKey: () => hex(state.shielded.encryptionPublicKey),
      ...eras.each(balance),
    }),
    midnightProvider: createMidnightProviderFromHandlers(eras.each(submit)),
  };
};

export const publicDataProvider = () =>
  indexerPublicDataProvider({ queryURL: config.indexerHttp, subscriptionURL: config.indexerWs });

export const buildProviders = ({ facade, state, forks, contractDir, privateStateId }) => {
  // Artefacts of an older compiler may have no contract manifest and need MN_ZK_VERIFY=warn.
  const verify = process.env['MN_ZK_VERIFY'];
  const zkConfigProvider = new NodeZkConfigProvider(contractDir, verify ? { verify } : undefined);
  log(`midnight.js providers: indexer ${config.indexerHttp}, proof server ${config.proofServerNew}, zk ${contractDir}`);
  return {
    // The hour matches the wallet's own TTL.
    config: { networkId: config.networkId, ttlSeconds: 60 * 60 },
    publicDataProvider: publicDataProvider(),
    privateStateProvider: levelPrivateStateProvider({
      midnightDbName: path.join(config.outDir, 'midnight-level-db'),
      privateStateStoreName: `${privateStateId}-store`,
      accountId: 'hf-checks',
      privateStoragePasswordProvider: async () => 'Hf-Checks-Private-State-9',
    }),
    zkConfigProvider,
    proofProvider: httpClientProofProvider({ url: config.proofServerNew, zkConfigProvider }),
    ...walletArms(facade, state, forks),
  };
};
