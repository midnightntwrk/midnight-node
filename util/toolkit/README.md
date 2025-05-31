# Toolkit

This toolkit works with or without transaction proofs:

- `cargo build -r -p mn-node-toolkit`
- `cargo build -r -p mn-node-toolkit --features erase-proof`

## Usage

### Generate Transactions

The `TxGenerator` is composed of four main components: `Source`, `Builder`, `Destination`, and `Prover`.

- **`Source`**: Determines where the `NetworkId` is selected and queries existing transactions to be applied to the local `LedgerState` before generating new transactions. Sources can be either a JSON file or a chain, selected via the following flags:
  - `--src-files <file_path>`
  - `--src-url <chain_url>` (defaults to `ws://127.0.0.1:9944`)

- **`Builder`**: Specifies how transactions are built. There are six builder subcommands:
  - `send`: Pass-through mode for sending transactions from a JSON file (`DoNothingBuilder`)
  - `migrate`: Migrates transactions between chains (`ReplaceInitialTxBuilder`)
  - `batches`: Generates ZSwap & Unshielded Utxos transaction batches (`BatcherBuilder`)
  - `claim-mint`: Builds claim mint transactions (`ClaimMintBuilder`)
  - `contract-calls deploy`: Builds contract deployment transactions (`ContractDeployBuilder`)
  - `contract-calls maintenance`: Builds contract maintenance transactions (`ContractMaintenanceBuilder`)
  - `contract-calls call`: Builds general contract call transactions (`ContractCallBuilder`)

- **`Destination`**: Specifies where the generated transactions will be sent (either a file or a chain). Use:
  - `--dest-file <file_path>` (use `--to-bytes` to specify whether to save in JSON or bytes)
  - `--dest-url <chain_url>` (defaults to `ws://127.0.0.1:9944`)

- **`Prover`**: Chooses which proof server to use — either local (`LocalProofServer`) or remote (`RemoteProveServer`).

This enables four combinations of querying and sending transactions:

- <u>File to File</u>: Apply transformations and save back to a file.
- <u>File to Chain</u>: Read from a file, build new transactions, and send to a chain.
- <u>Chain to File</u>: Read from a chain, build new transactions, and save to a file.
- <u>Chain to Chain</u>: Read from a chain, build new transactions, and send to a chain.

Use the `-h` flag for full usage information.

**NOTE**: `ClaimMintBuilder`, `ContractDeployBuilder`, `ContractMaintenanceBuilder`, and `ContractCallBuilder` will be replaced by `FromYamlBuilder` once [PM-10459](https://shielded.atlassian.net/browse/PM-10459) is implemented.

#### Generate Zswap & Unshielded Utxos batches
- Query from chain, generate, and send to chain:
```shell
mn-node-toolkit generate-txs batches -n <num_txs_per_batch> -b <num_batches>
```
- Query from file, generate, and send to file:
```shell
mn-node-toolkit generate-txs --dest-file txs.json batches -n <num_txs_per_batch> -b <num_batches>
```
- Query from file and send to chain:
```shell
mn-node-toolkit generate-txs --src-files txs.json --dest-url ws://127.0.0.1:9944 send
```

#### Generate Deploy Contract
- Query from chain, generate, and send to chain:
```shell
mn-node-toolkit generate-txs contract-calls deploy --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
```
- Query from chain, generate, and send to bytes file:
```shell
mn-node-toolkit generate-txs --src-files res/genesis/genesis_tx_undeployed.mn --dest-file deploy.mn --to-bytes contract-calls deploy --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
```
- Query from file, generate, and send to bytes file:
```shell
mn-node-toolkit generate-txs --dest-file deploy.mn --to-bytes contract-calls deploy --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
```

### Generate Contract Address
Shows and saves in a `--dest-file` the contract address found in a provided tx in `--src-file`
```shell
mn-node-toolkit contract-address --network undeployed --src-file ./res/test-contract/contract_tx_1_deploy_undeployed.mn --dest-file /out/contract_adress_undeployed.mn
```

#### Generate Maintenance Update
- Query from chain, generate, and send to chain:
```shell
mn-node-toolkit generate-txs contract-calls maintenance --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' --contract-address <contract_address_file>
```
Rest of examples similar to Generate Deploy Contract

#### Generate Contract Call
- Query from chain, generate, and send to chain:
```shell
mn-node-toolkit generate-txs contract-calls call --call-key <call_key> --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' --contract-address <contract_address_file>
```
Rest of examples similar to Generate Deploy Contract

### Generate Genesis
```shell
mn-node-toolkit generate-genesis --network <network_name>
```

### Show Transaction
Show deserialized result of a single transaction. Two options:
- Tx saved as hex string
- Tx saved as bytes: use `--from-bytes` flag if the tx is saved in a file as bytes
```shell
mn-node-toolkit show-transaction --network tesnet --src-file ./res/test-tx-deserialize/tx-testnet.mn
```

### Show Wallet
```shell
mn-node-toolkit show-wallet --seed 0000000000000000000000000000000000000000000000000000000000000001
```

### Show Address
```shell
mn-node-toolkit show-address --network undeployed 0000000000000000000000000000000000000000000000000000000000000001
```

### Show Address Legacy
```shell
mn-node-toolkit show-address-legacy --network undeployed 0000000000000000000000000000000000000000000000000000000000000001
```

## Development
### Add a new Builder
- Create a new builder struct under `util/toolkit/src/tx_generator/builder/builders` that implements `BuildTxs` trait.
- Add a new subcommand to `enum Builder` and handle the new variant in `TxGenerator::builder()` method.

### Add a new Contract
- Create a new contract struct under `ledger/helpers/src/contract/contracts` that implements `Contract<D>` trait.

## Docker
### How to build the Docker image

```shell
# Run from the repo root
cd ../..

# Build the Docker image
earthly +generator-image
```

### Tips for running on Docker

To access a node running on localhost, use the `--network option`. To write output files to your host system,
use `-v /host/path:/container/path`. Example:

```shell
docker run --network host -v $(pwd):/out mn-generator2 generate-zswap -n 1 -f /out/tx.json
```

**NOTE:** if you're running through Docker and want to access a node on localhost, use: `docker run --network host ...`
