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
import * as allure from 'allure-js-commons'
import fs from 'fs'
import { test } from '@playwright/test'
import { ApiPromise, WsProvider } from '@polkadot/api'
import jsonrpc from '@polkadot/types/interfaces/jsonrpc'
import { AnyTuple } from '@polkadot/types-codec/types'
import { Extrinsic } from '@polkadot/types/interfaces'
import { IExtrinsic } from '@polkadot/types/types'
import { Commons } from '../../utils/Commons'
import logging from '../../utils/Logger'
import {
  TestContainersFixture,
} from 'TestContainersFixture'

const require = createRequire(import.meta.url)
const config = require('../../../src/config/common.json')
const __filename = fileURLToPath(import.meta.url)
const _logger = logging(__filename)
// Expected contract address given by the deploy transaction in the toolkit
const EXPECTED_CONTRACT_ADDRESS = `0x${fs.readFileSync(
  '../../res/test-contract/contract_address_undeployed.mn'
)}`

const INVALID_CONTRACT_ADDRESS = `0x${fs.readFileSync(
  '../../res/test-contract/contract_address_undeployed_invalid.mn'
)}`

let testFixture: Promise<TestContainersFixture>
let api: ApiPromise

test.describe('Substrate-powered Midnight Node basic tests', async () => {
  test.beforeAll(async () => {
    const providerUrl = `ws://localhost:${TestContainersFixture.NODE_PORT_WS}`;
    api = await ApiPromise.create({
      provider: new WsProvider(providerUrl),
      rpc: { ...jsonrpc, ...config.CUSTOM_RPC_METHODS },
    })
  })

  // test('Deploy a contract', async () => {
  //   await allure.tms('PM-6176')
  //   await allure.epic('Midnight Node')
  //   await allure.feature('Transactions')
  //   await allure.story('Contract Deployment')
  //   const deployTx = api.tx.midnight.sendMnTransaction(
  //     Commons.getTxTemplate('contract_tx_1_deploy_undeployed.mn')
  //   )
  //   await sendTransaction(deployTx, 'Deploy')
  //
  //   const events = await api.query.system.events()
  //
  //   const deployEvent = events.find(
  //     event => event.event.toHuman().method == 'ContractDeploy'
  //   )
  //   test.expect(deployEvent).toBeDefined
  //
  //   const contractAddress = deployEvent.event.data[0].contractAddress.toHuman()
  //
  //   test.expect(contractAddress).toEqual(EXPECTED_CONTRACT_ADDRESS)
  // })

  // test('Contract Call - Store', async () => {
  //   await allure.tms('PM-6178')
  //   await allure.epic('Midnight Node')
  //   await allure.feature('Transactions')
  //   await allure.story('Contract Calls')
  //
  //   const storeTx = api.tx.midnight.sendMnTransaction(
  //     Commons.getTxTemplate('contract_tx_2_store_undeployed.mn')
  //       .toString()
  //       .trimEnd()
  //   )
  //
  //   await sendTransaction(storeTx, 'Store')
  //
  //   const events = await api.query.system.events()
  //
  //   const ContractCallEvent = events.find(
  //     event => event.event.toHuman().method == 'ContractCall'
  //   )
  //   test.expect(ContractCallEvent).toBeDefined
  // })

  // test('Contract Call - Check', async () => {
  //   await allure.tms('PM-6178')
  //   await allure.epic('Midnight Node')
  //   await allure.feature('Transactions')
  //   await allure.story('Contract Calls')
  //
  //   const checkTx = api.tx.midnight.sendMnTransaction(
  //     Commons.getTxTemplate('contract_tx_3_check_undeployed.mn')
  //       .toString()
  //       .trimEnd()
  //   )
  //
  //   await sendTransaction(checkTx, 'Check')
  //
  //   const events = await api.query.system.events()
  //
  //   const ContractCallEvent = events.find(
  //     event => event.event.toHuman().method == 'ContractCall'
  //   )
  //
  //   test.expect(ContractCallEvent).toBeDefined
  // })

  // test('Contract Call - Maintenance', async () => {
  //   await allure.tms('PM-6178')
  //   await allure.epic('Midnight Node')
  //   await allure.feature('Transactions')
  //   await allure.story('Contract Calls')
  //
  //   const maintenanceTx = api.tx.midnight.sendMnTransaction(
  //     Commons.getTxTemplate('contract_tx_4_change_authority_undeployed.mn')
  //       .toString()
  //       .trimEnd()
  //   )
  //
  //   await sendTransaction(maintenanceTx, 'Maintenance')
  //
  //   const events = await api.query.system.events()
  //
  //   const ContractMaintainEvent = events.find(
  //     event => event.event.toHuman().method == 'ContractMaintain'
  //   )
  //
  //   test.expect(ContractMaintainEvent).toBeDefined
  //
  //   const contractAddress = ContractMaintainEvent.event.data[0].contractAddress.toHuman()
  //
  //   test.expect(contractAddress).toEqual(EXPECTED_CONTRACT_ADDRESS)
  // })

  test('midnight_ledgerVersion works correctly', async () => {
    await allure.tms('PM-11943')
    await allure.epic('Midnight Node')
    await allure.feature('Custom RPC API')
    await allure.story('midnight_ledgerVersion')

    const ledgerVersion = await api.rpc.midnight.ledgerVersion()

    test.expect(ledgerVersion.toHuman()).toEqual('ledger-6.0.0-alpha.3')
  })

  test('midnight_apiVersions works correctly', async () => {
    const apiVersion = await api.rpc.midnight.apiVersions()

    test.expect(apiVersion.toHuman()).toEqual('2')
  })

  test('midnight_contractState responds with error for an invalid contract address', async () => {
    try {
      await api.rpc.midnight.contractState(INVALID_CONTRACT_ADDRESS)
    } catch (error) {
      test.expect(error.code).toBe(-32602)
      test
        .expect(error.message)
        .toMatch(`-32602: Unable to decode contract address:`)
    }
  })

  test('malformed tx is rejected', async () => {
    await allure.tms('PM-6229')
    await allure.epic('Midnight Node')
    await allure.feature('Transactions')
    await allure.story('Malformed tx rejected')

    const events = await api.query.system.events()
    const malformedTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('malformed.mn', 'test-zswap')
        .toString()
        .trimEnd()
    )

    await test.expect(async () => {
      await api.rpc.author.submitAndWatchExtrinsic(malformedTx)
    }).rejects.toThrow("1010: Invalid Transaction: Custom error: 1")

    const Event = events.find(
      event => event.event.toHuman().method == 'TxApplied'
    )

    test.expect(Event).toBeUndefined
  })
})

// eslint-disable-next-line @typescript-eslint/explicit-function-return-type
async function sendTransaction(
  tx: string | Uint8Array | Extrinsic | IExtrinsic<AnyTuple>,
  txType: string
) {
  let blockHash: string | undefined

  // eslint-disable-next-line no-async-promise-executor
  const resultPromise = new Promise(async resolve => {
    const unsub = await api.rpc.author.submitAndWatchExtrinsic(tx, callback => {
      _logger.info(`${txType} transaction status: ${callback.type}`)

      if (callback.isInBlock) {
        _logger.info(
          `${txType} transaction included in block: ${callback.asInBlock}`
        )
        blockHash = callback.asInBlock.toString()
        resolve(callback)
        unsub()
      }
    })
  })

  await resultPromise
}
