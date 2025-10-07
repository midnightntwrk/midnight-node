# Toolkit

This toolkit works with or without transaction proofs:

- `cargo build -r -p midnight-node-toolkit`
- `cargo build -r -p midnight-node-toolkit --features erase-proof`

## Usage

### Check Version information

To see compatibility with Node, Ledger, and Compactc versions, use the `version` command:

```shell
$ midnight-node-toolkit version
Node: 0.16.2
Ledger: ledger-6.1.0-alpha.2
Compactc: 0.25.103-rc.1-UT-ledger6
```

### Generate Transactions

Since the introduction of

The `TxGenerator` is composed of four main components: `Source`, `Destination`, `Prover`, `Builder`.

The order the arguments are declared when building the command matters. `Builder`'s specific ones should go at the end, after its subcommand.

Example:
```
midnight-node-toolkit generate-txs <SRC_ARGS> <DEST_ARGS> <PROVER_ARG> batches <BUILDER_ARGS>
```

- **`Source`**: Determines where the `NetworkId` is selected and queries existing transactions to be applied to the local `LedgerState` before generating new transactions. Sources can be either a JSON file or a chain, selected via the following flags:
  - `--src-files <file_path>`
  - `--src-url <chain_url>` (defaults to `ws://127.0.0.1:9944`)

- **`Destination`**: Specifies where the generated transactions will be sent (either a file or a chain). Use:
  - `--dest-file <file_path>` (use `--to-bytes` to specify whether to save in JSON or bytes)
  - `--dest-url <chain_url>` (defaults to `ws://127.0.0.1:9944`)
    - Supports multiple urls:
      - `--dest-url="ws://127.0.0.1:9944" --dest-url="ws://127.0.0.1:9933" --dest-url="ws://127.0.0.1:9922"`
      - `--dest-url=ws://127.0.0.1:9944 --dest-url=ws://127.0.0.1:9933 --dest-url="ws://127.0.0.1:9922"`
      - `--dest-url="ws://127.0.0.1:9944, ws://127.0.0.1:9933, ws://127.0.0.1:9922"`

- **`Prover`**: Chooses which proof server to use — either local (`LocalProofServer`) or remote (`RemoteProveServer`).

- **`Builder`**: Specifies how transactions are built. There are six builder subcommands:
  - `send`: Pass-through mode for sending transactions from a JSON file (`DoNothingBuilder`)
  - `single-tx`: Send a single transaction funded by a single wallet to N destination wallets (supports shielded and unshielded) (`SingleTxBuilder`)
  - `migrate`: Migrates transactions between chains (`ReplaceInitialTxBuilder`)
  - `batches`: Generates ZSwap & Unshielded Utxos transaction batches (`BatcherBuilder`)
  - `claim-mint`: Builds claim mint transactions (`ClaimMintBuilder`)
  - `contract-calls deploy`: Builds contract deployment transactions (`ContractDeployBuilder`)
  - `contract-calls maintenance`: Builds contract maintenance transactions (`ContractMaintenanceBuilder`)
  - `contract-calls call`: Builds general contract call transactions (`ContractCallBuilder`)

This enables four combinations of querying and sending transactions:

- **File to File**: Apply transformations and save back to a file.
- **File to Chain:** Read from a file, build new transactions, and send to a chain.
- **Chain to File:** Read from a chain, build new transactions, and save to a file.
- **Chain to Chain:** Read from a chain, build new transactions, and send to a chain.

Use the `-h` flag for full usage information.

**NOTE 1**
Since the introduction of the Ledger's `ReplayProtection` mechanism, the `TxGenerator` reads and send `TransactionWithContext` instead of `Transaction`. The reason is now it is necessary to know the `BlockContext` a transaction is valid.

If the user needs to know the `Transaction` value, it can make use of the command [`get-tx-from-context`](#) using as `--src-file` the previously generated `TransactionWithContext`.

**NOTE 2**: `ClaimMintBuilder`, `ContractDeployBuilder`, `ContractMaintenanceBuilder`, and `ContractCallBuilder` will be replaced by `FromYamlBuilder` onces [PM-10459](https://shielded.atlassian.net/browse/PM-10459) is implemented.

#### Generate Zswap & Unshielded Utxos batches
- Query from chain, generate, and send to chain:
```shell
midnight-node-toolkit generate-txs batches -n <num_txs_per_batch> -b <num_batches>
```
- Query from file, generate, and send to file:
```shell
midnight-node-toolkit generate-txs --dest-file txs.json batches -n <num_txs_per_batch> -b <num_batches>
```
- Query from file and send to chain with rate control:
```shell
midnight-node-toolkit generate-txs -r <tps> --src-files txs.json --dest-url ws://127.0.0.1:9944 send
```

#### Send a single transaction

- Query from local chain, generate with two unshielded outputs and one shielded output, send to local chain
```
midnight-node-toolkit generate-txs single-tx --shielded-amount 100 --unshielded-amount 5 --source-seed "0000000000000000000000000000000000000000000000000000000000000001" --destination-address mn_shield-addr_undeployed14gxh... --destination-address mn_addr_undeployed1g9nr3... --destination-address mn_addr_undeployed12vv6y...
```

#### Generate Deploy Contract (Built-in)

**Note:** These commands use a simple test contract built into the toolkit. For custom contracts, see the **Custom Contracts** section below

- Query from chain, generate, and send to chain:
```shell
midnight-node-toolkit generate-txs contract-calls deploy --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
```
- Query from chain, generate, and send to bytes file:
```shell
midnight-node-toolkit generate-txs --src-files res/genesis/genesis_tx_undeployed.mn --dest-file deploy.mn --to-bytes contract-calls deploy --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
```
- Query from file, generate, and send to bytes file:
```shell
midnight-node-toolkit generate-txs --dest-file deploy.mn --to-bytes contract-calls deploy --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
```
- Query fom chain, generate, and save as a serialized intent file:
```shell
midnight-node-toolkit generate-sample-intent --dest-dir "artifacts/intents" deploy --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
```
- Using the [toolkit-js](../toolkit-js), generate the deploy intent file:
  * The contract must have been compiled using `compact`. For this example, the contract is found in `util/toolkit-js/test/contract/managed`
  * Also, `toolkit-js` should already be built, and be specified either via the `--toolkit_js_path` argument, or the `TOOLKIT_JS_PATH' environment
    * export TOOLKIT_JS_PATH="util/toolkit-js"
```shell
midnight-node-toolkit generate-intent  -c util/toolkit-js/test/contract/contract.config.ts -C util/toolkit-js/test/contract/managed deploy
```

#### Generate Maintenance Update (Built-in)

**Note:** These commands use a simple test contract built into the toolkit. For custom contracts, see the **Custom Contracts** section below

- Query from chain, generate, and send to chain:
```shell
midnight-node-toolkit generate-txs contract-calls maintenance --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' --contract-address <contract_address>
```
- Query fom chain, generate, and save as a serialized intent file:
```shell
midnight-node-toolkit generate-sample-intent --dest-dir "artifacts/intents" maintenance --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' --contract-address <contract_address>
```
Rest of examples similar to Generate Deploy Contract

#### Generate Contract Call (Built-in)

**Note:** These commands use a simple test contract built into the toolkit. For custom contracts, see the **Custom Contracts** section below

- Query from chain, generate, and send to chain:
```shell
midnight-node-toolkit generate-txs contract-calls call --call-key <call_key> --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' --contract-address <contract_address>
```
- Query fom chain, generate, and save as a serialized intent file:
```shell
midnight-node-toolkit generate-sample-intent --dest-dir "artifacts/intents" call --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' --contract-address <contract_address>
```
Rest of examples similar to Generate Deploy Contract

#### Custom Contracts

The custom contract calls make use of **toolkit-js**. The nodejs `node` executable must be on the path, and a compiled version of toolkit js must be referenced by the `TOOLKIT_JS_PATH` environment variable for the following commands to work (if you're using the toolkit in a Docker container, this is done for you)

When compiling contracts, you **must** use the correct `compactc` version. To check compatibility, run `midnight-node-toolkit version`

- Get `coin-public-key` for a seed. Toolkit commands will accept tagged or untagged
```shell
midnight-node-toolkit show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --coin-public
```

- Generate a deploy intent
```shell
compactc counter.compact toolkit-js/contract/out # Compile your contract - compiled directory must be a child of $TOOLKIT_JS_PATH

midnight-node-toolkit generate-intent deploy \
    -c toolkit-js/contract/contract.config.ts \
    --coin-public <coin-public-key for caller> \
    --output-intent "/out/deploy.bin" \
    --output-private-state "/out/initial_private_state.json \
    --output-zswap-state "/out/$deploy_zswap_filename"
```

- Generate a tx from an intent
```shell
midnight-node-toolkit send-intent --intent-file "/out/deploy.bin" --compiled-contract-dir contract/counter/out --to-bytes --dest-file "/out/deploy_tx.mn"
```

- Generate and send a tx from an intent
```shell
midnight-node-toolkit send-intent --intent-file "/out/deploy.bin" --compiled-contract-dir contract/counter/out
```

- Get the contract address
```shell
midnight-node-toolkit contract-address --src-file /out/deploy_tx.mn --network undeployed
```

- Get the contract address (untagged)
```shell
midnight-node-toolkit contract-address --src-file /out/deploy_tx.mn --network undeployed --untagged
```

- Get the contract on-chain state
```shell
midnight-node-toolkit contract-state --contract-address <contract-address> --dest-file /out/contract_state.bin
```

- Generate a circuit call intent
```shell
midnight-node-toolkit generate-intent circuit \
    -c toolkit-js/contract/contract.config.ts \
    --coin-public <coin-public-key for caller> \
    --input-onchain-state <contract-onchain-state-file> \
    --input-private-state <contract-private-state-json> \
    --contract-address <contract-address> \
    --output-intent "/out/call.bin" \
    --output-private-state "/out/new_state.json" \
    --output-zswap-state "/out/zswap_state.json" \
    <name-of-circuit-to-call>

# To send it, see "Generate and send a tx from an intent" above
```

#### Custom Contracts (Shielded Tokens)

- Invoking a contract that mints shielded tokens requires destinations to be passed when sending the intent
Example:
```bash
shielded_destination=$(
    midnight-node-toolkit \
    show-address \
    --network undeployed \
    --seed 0000000000000000000000000000000000000000000000000000000000000001 \
    --shielded
)

echo "Generate and send mint tx"
midnight-node-toolkit \
    send-intent \
    --intent-file "/out/mint.bin" \
    --zswap-state-file "/out/zswap.json" \
    --compiled-contract-dir /toolkit-js/contract/out \
    --shielded-destination "$shielded_destination"
```

If this isn't done, the transaction will succeed, but no coins will be visible in the destination wallet. This is because the encryption key is not visible to the contract execution layer.

### Register DUST Address

- Register a seed's DUST address to start generating DUST based on owned NIGHT. This also spends all NIGHT UTxOs owned by the wallet and recreates them, allowing them to start generating DUST.

```bash
midnight-node-toolkit \
    generate-txs \
    --src-files "res/genesis/genesis_block_undeployed.mn" \
    --dest-file "register.mn" \
    --to-bytes \
    register-dust-address \
    --wallet-seed "0000000000000000000000000000000000000000000000000000000000000000" \
    --funding-seed "0000000000000000000000000000000000000000000000000000000000000001"
```

---
### Send A Serialized Contract Intent (.mn) File:
```shell
midnight-node-toolkit send-intent --rng-seed '0000000000000000000000000000000000000000000000000000000000000037' --artifacts-dir "artifacts"
```
The intent file should be inside `intents` subdirectory of `--artifacts-dir`.
For contracts needing the resolver, its files should be subdirectories of `--artifacts-dir`:
```
artifacts
 |-- intents
 |    |-- 1_maintenance_intent.mn
 |-- keys
 |    |-- check.prover
 |    |-- ..
 |-- zkir
      |-- check.bzkir
      |-- ..
```

---
### Generate Contract Address
Shows and saves in a `--dest-file` the contract address found in a provided tx in `--src-file`
```shell
midnight-node-toolkit contract-address --network undeployed --src-file ./res/test-contract/contract_tx_1_deploy_undeployed.mn --dest-file /out/contract_adress_undeployed.mn
```

---

### Get a serialized `Transaction` form a serialized `TransactionWithContext`
Extracts a `Transaction` from a `--src-file` which containes a serialized `TransactionWithContext`, serializes it, saves it in `--dest-file`, and return its `BlocContext` tiemstamp in seconds as output.
```shell
midnight-node-toolkit get-tx-from-context --src-file deploy_undeployed.mn --dest-file deploy_no_context_undeployed.mn --network undeployed --from-bytes > timestamp.txt
```
---

### Generate Genesis
```shell
midnight-node-toolkit generate-genesis --network <network_name>
```

---

### Show Transaction
Show deserialized result of a single transaction. Two options:
- Tx saved as hex string
- Tx saved as bytes: use `--from-bytes` flag if the tx is saved in a file as bytes
```shell
midnight-node-toolkit show-transaction --network undeployed --src-file ./res/test-tx-deserialize/hex_serialized_tx_no_context.mn
```

---

### Show Transaction With Context
Show deserialized result of a single transaction with its context. Two options:
- Tx saved as hex string
- Tx saved as bytes: use `--from-bytes` flag if the tx is saved in a file as bytes
```shell
midnight-node-toolkit show-transaction --with-context --network undeployed --src-file ./res/test-tx-deserialize/hex_serialized_tx_with_context.mn
```

---

### Show Wallet
```shell
midnight-node-toolkit show-wallet --seed 0000000000000000000000000000000000000000000000000000000000000001
```

---

### Show Address
```shell
midnight-node-toolkit show-address --network undeployed --shielded --seed 0000000000000000000000000000000000000000000000000000000000000001
```

---

### Generate Random Address
Generate and print a random unshielded or shielded address. Parameters:
- `--shielded`: Generate a random shielded address when present, or a random unshielded address when not present.
- `--network`: Specify which network to generate the address for
- `--randomness-seed`: Specify a seed for the RNG (distinct from the wallet seed) for repeatable executions
```shell
midnight-node-toolkit random-address --network undeployed --shielded --randomness-seed 0000000000000000000000000000000000000000000000000000000000000001
```

---

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
