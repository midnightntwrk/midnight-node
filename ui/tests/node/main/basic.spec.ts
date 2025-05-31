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
  useTestContainersFixture,
} from 'TestContainersFixture'
import { CONTRACT_JSON_RESPONSE } from 'fixtures/contractData'
import { EXPECTED_ZSWAP_CHAIN_STATE } from 'fixtures/zswapChainState'
import { EXPECTED_CONTRACT_STATE } from 'fixtures/contractState'

const require = createRequire(import.meta.url)
const config = require('../../../src/config/common.json')
const __filename = fileURLToPath(import.meta.url)
const _logger = logging(__filename)
// Expected contract address given by the deploy transaction in the generator
const EXPECTED_CONTRACT_ADDRESS = `0x${fs.readFileSync(
  '../../res/test-contract/contract_address_undeployed.mn'
)}`

const INVALID_CONTRACT_ADDRESS = `0x${fs.readFileSync(
  '../../res/test-contract/contract_address_undeployed_invalid.mn'
)}`

// Identifier for 'store' contract operation
const STORE_TRANSACTION_OPERATION = '73746f7265'

let testFixture: Promise<TestContainersFixture>
let api: ApiPromise

test.describe('Substrate-powered Midnight Node basic tests', async () => {
  test.beforeEach(async () => {
    test.setTimeout(480_000)
    testFixture = useTestContainersFixture('./docker/test-compose.yml')
    api = await ApiPromise.create({
      provider: new WsProvider((await testFixture).getNodeWs()),
      rpc: { ...jsonrpc, ...config.CUSTOM_RPC_METHODS },
    })
  })

  test.afterEach(async () => {
    await api.disconnect()
    test.setTimeout(120_000)
    ;(await testFixture).down()
  })

  test('deploys and checks contract state', async () => {
    await allure.tms('PM-6176')
    await allure.epic('Midnight Node')
    await allure.feature('Transactions')
    await allure.story('Contract Deployment')
    const deployTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_1_deploy_undeployed.mn')
    )
    const deployBlock = await sendTransaction(deployTx, 'Deploy')

    const deployExtrinsic = deployBlock.body.find(extrinsic => {
      try {
        const operations = extrinsic.MidnightTransaction.tx.operations
        return operations.some(op => JSON.stringify(op).includes('Deploy'))
      } catch (e) {
        return false
      }
    })
    test.expect(deployExtrinsic).toBeDefined()

    const events = await api.query.system.events()

    const deployEvent = events.find(
      event => event.event.toHuman().method == 'ContractDeploy'
    )
    test.expect(deployEvent).toBeDefined

    const contractAddress = deployEvent.event.data[0].contractAddress.toHuman()
    test.expect(contractAddress).toEqual(EXPECTED_CONTRACT_ADDRESS)

    const contractAddressNoPrefix = contractAddress.substring(2)

    // Get contract state by querying by address (stripped of "0x" hex prefix)
    const contractState = await api.rpc.midnight.contractState(
      contractAddressNoPrefix
    )
    // TODO: Need to bring in ledger code, and try deserializing the contract state
    // FIXME: Disabling this test until it's more robust and tests something more than a comparison
    // test.expect(contractState.toHuman()).toEqual(EXPECTED_CONTRACT_STATE)
  })

  test('deploys and checks contract json state', async () => {
    const deployTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_1_deploy_undeployed.mn')
        .toString()
        .trimEnd()
    )
    const deployBlock = await sendTransaction(deployTx, 'Deploy')

    const deployExtrinsic = deployBlock.body.find(extrinsic => {
      try {
        const operations = extrinsic.MidnightTransaction.tx.operations
        return operations.some(op => JSON.stringify(op).includes('Deploy'))
      } catch (e) {
        return false
      }
    })

    test.expect(deployExtrinsic).toBeDefined()

    const events = await api.query.system.events()

    const deployEvent = events.find(
      event => event.event.toHuman().method == 'ContractDeploy'
    )
    test.expect(deployEvent).toBeDefined

    const contractAddress = deployEvent.event.data[0].contractAddress.toHuman()
    test.expect(contractAddress).toEqual(EXPECTED_CONTRACT_ADDRESS)

    const contractAddressNoPrefix = contractAddress.substring(2)

    // Get contract state by querying by address (stripped of "0x" hex prefix)
    const contractState = await api.rpc.midnight.jsonContractState(
      contractAddressNoPrefix
    )
    // TODO: Need to bring in ledger code, and try deserializing the contract state
    // FIXME: Disabling this test until it's more robust and tests something more than a comparison
    // test.expect(contractState.toHuman()).toEqual(CONTRACT_JSON_RESPONSE)
  })

  test('deploys and checks Zswap chain state', async () => {
    const deployTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_1_deploy_undeployed.mn')
        .toString()
        .trimEnd()
    )
    const deployBlock = await sendTransaction(deployTx, 'Deploy')

    test
      .expect(
        deployBlock.body.some(extrinsic => {
          try {
            const operations = extrinsic.MidnightTransaction.tx.operations
            return operations.some(op => JSON.stringify(op).includes('Deploy'))
          } catch (e) {
            return false
          }
        })
      )
      .toBe(true)

    const events = await api.query.system.events()

    const deployEvent = events.find(
      event => event.event.toHuman().method == 'ContractDeploy'
    )
    test.expect(deployEvent).toBeDefined

    const contractAddress = deployEvent.event.data[0].contractAddress.toHuman()
    test.expect(contractAddress).toEqual(EXPECTED_CONTRACT_ADDRESS)

    const contractAddressNoPrefix = contractAddress.substring(2)

    const zswapChainState = await api.rpc.midnight.zswapChainState(
      contractAddressNoPrefix
    )

    // FIXME: Disabling this test until it's more robust and tests something more than a comparison
    // test.expect(zswapChainState.toHuman()).toEqual(EXPECTED_ZSWAP_CHAIN_STATE)

    api.disconnect()
  })

  test('deploys and checks Zswap state root has changed', async () => {
    await allure.tms('PM-15320')
    await allure.epic('Midnight Node')
    await allure.feature('Transactions')
    await allure.story('Zswap State Root')
    const zswapStateRoot = await api.rpc.midnight.zswapStateRoot()
    const initialRoot = zswapStateRoot.toHuman();

    const deployTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_1_deploy_undeployed.mn')
        .toString()
        .trimEnd()
    )
    const deployBlock = await sendTransaction(deployTx, 'Deploy')

    test
      .expect(
        deployBlock.body.some(extrinsic => {
          try {
            const operations = extrinsic.MidnightTransaction.tx.operations
            return operations.some(op => JSON.stringify(op).includes('Deploy'))
          } catch (e) {
            return false
          }
        })
      )
      .toBe(true)

    const events = await api.query.system.events()

    const deployEvent = events.find(
      event => event.event.toHuman().method == 'ContractDeploy'
    )
    test.expect(deployEvent).toBeDefined

    const zswapStateRootNew = await api.rpc.midnight.zswapStateRoot()
    test.expect(zswapStateRootNew.toHuman()).not.toEqual(initialRoot)
  })

  test('Deploy > Store > Check > Maintain transaction sequence', async () => {
    await allure.tms('PM-6178')
    await allure.epic('Midnight Node')
    await allure.feature('Transactions')
    await allure.story('Contract Calls')
    const deployTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_1_deploy_undeployed.mn')
        .toString()
        .trimEnd()
    )
    const deployBlock = await sendTransaction(deployTx, 'Deploy')

    test
      .expect(
        deployBlock.body.some(extrinsic => {
          try {
            const operations = extrinsic.MidnightTransaction.tx.operations
            return operations.some(op => JSON.stringify(op).includes('Deploy'))
          } catch (e) {
            return false
          }
        })
      )
      .toBe(true)

    const storeTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_2_store_undeployed.mn')
        .toString()
        .trimEnd()
    )

    const storeBlock = await sendTransaction(storeTx, 'Store')

    test
      .expect(
        storeBlock.body.some(extrinsic => {
          try {
            const operations = extrinsic.MidnightTransaction.tx.operations
            return operations.some(
              op =>
                op.Call &&
                op.Call.entry_point &&
                op.Call.entry_point.includes(STORE_TRANSACTION_OPERATION)
            )
          } catch (e) {
            return false
          }
        })
      )
      .toBe(true)

    const checkTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_3_check_undeployed.mn')
        .toString()
        .trimEnd()
    )

    const checkBlock = await sendTransaction(checkTx, 'Check')

    test
      .expect(
        checkBlock.body.some(extrinsic => {
          try {
            const operations = extrinsic.MidnightTransaction.tx.operations
            return operations.some(
              op =>
                op.Call &&
                op.Call.entry_point &&
                op.Call.entry_point.includes('636865636b')
            )
          } catch (e) {
            return false
          }
        })
      )
      .toBe(true)

    const maintenanceTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate('contract_tx_4_change_authority_undeployed.mn')
        .toString()
        .trimEnd()
    )

    const contractAddressNoPrefix = EXPECTED_CONTRACT_ADDRESS.substring(2)

    const maintenanceBlock = await sendTransaction(maintenanceTx, 'Maintenance')
    test.expect(
      maintenanceBlock.body.some(extrinsic => {
        try {
          const operations = extrinsic.MidnightTransaction.tx.operations;
          return operations.some(op => op.Maintain.address.includes(contractAddressNoPrefix))
        } catch (e) {
          return false;
        }
      })
    ).toBe(true);
  })

  test('midnight_ledgerVersion works correctly', async () => {
    await allure.tms('PM-11943')
    await allure.epic('Midnight Node')
    await allure.feature('Custom RPC API')
    await allure.story('midnight_ledgerVersion')

    const ledgerVersion = await api.rpc.midnight.ledgerVersion()

    test.expect(ledgerVersion.toHuman()).toEqual('feature/unshielded-ledger')
  })

  test('midnight_apiVersions works correctly', async () => {
    const apiVersion = await api.rpc.midnight.apiVersions()

    test.expect(apiVersion.toHuman()).toEqual('2')
  })

  test('midnight_jsonBlock works correctly', async () => {
    await allure.tms('PM-6247')
    await allure.epic('Midnight Node')
    await allure.feature('Custom RPC API')
    await allure.story('midnight_jsonBlock')

    const genesis = await api.rpc.chain.getBlockHash(0)
    const apiResponse = await api.rpc.midnight.jsonBlock(genesis.toU8a())
    const inner = JSON.parse(apiResponse.toJSON())

    test.expect(Object.keys(inner.body[0])).toContain('MidnightTransaction')
  })

  test('midnight_jsonContractState responds with error for an invalid contract address', async () => {
    try {
      await api.rpc.midnight.jsonContractState(INVALID_CONTRACT_ADDRESS)
    } catch (error) {
      test.expect(error.code).toBe(-32602)
      test
        .expect(error.message)
        .toMatch(`-32602: Unable to decode contract address:`)
    }
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
    test.expect(events.length).toBe(0)
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
  return JSON.parse(await api.rpc.midnight.jsonBlock(blockHash))
}
