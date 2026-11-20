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
// Midnight.js hands the wallet one arm per ledger era: a ledger-8 (retained-era) call
// arrives as bytes, a ledger-9 (current-era) one as a ledger object.

import { indexerPublicDataProvider } from '@midnight-ntwrk/midnight-js-indexer-public-data-provider';
import { httpClientProofProvider } from '@midnight-ntwrk/midnight-js-http-client-proof-provider';
import { NodeZkConfigProvider } from '@midnight-ntwrk/midnight-js-node-zk-config-provider';
import { levelPrivateStateProvider } from '@midnight-ntwrk/midnight-js-level-private-state-provider';
import { createMidnightProviderFromArms, createWalletProviderFromArms } from '@midnight-ntwrk/midnight-js-types';
import { ProtocolVersion, WalletTransaction } from '@midnightntwrk/wallet-sdk';
import * as ledgerV8 from '@midnight-ntwrk/ledger-v8';
import { Either } from 'effect';
import path from 'node:path';
import { config, log } from './config.mjs';
import { submit as submitWithTimeout, ttl as defaultTtl } from './wallet.mjs';

const unwrap = (handle, range) => {
  const result = WalletTransaction.unwrapWithin(handle, range);
  if (Either.isLeft(result)) throw new Error(`wallet refused the transaction: ${result.left?.message ?? String(result.left)}`);
  return result.right;
};

const hex = (key) => key?.toHexString?.() ?? key;

const walletArms = (facade, state, forks) => {
  const eras = {
    v8: { version: config.preForkSpec, epoch: ProtocolVersion.epochOf(config.preForkSpec, forks.v9) },
    v9: { version: forks.v9, epoch: ProtocolVersion.epochOf(forks.v9, forks.v9) },
  };
  const fromBytes = (txBytes) => ledgerV8.Transaction.deserialize(txBytes, config.networkId);
  const balance = async (era, tx, ttl) => {
    const handle = WalletTransaction.adopt('Unbound', tx, eras[era].version);
    const recipe = await facade.balanceUnboundTransaction(handle, { ttl: ttl ?? defaultTtl() });
    return unwrap(await facade.finalizeRecipe(recipe), eras[era].epoch);
  };
  const submit = (era, tx) => submitWithTimeout(facade, WalletTransaction.adopt('Finalized', tx, eras[era].version));
  return {
    walletProvider: createWalletProviderFromArms({
      getCoinPublicKey: () => hex(state.shielded.coinPublicKey),
      getEncryptionPublicKey: () => hex(state.shielded.encryptionPublicKey),
      currentEra: (tx, ttl) => balance('v9', tx, ttl),
      retainedEras: { v8: async (txBytes, ttl) => (await balance('v8', fromBytes(txBytes), ttl)).serialize(config.networkId) },
    }),
    midnightProvider: createMidnightProviderFromArms({
      currentEra: (tx) => submit('v9', tx),
      retainedEras: { v8: (txBytes) => submit('v8', fromBytes(txBytes)) },
    }),
  };
};

export const publicDataProvider = () =>
  indexerPublicDataProvider({ queryURL: config.indexerHttp, subscriptionURL: config.indexerWs });

export const buildProviders = ({ facade, state, forks, contractDir, privateStateId }) => {
  // compactc 0.31.x artefacts have no contract manifest and need MN_ZK_VERIFY=warn.
  const verify = process.env['MN_ZK_VERIFY'];
  const zkConfigProvider = new NodeZkConfigProvider(contractDir, verify ? { verify } : undefined);
  log(`midnight.js providers: indexer ${config.indexerHttp}, proof server ${config.proofServerV9}, zk ${contractDir}`);
  return {
    publicDataProvider: publicDataProvider(),
    privateStateProvider: levelPrivateStateProvider({
      midnightDbName: path.join(config.outDir, 'midnight-level-db'),
      privateStateStoreName: `${privateStateId}-store`,
      accountId: 'hf-checks',
      privateStoragePasswordProvider: async () => 'Hf-Checks-Private-State-9',
    }),
    zkConfigProvider,
    proofProvider: httpClientProofProvider({ url: config.proofServerV9, zkConfigProvider }),
    ...walletArms(facade, state, forks),
  };
};
