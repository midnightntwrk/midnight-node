# Toolkit

---

## 🚀 **IMPORTANT: See Usage Examples**

**The best way to understand how to use this CLI tool is by looking at the end-to-end test scripts.**

### 👉 Check out the `toolkit-*.sh` files here:
**https://github.com/midnightntwrk/midnight-node/tree/main/scripts/tests**

These scripts demonstrate real usage patterns and suggested best-practices for the toolkit.

---


## Implementation Status


| Feature                                                              | Progress |
|----------------------------------------------------------------------|----------|
| Send Shielded + Unshielded tokens                                    | ✅       |
| Sync with local and remote networks                                  | ✅       |
| DUST fee calculation                                                 | ✅       |
| Execute compiled contracts                                           | ✅       |
| Pre-generate and send 100s of transactions (performance testing)     | ✅       |
| Support for node runtime forks                                       | ✅       |
| Fetch and print wallet state and DUST balance                        | ✅       |
| Builds Node genesis                                                  | ✅       |
| Unit + integration tests                                             | ✅       |
| Shielded + Unshielded tokens sending between contract calls          | ✅       |
| DUST registration command                                            | 🚧       |
| Contract Maintenance - updating authority + verifier keys            | 🚧       |
| Contracts receiving Shielded + Unshielded tokens from user           | 🚧       |
| Support for Ledger forks                                             | ⏳       |
| Fallible Contracts                                                   | ⏳       |
| Composable Contracts                                                 | ⏳       |


## Usage

### Check Version information

To see compatibility with Node, Ledger, and Compactc versions, use the `version` command:

```console
$ midnight-node-toolkit version
Node: 0.18.0
Ledger: ledger-6.1.0-alpha.4
Compactc: 0.26.108-rc.0-UT-L6

```

### Generate Transactions

The `TxGenerator` is composed of four main components: `Source`, `Destination`, `Prover`, `Builder`.

The order the arguments are declared when building the command matters. `Builder`'s specific ones should go at the end, after its subcommand.

Example:
```shell
midnight-node-toolkit generate-txs <SRC_ARGS> <DEST_ARGS> <PROVER_ARG> batches <BUILDER_ARGS>
```

- **`Source`**: Determines where the `NetworkId` is selected and queries existing transactions to be applied to the local `LedgerState` before generating new transactions. Sources can be either a JSON file or a chain, selected via the following flags:
  - `--src-file <file_path>`
    - Supports multiple files:
      - `--src-file /res/genesis/genesis_block_undeployed.mn --src-file /res/test-data/contract/counter/deploy_tx.mn`
  - `--src-url <chain_url>` (defaults to `ws://127.0.0.1:9944`)

- **`Destination`**: Specifies where the generated transactions will be sent (either a file or a chain). Use:
  - `--dest-file <file_path>` (use `--to-bytes` to specify whether to save in JSON or bytes)
  - `--dest-url <chain_url>` (defaults to `ws://127.0.0.1:9944`)
    - Supports multiple urls:
      - `--dest-url="ws://127.0.0.1:9944" --dest-url="ws://127.0.0.1:9933" --dest-url="ws://127.0.0.1:9922"`
      - `--dest-url=ws://127.0.0.1:9944 --dest-url=ws://127.0.0.1:9933 --dest-url="ws://127.0.0.1:9922"`

- **`Prover`**: Chooses which proof server to use — either local (`LocalProofServer`) or remote (`RemoteProveServer`).

- **`Builder`**: Specifies how transactions are built. There are six builder subcommands:
  - `send`: Pass-through mode for sending transactions from a JSON file (`DoNothingBuilder`)
  - `single-tx`: Send a single transaction funded by a single wallet to N destination wallets (supports shielded and unshielded) (`SingleTxBuilder`)
  - `migrate`: Migrates transactions between chains (`ReplaceInitialTxBuilder`)
  - `batches`: Generates ZSwap & Unshielded Utxos transaction batches (`BatcherBuilder`)
  - `claim-mint`: Builds claim mint transactions (`ClaimMintBuilder`)
  - `contract-simple deploy`: Builds contract deployment transactions (`ContractDeployBuilder`)
  - `contract-simple maintenance`: Builds contract maintenance transactions (`ContractMaintenanceBuilder`)
  - `contract-simple call`: Builds general contract call transactions (`ContractCallBuilder`)

This enables four combinations of querying and sending transactions:

- **File to File**: Apply transformations and save back to a file.
- **File to Chain:** Read from a file, build new transactions, and send to a chain.
- **Chain to File:** Read from a chain, build new transactions, and save to a file.
- **Chain to Chain:** Read from a chain, build new transactions, and send to a chain.

Use the `-h` flag for full usage information.

**NOTE 1**
Since the introduction of the Ledger's `ReplayProtection` mechanism, the `TxGenerator` reads and send `TransactionWithContext` instead of `Transaction`. The reason is now it is necessary to know the `BlockContext` a transaction is valid.

If the user needs to know the `Transaction` value, it can make use of the command [`get-tx-from-context`](#) using as `--src-file` the previously generated `TransactionWithContext`.

#### Generate Zswap & Unshielded Utxos batches
- Query from chain, generate, and send to chain:
```console
$ midnight-node-toolkit generate-txs --dry-run batches -n 1 -b 2
Dry-run: Source transactions from url: "ws://127.0.0.1:9944"
Dry-run: Destination RPC: "ws://127.0.0.1:9944"
Dry-run: Destination rate: 1.0 TPS
Dry-run: Builder type: Batches(BatchesArgs { funding_seed: "0000000000000000000000000000000000000000000000000000000000000001", num_txs_per_batch: 1, num_batches: 2, concurrency: None, rng_seed: None, coin_amount: 100, shielded_token_type: ShieldedTokenType(0000000000000000000000000000000000000000000000000000000000000000), initial_unshielded_intent_value: 10000, unshielded_token_type: UnshieldedTokenType(0000000000000000000000000000000000000000000000000000000000000000), enable_shielded: false })
Dry-run: local prover (no proof server)

```
- Query from file, generate, and send to file:
```console
$ midnight-node-toolkit generate-txs --dry-run --dest-file txs.json batches -n 5 -b 1
...
```
- Query from file and send to chain with rate control:
```console
$ midnight-node-toolkit generate-txs --dry-run -r 2 --src-file txs.json --dest-url ws://127.0.0.1:9944 send
...
Dry-run: Destination rate: 2.0 TPS
Dry-run: Builder type: Send
...
```

#### Send a single transaction

- Query from local chain, generate with two unshielded outputs and one shielded output, send to local chain
```console
$ midnight-node-toolkit generate-txs --dry-run
>   single-tx
>   --shielded-amount 100
>   --unshielded-amount 5
>   --source-seed "0000000000000000000000000000000000000000000000000000000000000001"
>   --destination-address mn_shield-addr_undeployed12p0cn6f9dtlw74r44pg8mwwjwkr74nuekt4xx560764703qeeuvqxqqgft8uzya2rud445nach4lk74s7upjwydl8s0nejeg6hh5vck0vueqyws5
>   --destination-address mn_addr_undeployed13h0e3c2m7rcfem6wvjljnyjmxy5rkg9kkwcldzt73ya5pv7c4p8skzgqwj
>   --destination-address mn_addr_undeployed1h3ssm5ru2t6eqy4g3she78zlxn96e36ms6pq996aduvmateh9p9sk96u7s
...
```

#### Generate Deploy Contract (Built-in)

**Note:** These commands use a simple test contract built into the toolkit. For custom contracts, see the **Custom Contracts** section below

- Query from chain, generate, and send to chain:
```console
$ midnight-node-toolkit generate-txs --dry-run
>   contract-simple deploy
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
Dry-run: Source transactions from url: "ws://127.0.0.1:9944"
Dry-run: Destination RPC: "ws://127.0.0.1:9944"
Dry-run: Destination rate: 1.0 TPS
Dry-run: Builder type: ContractSimple(Deploy[..]
Dry-run: local prover (no proof server)

```
- Query from chain, generate, and send to bytes file:
```console
$ midnight-node-toolkit generate-txs --dry-run
>   --src-file res/genesis/genesis_tx_undeployed.mn
>   --dest-file deploy.mn
>   --to-bytes
>   contract-simple deploy
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
Dry-run: Source transactions from file(s): ["res/genesis/genesis_tx_undeployed.mn"]
Dry-run: Destination file: "deploy.mn"
Dry-run: Destination file-format: bytes
Dry-run: Builder type: ContractSimple(Deploy[..]
Dry-run: local prover (no proof server)

```
- Query from file, generate, and send to bytes file:
```console
$ midnight-node-toolkit generate-txs --dry-run
>   --dest-file deploy.mn
>   --to-bytes
>   contract-simple deploy
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
Dry-run: Source transactions from url: "ws://127.0.0.1:9944"
Dry-run: Destination file: "deploy.mn"
Dry-run: Destination file-format: bytes
Dry-run: Builder type: ContractSimple(Deploy[..]
Dry-run: local prover (no proof server)

```
- Query fom chain, generate, and save as a serialized intent file:
```console
$ midnight-node-toolkit generate-sample-intent --dry-run
>   --dest-dir "artifacts/intents"
>   deploy
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
...
```
- Using the [toolkit-js](../toolkit-js), generate the deploy intent file:
  * The contract must have been compiled using `compact`. For this example, the contract is found in `util/toolkit-js/test/contract/managed`
  * Also, `toolkit-js` should already be built, and be specified either via the `--toolkit_js_path` argument, or the `TOOLKIT_JS_PATH' environment
    * export TOOLKIT_JS_PATH="util/toolkit-js"
```console
$ midnight-node-toolkit generate-intent deploy
>   -c ../toolkit-js/test/contract/contract.config.ts \
>    --toolkit-js-path ../toolkit-js/
>    --output-intent out/intent.bin \
>    --output-private-state out/private_state.json \
>    --output-zswap-state out/zswap.json \
>    --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>    0
Executing generate-intent
Executing deploy command
Executing ../toolkit-js/dist/bin.js with arguments: ["deploy", "-c", "[CWD]/../toolkit-js/test/contract/contract.config.ts", "--network", "undeployed", "--coin-public", "aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98", "--output", "[CWD]/out/intent.bin", "--output-ps", "[CWD]/out/private_state.json", "--output-zswap", "[CWD]/out/zswap.json", "0"]...
written: out/intent.bin, out/private_state.json, out/zswap.json

```

#### Generate Maintenance Update (Built-in)

**Note:** These commands use a simple test contract built into the toolkit. For custom contracts, see the **Custom Contracts** section below

- Query from chain, generate, and send to chain:
```console
$ midnight-node-toolkit generate-txs --dry-run
>   contract-simple maintenance
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
...
```
- Query fom chain, generate, and save as a serialized intent file:
```console
$ midnight-node-toolkit generate-sample-intent --dry-run
>   --dest-dir "artifacts/intents"
>   maintenance
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
...
```
Rest of examples similar to Generate Deploy Contract

#### Generate Contract Call (Built-in)

**Note:** These commands use a simple test contract built into the toolkit. For custom contracts, see the **Custom Contracts** section below

- Query from chain, generate, and send to chain:
```console
$ midnight-node-toolkit generate-txs --dry-run
>   contract-simple call
>   --call-key store
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
...
```
- Query fom chain, generate, and save as a serialized intent file:
```console
$ midnight-node-toolkit generate-sample-intent --dry-run
>   --dest-dir "artifacts/intents"
>   call
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
...
```
Rest of examples similar to Generate Deploy Contract

#### Custom Contracts

The custom contract calls make use of **toolkit-js**. The nodejs `node` executable must be on the path, and a compiled version of toolkit js must be referenced by the `TOOLKIT_JS_PATH` environment variable for the following commands to work (if you're using the toolkit in a Docker container, this is done for you)

When compiling contracts, you **must** use the correct `compactc` version. To check compatibility, run `midnight-node-toolkit version`

- Get `coin-public-key` for a seed. In this context, the `coin-public` value is used to set the Shielded coin-public key for the contract caller
```console
$ midnight-node-toolkit show-address
>    --network undeployed
>    --seed 0000000000000000000000000000000000000000000000000000000000000001
>    --coin-public
aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98

```

- Generate a deploy intent
```shell
compactc counter.compact toolkit-js/contract/out # Compile your contract - compiled directory must be a child of $TOOLKIT_JS_PATH
```

```console
$ midnight-node-toolkit generate-intent deploy --dry-run
>    -c toolkit-js/contract/contract.config.ts
>    --toolkit-js-path ../toolkit-js/
>    --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>    --output-intent "/out/deploy.bin"
>    --output-private-state "/out/initial_private_state.json"
>    --output-zswap-state "/out/out.json"
Executing generate-intent
Dry-run: toolkit-js path: "../toolkit-js/"
Dry-run: generate deploy intent: DeployArgs[..]
...
```

- Generate a tx from an intent
```console
$ midnight-node-toolkit send-intent --dry-run
>   --intent-file "/out/deploy.bin"
>   --compiled-contract-dir contract/counter/out
>   --to-bytes
>   --dest-file "/out/deploy_tx.mn"
...
```

- Generate and send a tx from an intent
```shell
$ midnight-node-toolkit send-intent --dry-run
>   --intent-file "/out/deploy.bin"
>   --compiled-contract-dir contract/counter/out
```

- Generate and send a tx using multiple contract calls
```console
$ midnight-node-toolkit send-intent --dry-run
>   --intent-file "out/mint_intent.bin"
>   --intent-file "out/recieveAndSend_intent.bin"
>   --compiled-contract-dir ../toolkit-js/test/ut_contract/out
>   --to-bytes
>   --dest-file "/out/mint_tx.mn"
...
```

- Get the contract address
```console
$ midnight-node-toolkit contract-address
>   --src-file ./test-data/contract/counter/deploy_tx.mn
040dcc237a542543f1c0e0af4a8e937f74f357a238c9d2a9fcfcd644eb0f5c70

```

- Get the contract on-chain state
```console
$ midnight-node-toolkit contract-state
>   --src-file ../../res/genesis/genesis_block_undeployed.mn
>   --src-file ./test-data/contract/counter/deploy_tx.mn
>   --contract-address 040dcc237a542543f1c0e0af4a8e937f74f357a238c9d2a9fcfcd644eb0f5c70
>   --dest-file out/contract_state.bin
```

- Generate a circuit call intent
```console
$ midnight-node-toolkit generate-intent circuit
>   -c ../toolkit-js/test/contract/contract.config.ts
>   --toolkit-js-path ../toolkit-js/
>   --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>   --input-onchain-state ./test-data/contract/counter/contract_state.mn
>   --input-private-state ./test-data/contract/counter/initial_state.json
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
>   --output-intent out/intent.bin
>   --output-private-state out/ps_state.json
>   --output-zswap-state out/zswap_state.json
>   increment
Executing generate-intent
Executing circuit command
Executing ../toolkit-js/dist/bin.js with arguments: ["circuit", "-c", "[CWD]/../toolkit-js/test/contract/contract.config.ts", "--network", "undeployed", "--coin-public", "aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98", "--input", "[CWD]/test-data/contract/counter/contract_state.mn", "--input-ps", "[CWD]/test-data/contract/counter/initial_state.json", "--output", "[CWD]/out/intent.bin", "--output-ps", "[CWD]/out/ps_state.json", "--output-zswap", "[CWD]/out/zswap_state.json", "3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806", "increment"]...
toolkit-js> []
written: out/intent.bin, out/ps_state.json, out/zswap_state.json

```

To send it, see "Generate and send a tx from an intent" above

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
    --intent-file "out/mint.bin" \
    --zswap-state-file "out/zswap.json" \
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

### Get a serialized `Transaction` form a serialized `TransactionWithContext`
Extracts a `Transaction` from a `--src-file` which contains a serialized `TransactionWithContext`, serializes it, saves it in `--dest-file`, and return its `BlockContext` timestamp in seconds as output.
```ignore
$ midnight-node-toolkit get-tx-from-context
>   --src-file deploy_undeployed.mn
>   --dest-file deploy_no_context_undeployed.mn
>   --network undeployed --from-bytes > timestamp.txt
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
```console
$ midnight-node-toolkit show-transaction
>   --src-file ../../res/test-tx-deserialize/serialized_tx_no_context.mn

Tx StandardTransaction {
...
```

---

### Show Transaction With Context
Show deserialized result of a single transaction with its context. Two options:
- Tx saved as hex string
- Tx saved as bytes: use `--from-bytes` flag if the tx is saved in a file as bytes
```console
$ midnight-node-toolkit show-transaction --with-context
>   --src-file ../../res/test-tx-deserialize/serialized_tx_with_context.mn

Tx TransactionWithContext {
...
```

---

### Show Wallet (JSON output)
```console
$ midnight-node-toolkit show-wallet
>   --src-file ../../res/genesis/genesis_block_undeployed.mn
>   --seed 0000000000000000000000000000000000000000000000000000000000000001
{
  "coins": {
...
  },
  "utxos": [
    {
      "id": "c230c54a599a3d3472c5ee3f350c94745f1231412a4be729ea9f40db5e6776df#0",
      "initial_nonce": "ba356be982b6a8b77635cb5e8602d2889049c6f0080af6cd7403feab3b71eadf",
      "value": 500000000000000,
      "user_address": "bc610dd07c52f59012a88c2f9f1c5f34cbacc75b868202975d6f19beaf37284b",
      "token_type": "0000000000000000000000000000000000000000000000000000000000000000",
      "intent_hash": "c230c54a599a3d3472c5ee3f350c94745f1231412a4be729ea9f40db5e6776df",
      "output_number": 0
    },
...
  ],
  "dust_utxos": [
    {
      "initial_value": 0,
      "dust_public": "73ff4aaccbb878703e922c8ab5da32a349ca7b5a6e0a2b0950ac68c6a3e273471a",
      "nonce": "732ccb837ef1fa8cf30c5e4f1beafb9973c47ac6a67529a5541aff0f6625edf72e",
      "seq": 0,
      "ctime": 1754395200,
      "backing_night": "c7b64d5aa64262705b14735aa8eba798d072aa962ac1cb7f9da9693421410552",
      "mt_index": 0
    },
...
  ]
}

```

---

### Dust Balance

Prints the total Dust Balance, including a breakdown of the source of Dust per Dust-Output.

```console
$ midnight-node-toolkit dust-balance
>   --src-file ../../res/genesis/genesis_block_undeployed.mn
>   --seed 0000000000000000000000000000000000000000000000000000000000000001
{
  "generation_infos": [
...
  ],
  "source": {
...
  },
  "total": 12500000000000000000000000
}

```

---

### Show Address
```console
$ midnight-node-toolkit show-address
>   --network undeployed
>   --shielded
>   --seed 0000000000000000000000000000000000000000000000000000000000000001
mn_shield-addr_undeployed14gxh9wmhafr0np4gqrrx6awyus52jk7huyjy78kstym5ucnxawvqxq9k9e3s5qcpwx67zxhjfplszqlx2rx8q0egf59y0ze2827lju2mwqxr6r2x

```

---

### Generate Random Address
Generate and print a random unshielded or shielded address. Parameters:
- `--shielded`: Generate a random shielded address when present, or a random unshielded address when not present.
- `--network`: Specify which network to generate the address for
- `--randomness-seed`: Specify a seed for the RNG (distinct from the wallet seed) for repeatable executions
```console
$ midnight-node-toolkit random-address --network undeployed --shielded --randomness-seed 0000000000000000000000000000000000000000000000000000000000000001
mn_shield-addr_undeployed1[..]

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
docker run --network host -v $(pwd):/out midnight-node-toolkit:latest ... --dest-file /out/tx.json ...
```

**NOTE:** if you're running through Docker and want to access a node on localhost, use: `docker run --network host ...`
