// This file is part of midnight-node.
// Copyright (C) 2025 Midnight Foundation
// SPDX-License-Identifier: Apache-2.0
// Licensed under the Apache License, Version 2.0 (the "License");
// You may not use this file except in compliance with the License.
// You may obtain a copy of the License at
// http://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

import { fileURLToPath } from 'url'
import { createRequire } from 'module'
import fs from 'fs'
import { test } from '@playwright/test'
import { ApiPromise, WsProvider } from '@polkadot/api'
import jsonrpc from '@polkadot/types/interfaces/jsonrpc'
import { Keyring } from '@polkadot/api';
import { blake2AsHex, cryptoWaitReady } from '@polkadot/util-crypto';
import logging from '../../utils/Logger'
import { Commons } from '../../utils/Commons'
import {
  TestContainersFixture,
  useTestContainersFixture,
} from 'TestContainersFixture'

const require = createRequire(import.meta.url)
const config = require('../../../src/config/common.json')

const __filename = fileURLToPath(import.meta.url)
const _logger = logging(__filename)

let testFixture: Promise<TestContainersFixture>
let api: ApiPromise

const DOCKER_COMPOSE_LOCATION = 'docker/fork-test-compose.yml'
let keyring: Keyring

test.describe('Midnight Fork tests', async () => {
  test.beforeEach(async () => {
    test.setTimeout(3_900_000)
    testFixture = useTestContainersFixture(DOCKER_COMPOSE_LOCATION)

    const _fixtureReady = (await testFixture).getNodeWs();
    const providerUrl = `ws://localhost:${TestContainersFixture.NODE_PORT_WS}`;

    api = await ApiPromise.create({
      provider: new WsProvider(providerUrl),
      rpc: { ...jsonrpc, ...config.CUSTOM_RPC_METHODS },
    });

    cryptoWaitReady().then(() => {
      keyring = new Keyring({ type: 'sr25519' });
    });
  })


  test('Tests full fork upgrade process', async () => {
    const currentVersion = await api.query.system.lastRuntimeUpgrade();

    const WASM_PATH = process.env.WASM_PATH;
    const wasmBytes = fs.readFileSync(WASM_PATH);
    const wasmHash = blake2AsHex(wasmBytes);

    test.expect(wasmHash).toEqual(process.env.PROPOSED_RUNTIME_HASH)


    keyring.setSS58Format(42)
    const pair = keyring.addFromUri("//Alice", { type: 'sr25519' })
    const address = pair.address;
    const currentSudo = await api.query.sudo.key();
    test.expect(address).toEqual(currentSudo.toHuman())

    const preimageRequestTx = api.tx.preimage.requestPreimage(wasmHash);
    const sudoPreimageRequestTx = api.tx.sudo.sudo(preimageRequestTx);
    await sudoPreimageRequestTx.signAndSend(pair)
    await Commons.sleep(6200);

    const wasmFile = fs.readFileSync(WASM_PATH);
    const wasmHex = `0x${wasmFile.toString('hex')}`;
    const preimageTx = api.tx.preimage.notePreimage(wasmHex);

    await preimageTx.signAndSend(pair);

    const checkUpgradeStatus = async () => {
      const newVersion = await api.query.system.lastRuntimeUpgrade();
      if (newVersion.toString() !== currentVersion.toString()) {
        console.log('Runtime has been upgraded to version:', newVersion.toHuman());
        return true;
      }
      return false;
    };

    const startingSession = await api.query.session.currentIndex();
    let session = startingSession;

    while (session < 5) {
      console.log("New Session is ", session)
      const upgraded = await checkUpgradeStatus();
      console.log("Upgrade status", upgraded)

      if (session > 0 && upgraded) {
        console.log("Upgraded successfully")
        break
      };
      session = await api.query.session.currentIndex();

      await api.rpc.chain.getFinalizedHead();
      await Commons.sleep(60000);
    }
  });

})
