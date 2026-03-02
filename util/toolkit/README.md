# Midnight Toolkit

CLI tool for interacting with the Midnight blockchain. Supports transaction generation, wallet management, contract deployment, and testing.

---

## 🚀 **Quick Start: See Usage Examples**

**👉 Check out the `toolkit-*.sh` test scripts for real usage patterns:**  
**https://github.com/midnightntwrk/midnight-node/tree/main/scripts/tests**

### 📦 Version Selection

**Recommended:** Use `latest-main` for backwards compatibility and latest bugfixes:
```bash
docker pull midnightntwrk/midnight-node-toolkit:latest-main
```

For guaranteed compatibility with a specific node version, use matching tags:
```bash
# Example: both toolkit and node at version 0.18.0-rc.7
docker pull midnightntwrk/midnight-node-toolkit:0.18.0-rc.7
docker pull midnightntwrk/midnight-node:0.18.0-rc.7
```

---

## Implementation Status

| Feature | Progress |
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
| Contract Maintenance - updating authority + verifier keys            | ✅       |
| Execute calls via governance (root-call)                             | ✅       |
| Support for Ledger forks                                             | ✅       |
| DUST registration command                                            | ✅       |
| Contracts receiving Shielded + Unshielded tokens from user           | 🚧       |
| Fallible Contracts                                                   | ⏳       |
| Composable Contracts                                                 | ⏳       |

---

## Usage

### Check Version information

To see compatibility with Node, Ledger, and Compactc versions, use the `version` command:

```console
$ midnight-node-toolkit version
Node: [..]
Ledger: [..]
Compactc: [..]

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
  - `--dest-file <file_path>`
  - `--dest-url <chain_url>` (defaults to `ws://127.0.0.1:9944`)
    - Supports multiple urls:
      - `--dest-url="ws://127.0.0.1:9944" --dest-url="ws://127.0.0.1:9933" --dest-url="ws://127.0.0.1:9922"`
      - `--dest-url=ws://127.0.0.1:9944 --dest-url=ws://127.0.0.1:9933 --dest-url="ws://127.0.0.1:9922"`

- **`Prover`**: Chooses which proof server to use — either local (`LocalProofServer`) or remote (`RemoteProveServer`).

- **`Builder`**: Specifies how transactions are built. There are six builder subcommands:
  - `send`: Pass-through mode for sending transactions from a JSON file (`DoNothingBuilder`)
  - `single-tx`: Send a single transaction funded by a single wallet to N destination wallets (supports shielded and unshielded) (`SingleTxBuilder`)
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

If the user needs to know the `Transaction` value, it can make use of the command [`get-tx-from-context`](#get-a-serialized-transaction-from-a-serialized-transactionwithcontext) using as `--src-file` the previously generated `TransactionWithContext`.

### Caching fetched transactions

The toolkit implements a caching mechanism to avoid fetching the entire chain each time you generate a new transaction. The caching mechanism implements three backends, which can be set using the `MN_FETCH_CACHE` environment variable:

- `inmemory` - no persistence, fetched transactions are not stored to disk
- `redb:<filename>` - persists fetched transactions to disk. Toolkit process must have exclusive access to this file
- `postgres://[user[:password]@][netloc][:port][/dbname][?param1=value1&...]` - persists fetched transactions to a postgres database. Supports concurrent readers/writers.

#### Generate Zswap & Unshielded Utxos batches
- Query from chain, generate, and send to chain:
```console
$ midnight-node-toolkit generate-txs --dry-run batches -n 1 -b 2
Dry-run: Source transactions from url: "ws://127.0.0.1:9944"
Dry-run: Destination RPC(s): ["ws://127.0.0.1:9944"]
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
Dry-run: Destination RPC(s): ["ws://127.0.0.1:9944"]
Dry-run: Destination rate: 1.0 TPS
Dry-run: Builder type: ContractSimple(Deploy[..]
Dry-run: local prover (no proof server)

```
- Query from chain, generate, and send to bytes file:
```console
$ midnight-node-toolkit generate-txs --dry-run
>   --src-file res/genesis/genesis_tx_undeployed.mn
>   --dest-file deploy.mn
>   contract-simple deploy
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
Dry-run: Source transactions from file(s): ["res/genesis/genesis_tx_undeployed.mn"]
Dry-run: Destination file: "deploy.mn"
Dry-run: Builder type: ContractSimple(Deploy[..]
Dry-run: local prover (no proof server)

```
- Query from file, generate, and send to bytes file:
```console
$ midnight-node-toolkit generate-txs --dry-run
>   --dest-file deploy.mn
>   contract-simple deploy
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
Dry-run: Source transactions from url: "ws://127.0.0.1:9944"
Dry-run: Destination file: "deploy.mn"
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
```ignore-compact-0.27
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

#### Generate Maintenance Update

Works with either the built-in contract or custom contracts.

- Add a new `increment2` endpoint, update `increment` entypoint, remove the `decrement` entrypoint, and switch to a new authority.
```console
$ midnight-node-toolkit generate-txs --dry-run
>   contract-simple maintenance
>   --rng-seed '0000000000000000000000000000000000000000000000000000000000000037'
>   --remove-entrypoint decrement \
>   --upsert-entrypoint ../toolkit-js/contract/managed/counter/keys/increment.verifier \
>   --upsert-entrypoint ../toolkit-js/contract/managed/counter/keys/increment2.verifier \
>   --authority-seed 1000000000000000000000000000000000000000000000000000000000000001 \
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
1bd4f827be97ff013c4a702e4b08f30ec378728a54670cf7cc92cb9b1a14eff6

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
>   --compiled-contract-dir ../toolkit-js/test/minter_contract/out
>   --dest-file "/out/mint_tx.mn"
...
```

- Get the contract address
```ignore-compact-0.27
$ midnight-node-toolkit contract-address
>   --src-file ./test-data/contract/counter/deploy_tx.mn
3f418f852023931a1f2f507500a3879cdeb357415418cce083946fedb6afe299

```

- Get the contract on-chain state
```ignore-compact-0.27
$ midnight-node-toolkit contract-state
>   --src-file ../../res/genesis/genesis_block_undeployed.mn
>   --src-file ./test-data/contract/counter/deploy_tx.mn
>   --contract-address 3f418f852023931a1f2f507500a3879cdeb357415418cce083946fedb6afe299
>   --dest-file out/contract_state.bin
```

- Generate a circuit call intent
```ignore-compact-0.27
$ midnight-node-toolkit generate-intent circuit
>   -c ../toolkit-js/test/contract/contract.config.ts
>   --toolkit-js-path ../toolkit-js/
>   --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>   --input-onchain-state ./test-data/contract/counter/contract_state.mn
>   --input-private-state ./test-data/contract/counter/initial_state.json
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
>   --output-intent out/intent.bin
>   --output-onchain-state out/onchain_state.mn
>   --output-private-state out/ps_state.json
>   --output-zswap-state out/zswap_state.json
>   --output-result out/result.json
>   increment
Executing generate-intent
Executing circuit command
Executing ../toolkit-js/dist/bin.js with arguments: ["circuit", "-c", "[CWD]/../toolkit-js/test/contract/contract.config.ts", "--network", "undeployed", "--coin-public", "aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98", "--input", "[CWD]/test-data/contract/counter/contract_state.mn", "--input-ps", "[CWD]/test-data/contract/counter/initial_state.json", "--output", "[CWD]/out/intent.bin", "--output-ps", "[CWD]/out/ps_state.json", "--output-zswap", "[CWD]/out/zswap_state.json", "--output-oc", "[CWD]/out/onchain_state.mn", "--output-result", "[CWD]/out/result.json", "3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806", "increment"]...
toolkit-js> []
written: out/intent.bin, out/ps_state.json, out/zswap_state.json

```

To send it, see "Generate and send a tx from an intent" above

- Generate batched circuit call intents (multiple calls in one transaction)

When batching multiple circuit calls into a single transaction, each call's output state must be chained as the next call's input. Without chaining, subsequent calls would operate on stale state and the transaction would fail.

```ignore-compact-0.27
# Call 1: first circuit call — outputs on-chain and private state for chaining
$ midnight-node-toolkit generate-intent circuit
>   -c ../toolkit-js/test/contract/contract.config.ts
>   --toolkit-js-path ../toolkit-js/
>   --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>   --input-onchain-state ./contract_state.mn
>   --input-private-state ./initial_state.json
>   --contract-address <CONTRACT_ADDRESS>
>   --output-intent out/intent_1.bin
>   --output-onchain-state out/onchain_state_1.mn
>   --output-private-state out/private_state_1.json
>   --output-zswap-state out/zswap_1.json
>   increment
```
```ignore-compact-0.27
# Call 2: uses call 1's outputs as inputs
$ midnight-node-toolkit generate-intent circuit
>   -c ../toolkit-js/test/contract/contract.config.ts
>   --toolkit-js-path ../toolkit-js/
>   --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>   --input-onchain-state out/onchain_state_1.mn
>   --input-private-state out/private_state_1.json
>   --contract-address <CONTRACT_ADDRESS>
>   --output-intent out/intent_2.bin
>   --output-onchain-state out/onchain_state_2.mn
>   --output-private-state out/private_state_2.json
>   --output-zswap-state out/zswap_2.json
>   increment
```
```ignore-compact-0.27
# Combine both intents into a single transaction
$ midnight-node-toolkit send-intent
>   --intent-file out/intent_1.bin
>   --intent-file out/intent_2.bin
>   --compiled-contract-dir contract/counter/out
```

The key state files to chain between calls:
- `--output-onchain-state` from call N becomes `--input-onchain-state` for call N+1
- `--output-private-state` from call N becomes `--input-private-state` for call N+1

- Generate a contract maintenance intent
```ignore-compact-0.27
$ midnight-node-toolkit generate-intent maintain-contract
>   -c ../toolkit-js/test/contract/contract.config.ts
>   --toolkit-js-path ../toolkit-js/
>   --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>   --input-onchain-state ./test-data/contract/counter/contract_state.mn
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
>   --output-intent out/intent.bin
>   --signing 0000000000000000000000000000000000000000000000000000000000000001
>   0000000000000000000000000000000000000000000000000000000000000002
Executing generate-intent
Executing maintain command
Executing ../toolkit-js/dist/bin.js with arguments: ["maintain", "contract", "-c", "[CWD]/../toolkit-js/test/contract/contract.config.ts", "--network", "undeployed", "--coin-public", "aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98", "--input", "[CWD]/test-data/contract/counter/contract_state.mn", "--output", "[CWD]/out/intent.bin", "--signing", "0000000000000000000000000000000000000000000000000000000000000001", "3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806", "0000000000000000000000000000000000000000000000000000000000000002"]...
written: out/intent.bin

```

To send it, see "Generate and send a tx from an intent" above

- Generate a circuit maintenance intent
```ignore-compact-0.27
$ midnight-node-toolkit generate-intent maintain-circuit
>   -c ../toolkit-js/test/contract/contract.config.ts
>   --toolkit-js-path ../toolkit-js/
>   --coin-public aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98
>   --input-onchain-state ./test-data/contract/counter/contract_state.mn
>   --contract-address 3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806
>   --output-intent out/intent.bin
>   --signing 0000000000000000000000000000000000000000000000000000000000000001
>   increment
>   ./test-data/contract/counter/keys/increment.verifier
Executing generate-intent
Executing maintain command
Executing ../toolkit-js/dist/bin.js with arguments: ["maintain", "circuit", "-c", "[CWD]/../toolkit-js/test/contract/contract.config.ts", "--network", "undeployed", "--coin-public", "aa0d72bb77ea46f986a800c66d75c4e428a95bd7e1244f1ed059374e6266eb98", "--input", "[CWD]/test-data/contract/counter/contract_state.mn", "--output", "[CWD]/out/intent.bin", "--signing", "0000000000000000000000000000000000000000000000000000000000000001", "3102ba67572345ef8bc5cd238bff10427b4533e376b4aaed524c2f1ef5eca806", "increment", "[CWD]/test-data/contract/counter/keys/increment.verifier"]...
written: out/intent.bin

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
    register-dust-address \
    --wallet-seed "0000000000000000000000000000000000000000000000000000000000000000" \
    --funding-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --destination-dust "mn_dust-addr_undeployed1v36hxapdv9jxgun9wde4ka33t5a88l624n9ms7rs86fzez44mge2xjw20ddxuz3tp9g2c6xx5038x3c6nnqc6y"
```

### Deregister DUST Address

- Deregister (unlink) a wallet's DUST address mapping. This is useful when migrating to a new DUST address, cleaning up test registrations, or revoking access before rotating wallet keys.

```bash
midnight-node-toolkit \
    generate-txs \
    --src-url "wss://rpc.qanet.dev.midnight.network" \
    --dest-url "wss://rpc.qanet.dev.midnight.network" \
    deregister-dust-address \
    --wallet-seed "0000000000000000000000000000000000000000000000000000000000000000" \
    --funding-seed "0000000000000000000000000000000000000000000000000000000000000001"
```

---

### Get a serialized `Transaction` from a serialized `TransactionWithContext`
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

#### Custom Ledger Parameters
You can optionally provide a JSON file with custom ledger parameters to use instead of the default `INITIAL_PARAMETERS`:

```shell
midnight-node-toolkit generate-genesis \
    --network <network_name> \
    --seeds-file /path/to/seeds.json \
    --ledger-parameters-config /path/to/ledger-parameters-config.json
```

The `ledger-parameters-config.json` file should contain a JSON representation of the `LedgerParameters` struct. Default config files with the initial parameters are available in `res/<network>/ledger-parameters-config.json`.

---

### Show Transaction
Show the structure of a saved transaction. Works with files containing multiple txs
```console
$ midnight-node-toolkit show-transaction
>   --src-file ../../res/test-tx-deserialize/serialized_tx.mn

...
```

### Show Ledger Parameters
Show parsed and serialized ledger parameters. \
It allows overriding the base parameters by passing the new values:
```ignore
$ midnight-node-toolkit show-ledger-parameters -r ws://localhost:9944 --c-to-m-bridge-min-amount 2000
```
Base parameters can be loaded in these ways:
 - From the remote server: `-r ws://localhost:9944`
 - By providing the serialized parameters: `--base-parameters 0x...`
 - Otherwise, the initial ledger parameters are used.

Return types:
 - With the `--serialize` option, only the serialized parameters are returned.
 - Otherwise, the parsed parameters and the serialized are returned.

### Update Ledger Parameters
Update the ledger parameters on the remote server via federated authority.

Update parameters based on the existing ones:
```ignore
$ midnight-node-toolkit update-ledger-parameters -t //Alice -t //Bob -c //Dave -c //Eve --c-to-m-bridge-min-amount 2000
```
Update parameters based on a serialized value:
```ignore
$ midnight-node-toolkit update-ledger-parameters --parameters=0x... -t //Alice -t //Bob -c //Dave -c //Eve --c-to-m-bridge-min-amount 2000
```

### Root Call (Execute Call via Governance)
Execute an arbitrary runtime call with Root origin through the federated authority governance mechanism using proper governance (Council + Technical Committee approval).

The command requires private keys from both Council and Technical Committee members to vote and approve the motion.

```bash
midnight-node-toolkit root-call \
    --council-keys <HEX_PRIVATE_KEY_1> <HEX_PRIVATE_KEY_2> [...] \
    --tc-keys <HEX_PRIVATE_KEY_1> <HEX_PRIVATE_KEY_2> [...] \
    --encoded-call <HEX_ENCODED_CALL>
```

Parameters:
- `--council-keys`: Council member private keys as hex strings (32-byte sr25519 seeds). At least 2 required for 2/3 threshold voting.
- `--tc-keys`: Technical Committee member private keys as hex strings (32-byte sr25519 seeds). At least 2 required for 2/3 threshold voting.
- `--encoded-call`: The SCALE-encoded runtime call as a hex string (e.g., `0x00000400`)
- `--encoded-call-file`: Alternative to `--encoded-call`, path to a file containing the encoded call hex string
- `--rpc-url`: RPC URL of the node (defaults to `ws://127.0.0.1:9944`, can also be set via `RPC_URL` env var)

Example:
```bash
midnight-node-toolkit root-call \
    --council-keys 0x42438b7883391c05512a938e36c2df0131e088b3756d6aa7a755fbff19d2f842 \
                   0x868020ae0687dda7d57565093a69090211449845a7e11453612800b663307246 \
    --tc-keys 0x398f0c28f98885e046333d4a41c19cee4c37368a9832c6502f6cfd182e2aef89 \
              0xbc1ede780f784bb6991a585e4f6e61522c14e1cae6ad0895fb57b9a205a8f938 \
    --encoded-call 0x00000400
```

The command will:
1. Decode and validate the encoded call
2. Create a Council proposal for `FederatedAuthority::motion_approve`
3. Have Council members vote on the proposal
4. Close the Council proposal
5. Create a Technical Committee proposal for the same motion
6. Have TC members vote on the proposal
7. Close the TC proposal
8. Close the federated motion to execute the call with Root origin

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
      "id": "01c5ad3ff58d687dfe27fc779726188adfe777de5efa8f938a014d7fd7045c59#0",
      "initial_nonce": "f5e761a22c22f362f1e62435c303c3f6210d93cde80f4ada80465002a172ecc9",
      "value": 50000000000000,
      "user_address": "bc610dd07c52f59012a88c2f9f1c5f34cbacc75b868202975d6f19beaf37284b",
      "token_type": "0000000000000000000000000000000000000000000000000000000000000000",
      "intent_hash": "01c5ad3ff58d687dfe27fc779726188adfe777de5efa8f938a014d7fd7045c59",
      "output_number": 0
    },
...
  ],
  "dust_utxos": [
    {
      "initial_value": 0,
      "dust_public": "73ff4aaccbb878703e922c8ab5da32a349ca7b5a6e0a2b0950ac68c6a3e273471a",
      "nonce": "73171d7cd802d682ff676a912f381f40169764f3c1c9c5f08714ae47539ed6ef1c",
      "seq": 0,
      "ctime": 1754395200,
      "backing_night": "47efc37cb1f6e9840820529e664a26ef73faae932466aaf94cb523c2df577051",
      "mt_index": 3
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
  "total": 1250000000000000000000000,
  "capacity": 1250000000000000000000000
}

```

---

### Show Address
```console
$ midnight-node-toolkit show-address
>   --network undeployed
>   --shielded
>   --seed 0000000000000000000000000000000000000000000000000000000000000001
mn_shield-addr_undeployed1r020sfa7jllsz0z2wqhykz8npmphsu5223nsea7vjt9ekxs5almtvtnrpgpszud4uyd0yjrlqyp7v5xvwqljsng2g79j5w4al9c4kuqy0xtw4

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
