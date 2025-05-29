import { fileURLToPath } from 'url'
import { createRequire } from 'module'
import * as allure from "allure-js-commons";
import fs from 'fs'
import { test } from '@playwright/test'
import { ApiPromise, WsProvider } from '@polkadot/api'
import jsonrpc from '@polkadot/types/interfaces/jsonrpc'
import { AnyTuple } from '@polkadot/types-codec/types'
import { Extrinsic } from '@polkadot/types/interfaces'
import { IExtrinsic } from '@polkadot/types/types'
import { Commons } from '../../utils/Commons'
import logging from '../../utils/Logger'
import { exit } from 'process'


const require = createRequire(import.meta.url)
const config = require('../../../src/config/common.json')
const __filename = fileURLToPath(import.meta.url)
const logger = logging(__filename)
let networkId: string;
if (process.env.TEST_ENV === undefined) {  
  logger.info('TEST_ENV env var not set');
  exit(1);
}
if (process.env.WS_URL === undefined) {  
  logger.info('WS_URL env var not set');
  exit(1);
}
switch (process.env.TEST_ENV) {
  case 'node-dev-01':
  case 'qanet':
    networkId = 'devnet';
    break;
  case 'testnet-02':
    networkId = 'testnet';
    break;
  default:
    networkId = 'undeployed';
    break;
}
// Expected contract address given by the deploy transaction in the generator
const EXPECTED_CONTRACT_ADDRESS = `0x${fs.readFileSync(
  `../../res/test-contract/contract_address_${networkId}.mn`
)}`

const INVALID_CONTRACT_ADDRESS = `0x${fs.readFileSync(
  `../../res/test-contract/contract_address_${networkId}_invalid.mn`
)}`

// Identifier for 'store' contract operation
const STORE_TRANSACTION_OPERATION= "73746f7265";

let api: ApiPromise

test.describe('Substrate-powered Midnight Node basic tests', async () => {
  test.beforeEach(async () => {
    test.setTimeout(480_000)
    api = await ApiPromise.create({
      provider: new WsProvider(process.env.WS_URL),
      rpc: { ...jsonrpc, ...config.CUSTOM_RPC_METHODS },
    })
  })

  test.afterEach(async () => {
    api.disconnect();
    test.setTimeout(120_000);
  })

   test('deploys and checks contract state, json state, zswap chain state, and zswap state root', async () => {
    test.skip(process.env.TEST_ENV === 'testnet-02', 'Latest node not deployed');
    const zswapStateRoot = await api.rpc.midnight.zswapStateRoot()
    const initialRoot = zswapStateRoot.toHuman();

    const deployTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate(`contract_tx_1_deploy_${networkId}.mn`).toString().trimEnd()
    )
    const deployBlock = await sendTransaction(deployTx, 'Deploy')

    const deployExtrinsic = deployBlock.body.find((extrinsic) => {
      try {
        const operations = extrinsic.MidnightTransaction.tx.operations;
        return operations.some(op => JSON.stringify(op).includes('Deploy'));
      } catch (e) {
        return false;
      }
    });
    test.expect(deployExtrinsic).toBeDefined();
  
    const events = await api.query.system.events()

    const deployEvent = events.find(
      event => event.event.toHuman().method == 'ContractDeploy'
    )
    test.expect(deployEvent).toBeDefined

    const contractAddress = deployEvent.event.data[0].contractAddress.toHuman()
    test.expect(contractAddress).toEqual(EXPECTED_CONTRACT_ADDRESS)

    const zswapStateRootNew = await api.rpc.midnight.zswapStateRoot()
    test.expect(zswapStateRootNew.toHuman()).not.toEqual(initialRoot)
  })

  test('Deploy > Store > Check > Maintain transaction sequence', async () => {
    const storeTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate(`contract_tx_2_store_${networkId}.mn`).toString().trimEnd()
    )

    const storeBlock = await sendTransaction(storeTx, 'Store')

    test.expect(
      storeBlock.body.some(extrinsic => {
        try {
          const operations = extrinsic.MidnightTransaction.tx.operations;
          return operations.some(
            op =>
              op.Call &&
              op.Call.entry_point &&
              op.Call.entry_point.includes(STORE_TRANSACTION_OPERATION)
          );
        } catch (e) {
          return false;
        }
      })
    ).toBe(true);

    const checkTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate(`contract_tx_3_check_${networkId}.mn`).toString().trimEnd()
    )

    const checkBlock = await sendTransaction(checkTx, 'Check')

    test.expect(
      checkBlock.body.some(extrinsic => {
        try {
          const operations = extrinsic.MidnightTransaction.tx.operations;
          return operations.some(
            op =>
              op.Call &&
              op.Call.entry_point &&
              op.Call.entry_point.includes('636865636b')
          );
        } catch (e) {
          return false;
        }
      })
    ).toBe(true);

    const maintenanceTx = api.tx.midnight.sendMnTransaction(
      Commons.getTxTemplate(`contract_tx_4_change_authority_${networkId}.mn`)
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

    test.expect(ledgerVersion.toHuman()).toEqual('ledger-3.0.6')
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

  test('sidechainParams returns unchanged data', async () => {
    await allure.epic('Midnight Node')
    await allure.feature('Sidechain storage')
    await allure.story('sidechain.sidechainParams')

    const queryResponse = await api.query.sidechain.sidechainParams()
    test.expect(JSON.stringify(queryResponse)).toMatchSnapshot(`${process.env.TEST_ENV}-sidechainParams.json`)
  })

  test('mainChainScriptsConfiguration returns unchanged data', async () => {
    await allure.epic('Midnight Node')
    await allure.feature('SessionCommitteeManagement storage')
    await allure.story('sessionCommitteeManagement.mainChainScriptsConfiguration')

    const queryResponse = await api.query.sessionCommitteeManagement.mainChainScriptsConfiguration()
    test.expect(JSON.stringify(queryResponse)).toMatchSnapshot(`${process.env.TEST_ENV}-mainChainScriptsConfiguration.json`)
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

      const malformedTx = api.tx.midnight.sendMnTransaction(
        Commons.getTxTemplate('malformed.mn', 'test-zswap')
          .toString()
          .trimEnd()
      )
    
      await test.expect(async () => {
        await api.rpc.author.submitAndWatchExtrinsic(malformedTx)
      }).rejects.toThrow("1010: Invalid Transaction: Custom error: 1")
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
      logger.info(`${txType} transaction status: ${callback.type}`)

      if (callback.isInBlock) {
        logger.info(
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
