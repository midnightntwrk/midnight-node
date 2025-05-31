// This script connects to a chain and attempts to update the onchain `sidechain.sidechainParams` with the values in a local file: `../config/addresses.json`

const { ApiPromise, WsProvider } = require("@polkadot/api");
const { Keyring } = require("@polkadot/keyring");
const { cryptoWaitReady, mnemonicGenerate } = require("@polkadot/util-crypto");
import fs from 'fs';
import path from 'path';

const SIDECHAIN_PARAMS_STORAGE_KEY = "0x3eaeb1cee77dc09baac326e5a1d297264a0b7b7cdd2d400cd454bedfa90d1f16";

// Load configuration from a JSON file
const configPath = path.resolve(__dirname, '../config/addresses.json');
const config = JSON.parse(fs.readFileSync(configPath, 'utf8'));

const {
  chainId,
  genesisCommitteeUtxo,
  thresholdNumerator,
  thresholdDenominator,
  governanceAuthority
} = config;

async function main() {
  await cryptoWaitReady();
  const wsProvider = new WsProvider("ws://localhost:9944");
  const api = await ApiPromise.create({
    provider: wsProvider
  });
  const keyring = new Keyring({ type: "sr25519" });

  // Dev account
  const alice = keyring.addFromUri('//Alice');

  console.log(`Encoding type based on values from JSON file in ${configPath}: ${JSON.stringify(config)}`);

  // Get codec of onchain `SidechainParams` struct in order to tell polkadotjs how to encode later
  const sidechainParams = api.registry.createType("ChainParamsSidechainParams", {
    chainId,
    genesisCommitteeUtxo,
    thresholdNumerator,
    thresholdDenominator,
    governanceAuthority
  });

  const updatedStorageValue = sidechainParams.toHex();

  console.log("Sending tx...");

  await api.tx.sudo
    .sudo(
      api.tx.system.setStorage([
        [SIDECHAIN_PARAMS_STORAGE_KEY, updatedStorageValue]
      ])
    )
    .signAndSend(alice);

  console.log("Sent sudo tx to update onchain address values")
  await api.disconnect();
}

main()
  .then(() => { })
  .catch(console.error)
  .finally(() => process.exit());