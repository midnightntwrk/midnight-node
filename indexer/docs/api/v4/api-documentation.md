# Midnight Indexer API Documentation v4

The Midnight Indexer API exposes a GraphQL API that enables clients to query and subscribe to blockchain data—blocks, transactions, contracts, DUST generation, shielded/unshielded transaction events, stake-pool-operator (SPO) data, and governance history—indexed from the Midnight blockchain. These capabilities facilitate both historical lookups and real-time monitoring.

**Version Information:**
- Current API version: v4

**Stability (`@beta`):**
Fields and types marked `@beta` in the schema are in-flight and may change without notice; stability is signalled by *removal* of the directive (a field losing `@beta` is a promise it has stabilised). Throughout this document, operations and fields that carry the directive are flagged with a *(@beta)* marker.

The `@beta` surface in this version (driven by the dust API mid-redesign—see tickets #1181 and #1173):
- **Queries:** `dustCommitmentMerkleTreeUpdate`, `dustGenerationMerkleTreeUpdate`.
- **Subscriptions:** `dustGenerations`, and its event types `DustGenerationsItem`, `DustGenerationsProgress`, `DustGenerationDtimeUpdateItem`.
- **Fields:** the dust end indices and Merkle roots on `Block` (`dustCommitmentEndIndex`, `dustGenerationEndIndex`, `dustCommitmentMerkleTreeRoot`, `dustGenerationMerkleTreeRoot`), the dust start/end indices on `RegularTransaction`, and the nullifier-transaction fields (`DustNullifierTransaction.nullifierLeBytes` / `.commitmentLeBytes` / `.transaction`, and `ShieldedNullifierTransaction.transaction`).

**Disclaimer:**
The examples provided here are illustrative and may need updating if the API changes. Always consider [`indexer-api/graphql/schema-v4.graphql`](../../../indexer-api/graphql/schema-v4.graphql) as the primary source of truth. Adjust queries as necessary to match the latest schema.

## GraphQL Schema

The GraphQL schema is defined in [`indexer-api/graphql/schema-v4.graphql`](../../../indexer-api/graphql/schema-v4.graphql). It specifies all queries, mutations, subscriptions, and their types, including arguments and return structures.

## Overview of Operations

- **Queries**:
    - *Blocks, transactions, contracts:* `block`, `transactions`, `contractAction`, `zswapMerkleTreeCollapsedUpdate`.
    - *DUST:* `dustGenerationStatus`, `dustGenerations`, `dustCommitmentMerkleTreeUpdate`, `dustGenerationMerkleTreeUpdate`.
    - *Governance history:* `dParameterHistory`, `termsAndConditionsHistory`.
    - *Stake Pool Operators (SPO):* identity and metadata (`spoIdentities`, `spoIdentityByPoolId`, `spoByPoolId`, `spoList`, `spoCompositeByPoolId`, `poolMetadata`, `poolMetadataList`, `spoCount`, `stakePoolOperators`), performance and epochs (`spoPerformanceLatest`, `spoPerformanceBySpoSk`, `epochPerformance`, `currentEpochInfo`, `epochUtilization`, `committee`), and registration series (`registeredTotalsSeries`, `registeredSpoSeries`, `registeredPresence`, `registeredFirstValidEpochs`, `stakeDistribution`).

- **Mutations**: Manage wallet sessions.
    - `connect(viewingKey: ViewingKey!, options: ConnectOptions)`: Creates a session associated with a viewing key.
    - `disconnect(sessionId: HexEncoded!)`: Ends a previously established session.

- **Subscriptions**: Receive real-time updates.
    - `blocks(offset)`: Stream newly indexed blocks.
    - `contractActions(address, offset)`: Stream contract actions.
    - `shieldedTransactions(sessionId, index)`: Stream shielded transaction updates, including relevant transactions and progress updates.
    - `unshieldedTransactions(address, transactionId)`: Stream unshielded transaction events for a specific address.
    - `dustGenerations(dustAddress, startIndex, endIndex)` *(@beta)*: Stream a dust address's generation entries interleaved with collapsed Merkle tree updates.
    - `dustLedgerEvents(id)`: Stream DUST ledger events.
    - `zswapLedgerEvents(id)`: Stream Zswap ledger events.
    - `dustNullifierTransactions(nullifierLeBytesPrefixes, fromBlock, toBlock)`: Stream transactions matching DUST nullifier prefixes.
    - `shieldedNullifierTransactions(nullifierPrefixes, fromBlock, toBlock)`: Stream transactions matching shielded nullifier prefixes.

## API Endpoints

**HTTP (Queries & Mutations):**
```
POST https://<host>:<port>/api/v4/graphql
Content-Type: application/json
```

**WebSocket (Subscriptions):**
```
wss://<host>:<port>/api/v4/graphql/ws
Sec-WebSocket-Protocol: graphql-transport-ws
```

## GraphQL Introspection

The API supports standard [GraphQL introspection](https://graphql.org/learn/introspection/), so clients and tooling (GraphiQL, code generators, schema-diff tools) can discover the schema at runtime rather than relying on this document. Send an introspection query to the HTTP endpoint using the `__schema` and `__type` meta-fields.

**Example** (list every subscription field and its arguments):

```graphql
query {
  __type(name: "Subscription") {
    fields {
      name
      args {
        name
        type { name kind ofType { name } }
      }
    }
  }
}
```

The committed [`indexer-api/graphql/schema-v4.graphql`](../../../indexer-api/graphql/schema-v4.graphql) is the canonical SDL and matches what introspection returns at runtime. (Some deployments may restrict introspection; if so, use the committed schema file instead.)

## Core Scalars

- `HexEncoded`: Hex-encoded bytes (for hashes, addresses, session IDs).
- `ViewingKey`: A viewing key in hex or Bech32 format for wallet sessions.
- `Unit`: An empty return type for mutations that do not return data.
- `UnshieldedAddress`: An unshielded address in Bech32m format (e.g., `mn_addr_test1...`). Used for unshielded token operations.
- `CardanoRewardAddress`: A Bech32-encoded Cardano reward (stake) address (e.g., `stake1...` or `stake_test1...`). Used for DUST generation queries.
- `DustAddress`: A Bech32m-encoded DUST address (`mn_dust...` on mainnet, `mn_dust_<network-id>...` elsewhere). Used for the DUST generations subscription.

## Input Types

### BlockOffset (oneOf)
Used to specify a block by either hash or height:
- `hash`: HexEncoded - The block hash
- `height`: Int - The block height

### TransactionOffset (oneOf)
Used to specify a transaction by either hash or identifier:
- `hash`: HexEncoded - The transaction hash
- `identifier`: HexEncoded - The transaction identifier

### ContractActionOffset (oneOf)
Used to specify a contract action location:
- `blockOffset`: BlockOffset - Query by block (hash or height)
- `transactionOffset`: TransactionOffset - Query by transaction (hash or identifier)

## Example Queries and Mutations

**Note:** These are examples only. Refer to the schema file to confirm exact field names and structures.

### block(offset: BlockOffset): Block

Query a block by offset. If no offset is provided, the latest block is returned.

**Example:**

Query by height:

```graphql
query {
  block(offset: { height: 3 }) {
    hash
    height
    protocolVersion
    timestamp
    author
    parent {
      hash
    }
    transactions {
      id
      hash
      transactionResult {
        status
        segments {
          id
          success
        }
      }
    }
  }
}
```

### transactions(offset: TransactionOffset!): [Transaction!]!

Fetch transactions by hash or by identifier. Returns an array of transactions matching the criteria.

**Note:** The `fees` field is now available on transactions, providing both `paidFees` and `estimatedFees` information.

**Example (by hash):**

```graphql
query {
  transactions(offset: { hash: "3031323..." }) {
    id
    hash
    protocolVersion
    merkleTreeRoot
    block {
      height
      hash
    }
    identifiers
    raw
    contractActions {
      __typename
      ... on ContractDeploy {
        address
        state
        zswapState
        unshieldedBalances {
          tokenType
          amount
        }
      }
      ... on ContractCall {
        address
        state
        entryPoint
        zswapState
        unshieldedBalances {
          tokenType
          amount
        }
      }
      ... on ContractUpdate {
        address
        state
        zswapState
        unshieldedBalances {
          tokenType
          amount
        }
      }
    }
    fees {
      paidFees
      estimatedFees
    }
    transactionResult {
      status
      segments {
        id
        success
      }
    }
    unshieldedCreatedOutputs {
      owner
      value
      tokenType
      intentHash
      outputIndex
    }
    unshieldedSpentOutputs {
      owner
      value
      tokenType
      intentHash
      outputIndex
    }
  }
}
```

**Example (by identifier):**
```graphql
query {
  transactions(offset: { identifier: "abc123..." }) {
    id
    hash
    unshieldedCreatedOutputs {
      owner
      value
      tokenType
    }
    unshieldedSpentOutputs {
      owner
      value
      tokenType
    }
  }
}
```


### contractAction(address: HexEncoded!, offset: ContractActionOffset): ContractAction

Retrieve the latest known contract action at a given offset (by block or transaction). If no offset is provided, returns the latest state.

**Example (latest):**

```graphql
query {
  contractAction(address: "3031323...") {
    __typename
    ... on ContractDeploy {
      address
      state
      zswapState
      unshieldedBalances {
        tokenType
        amount
      }
    }
    ... on ContractCall {
      address
      state
      zswapState
      entryPoint
      unshieldedBalances {
        tokenType
        amount
      }
    }
    ... on ContractUpdate {
      address
      state
      zswapState
      unshieldedBalances {
        tokenType
        amount
      }
    }
  }
}
```

**Example (by block height):**

```graphql
query {
  contractAction(
    address: "3031323...",
    offset: { blockOffset: { height: 10 } }
  ) {
    __typename
    ... on ContractDeploy {
      address
      state
      zswapState
      unshieldedBalances {
        tokenType
        amount
      }
    }
    ... on ContractCall {
      address
      state
      zswapState
      entryPoint
      unshieldedBalances {
        tokenType
        amount
      }
    }
    ... on ContractUpdate {
      address
      state
      zswapState
      unshieldedBalances {
        tokenType
        amount
      }
    }
  }
}
```

### dustGenerationStatus(cardanoRewardAddresses: [CardanoRewardAddress!]!): [DustGenerationStatus!]!

Query DUST generation status for one or more Cardano stake keys.

**Example:**

```graphql
query {
  dustGenerationStatus(
    cardanoRewardAddresses: [
      "stake_test1uqtgpdz0chm6jnxx7erfd7rhqfud7t4ajazx8es8xk8x3ts06psdv"
    ]
  ) {
    cardanoRewardAddress
    dustAddress
    registered
    nightBalance
    generationRate
    currentCapacity
  }
}
```

**DUST Generation Parameters:**
- Generation rate: 8,267 Specks per Star per second
- Maximum capacity: 5 DUST per NIGHT
- The `registered` field indicates if stake key is registered via NativeTokenObservation pallet
- Registration data comes from Cardano mainnet via bridge

**Important Note on `currentCapacity`:**
The `currentCapacity` field represents the maximum DUST generation capacity based on the Night UTXO balance and elapsed time. This value:
- Is accurate until the first DUST fee payment
- May be higher than actual balance after fee payments
- Cannot track spent DUST (fee payments are shielded transactions)

For accurate DUST balance after fee payments, query the connected wallet directly via wallet SDK or DApp Connector API. Use `currentCapacity` as an approximation when wallet connection is unavailable

### dustGenerations(cardanoRewardAddresses: [CardanoRewardAddress!]!): [DustGenerations!]!

Return all active DUST registrations with aggregated generation stats per Cardano reward address. Unlike `dustGenerationStatus`, this returns **every** active registration for each reward address (not capped at one), each as a `DustRegistration`.

**Example:**

```graphql
query {
  dustGenerations(cardanoRewardAddresses: ["stake_test1..."]) {
    cardanoRewardAddress
    registrations {
      dustAddress
      valid
      nightBalance
      generationRate
      maxCapacity
      currentCapacity
      utxoTxHash
      utxoOutputIndex
    }
  }
}
```

### Merkle Tree Collapsed Update Queries

Return a collapsed Merkle tree update for a `[startIndex, endIndex]` index range, so wallets can reconstruct tree state without downloading every leaf. Each returns a `MerkleTreeCollapsedUpdate` (`startIndex`, `endIndex`, `update: HexEncoded!`, `protocolVersion`).

- `zswapMerkleTreeCollapsedUpdate(startIndex: Int!, endIndex: Int!): MerkleTreeCollapsedUpdate!` — zswap (shielded) state tree.
- `dustCommitmentMerkleTreeUpdate(startIndex: Int!, endIndex: Int!): MerkleTreeCollapsedUpdate!` *(@beta)* — dust commitment tree.
- `dustGenerationMerkleTreeUpdate(startIndex: Int!, endIndex: Int!): MerkleTreeCollapsedUpdate!` *(@beta)* — dust generation tree.

**Example:**

```graphql
query {
  zswapMerkleTreeCollapsedUpdate(startIndex: 0, endIndex: 100) {
    startIndex
    endIndex
    update
    protocolVersion
  }
}
```

### Governance History Queries

Return the full history of on-chain governance parameter changes for auditability.

- `dParameterHistory: [DParameterChange!]!` — D-parameter changes. Each entry: `blockHeight`, `blockHash`, `timestamp`, `numPermissionedCandidates`, `numRegisteredCandidates`.
- `termsAndConditionsHistory: [TermsAndConditionsChange!]!` — Terms & Conditions changes. Each entry: `blockHeight`, `blockHash`, `timestamp`, `hash`, `url`.

**Example:**

```graphql
query {
  dParameterHistory {
    blockHeight
    timestamp
    numPermissionedCandidates
    numRegisteredCandidates
  }
}
```

### Stake Pool Operator (SPO) Queries

The indexer surfaces Cardano stake-pool-operator data: identities, metadata, per-epoch performance, and registration series. These are read-only queries; most take `limit`/`offset` pagination.

**Identity and metadata:**
- `spoIdentities(limit: Int, offset: Int): [SpoIdentity!]!` — SPO identities (`poolIdHex`, `mainchainPubkeyHex`, `sidechainPubkeyHex`, `auraPubkeyHex`, `validatorClass`).
- `spoIdentityByPoolId(poolIdHex: String!): SpoIdentity`
- `spoByPoolId(poolIdHex: String!): Spo` and `spoList(limit: Int, offset: Int, search: String): [Spo!]!` — SPO with metadata (`poolIdHex`, `validatorClass`, `name`, `ticker`, `homepageUrl`, `logoUrl`, ...).
- `spoCompositeByPoolId(poolIdHex: String!): SpoComposite` — combined identity, metadata and latest performance.
- `poolMetadata(poolIdHex: String!): PoolMetadata` and `poolMetadataList(limit: Int, offset: Int, withNameOnly: Boolean): [PoolMetadata!]!`
- `spoCount: Int`, `stakePoolOperators(limit: Int): [String!]!` — count and pool-id list.

**Performance and epochs:**
- `spoPerformanceLatest(limit, offset): [EpochPerf!]!`, `spoPerformanceBySpoSk(spoSkHex: String!, limit, offset): [EpochPerf!]!`, `epochPerformance(epoch: Int!, limit, offset): [EpochPerf!]!` — per-epoch produced/expected blocks (`epochNo`, `spoSkHex`, `produced`, `expected`, ...).
- `currentEpochInfo: EpochInfo`, `epochUtilization(epoch: Int!): Float`, `committee(epoch: Int!): [CommitteeMember!]!`.

**Registration series (analytics over an epoch range):**
- `registeredTotalsSeries(fromEpoch: Int!, toEpoch: Int!): [RegisteredTotals!]!`
- `registeredSpoSeries(fromEpoch: Int!, toEpoch: Int!): [RegisteredStat!]!`
- `registeredPresence(fromEpoch: Int!, toEpoch: Int!): [PresenceEvent!]!`
- `registeredFirstValidEpochs(uptoEpoch: Int): [FirstValidEpoch!]!`
- `stakeDistribution(limit, offset, search, orderByStakeDesc: Boolean): [StakeShare!]!`

**Example:**

```graphql
query {
  spoList(limit: 10, search: "pool") {
    poolIdHex
    name
    ticker
    validatorClass
  }
}
```

For the exact field set of each SPO type (`SpoIdentity`, `Spo`, `PoolMetadata`, `SpoComposite`, `EpochPerf`, `EpochInfo`, `CommitteeMember`, `RegisteredTotals`, `RegisteredStat`, `PresenceEvent`, `FirstValidEpoch`, `StakeShare`), consult the schema.

## Contract Action Types

All ContractAction types (ContractDeploy, ContractCall, ContractUpdate) implement the ContractAction interface with these common fields:
- `address`: The contract address (HexEncoded)
- `state`: The contract state (HexEncoded)
- `zswapState`: The contract-specific zswap state at this action (HexEncoded)
- `transaction`: The transaction that contains this action

Contract actions can be one of three types:
- **ContractDeploy**: Initial contract deployment
- **ContractCall**: Invocation of a contract's entry point
- **ContractUpdate**: State update to an existing contract

Each type implements the ContractAction interface but may have additional fields. For example, ContractCall includes an `entryPoint` field and a reference to its associated `deploy`.

All contract action types include an `unshieldedBalances` field that returns the token balances held by the contract:

- **ContractDeploy**: Always returns empty balances (contracts are deployed with zero balance).
- **ContractCall**: Returns balances after the call execution (may be modified by `unshielded_inputs`/`unshielded_outputs`).
- **ContractUpdate**: Returns balances after the maintenance update.

#### ContractBalance Type

```graphql
type ContractBalance {
  tokenType: HexEncoded!  # Token type identifier
  amount: String!         # Balance amount (supports u128 values)
}
```


## Block Type

The Block type represents a blockchain block:
- `hash`: The block hash (HexEncoded!)
- `height`: The block height (Int!)
- `protocolVersion`: The protocol version (Int!)
- `timestamp`: The UNIX timestamp (Int!)
- `author`: The block author (HexEncoded, optional)
- `zswapMerkleTreeRoot`: The hex-encoded serialized zswap state Merkle tree root (HexEncoded!)
- `ledgerParameters`: The hex-encoded ledger parameters for this block (HexEncoded!)
- `zswapEndIndex`: The zswap commitment tree end index at this block, exclusive/next-free (Int!)
- `dustCommitmentEndIndex`: The dust commitment tree end index at this block, exclusive/next-free (Int!, @beta)
- `dustGenerationEndIndex`: The dust generation tree end index at this block, exclusive/next-free (Int!, @beta)
- `dustCommitmentMerkleTreeRoot`: The hex-encoded dust commitment Merkle tree root at this block (HexEncoded, @beta)
- `dustGenerationMerkleTreeRoot`: The hex-encoded dust generation Merkle tree root at this block (HexEncoded, @beta)
- `parent`: Reference to the parent block (Block, optional)
- `transactions`: Array of transactions within this block ([Transaction!]!)
- `systemParameters`: The system (governance) parameters at this block height (SystemParameters!)

## Transaction Type

The Transaction type represents a blockchain transaction with its associated data:
- `id`: The transaction ID (Int!)
- `hash`: The transaction hash (HexEncoded)
- `protocolVersion`: The protocol version (Int!)
- `transactionResult`: The result of applying the transaction to the ledger state
- `fees`: Fee information including both paid and estimated fees
- `identifiers`: Transaction identifiers array ([HexEncoded!]!)
- `raw`: The raw transaction content (HexEncoded)
- `merkleTreeRoot`: The Merkle tree root (HexEncoded)
- `block`: Reference to the block containing this transaction
- `contractActions`: Array of contract actions within this transaction
- `unshieldedCreatedOutputs`: UTXOs created by this transaction
- `unshieldedSpentOutputs`: UTXOs spent by this transaction

### TransactionResult Type

The result of applying a transaction to the ledger state:
- `status`: TransactionResultStatus (SUCCESS, PARTIAL_SUCCESS, or FAILURE)
- `segments`: Optional array of segment results for partial success cases

### TransactionFees Type

Fee information for a transaction:
- `paidFees`: The actual fees paid for this transaction in DUST (String)
- `estimatedFees`: The estimated fees that was calculated for this transaction in DUST (String)

## Unshielded Token Types

### UnshieldedUtxo

Represents an unshielded UTXO (Unspent Transaction Output):
- `owner`: The owner's address in Bech32m format
- `intentHash`: The hash of the intent that created this output (HexEncoded)
- `value`: The UTXO value as a string (to support u128)
- `tokenType`: The token type identifier (HexEncoded)
- `outputIndex`: The index of this output within its creating transaction
- `createdAtTransaction`: Reference to the transaction that created this UTXO
- `spentAtTransaction`: Reference to the transaction that spent this UTXO (null if unspent)

## DUST Generation Types

### DustGenerationStatus

DUST generation status for a Cardano stake key:
- `cardanoRewardAddress`: The Bech32-encoded Cardano stake address (e.g., stake_test1... or stake1...)
- `dustAddress`: Associated DUST address if registered (HexEncoded, optional)
- `registered`: Whether this stake key is registered (Boolean!)
- `nightBalance`: NIGHT balance backing generation (String)
- `generationRate`: Generation rate in Specks per second (String)
- `currentCapacity`: Current DUST generation capacity in Specks - represents maximum possible balance, may be higher than actual balance after fee payments (String)

### DustLedgerEvent

DUST ledger events include:
- `DustInitialUtxo`: Initial DUST UTXO creation event
- `DustGenerationDtimeUpdate`: DUST generation decay time update
- `DustSpendProcessed`: DUST spend processing event
- `ParamChange`: DUST parameter change event

All DUST ledger event types share common fields:
- `id`: Event ID (Int!)
- `raw`: Raw event data (HexEncoded)
- `maxId`: Maximum ID of all DUST events (Int!)

## Mutations

Mutations allow the client to connect a wallet (establishing a session) and disconnect it.

### connect(viewingKey: ViewingKey!, options: ConnectOptions): HexEncoded!

Establishes a session for a given wallet viewing key in **either** bech32m or hex format. Returns the session ID. The optional `options` argument (`ConnectOptions`) accepts `startIndex: Int`, the transaction index (inclusive) from which to start searching for relevant transactions.

**Viewing Key Format Support**
- **Bech32m** (preferred): A base-32 encoded format with a human-readable prefix, e.g., `mn_shield-esk_dev1...`
- **Hex** (fallback): A hex-encoded string representing the key bytes.

**Example:**

```graphql
mutation {
  # Provide the bech32m format:
  connect(viewingKey: "mn_shield-esk1abcdef...")
}
```

**Response:**
```json
{
  "data": {
    "connect": "sessionIdHere"
  }
}
```

Use this `sessionId` for shielded transactions subscriptions.

### disconnect(sessionId: HexEncoded!): Unit!

Ends an existing session.

**Example:**

When done:
```graphql
mutation {
  disconnect(sessionId: "sessionIdHere")
}
```

## Subscriptions: Real-time Updates

Subscriptions use a WebSocket connection following the [GraphQL over WebSocket](https://github.com/enisdenjo/graphql-ws/blob/master/PROTOCOL.md) protocol. After connecting and sending a `connection_init` message, the client can start subscription operations.

### Blocks Subscription

`blocks(offset: BlockOffset): Block!`

Subscribe to new blocks. The `offset` parameter lets you start receiving from a given block (by height or hash). If omitted, starts from the latest block.

**Example:**

```json
{
  "id": "1",
  "type": "start",
  "payload": {
    "query": "subscription { blocks(offset: { height: 10 }) { hash height protocolVersion timestamp author parent { hash } transactions { id hash } } }"
  }
}
```

When a new block is indexed, the client receives a `next` message.

### Contract Actions Subscription

`contractActions(address: HexEncoded!, offset: BlockOffset): ContractAction!`

Subscribes to contract actions for a particular address. New contract actions (calls, updates) are pushed as they occur.

**Example:**

```json
{
  "id": "2",
  "type": "start",
  "payload": {
    "query": "subscription { contractActions(address:\"3031323...\", offset: { height: 1 }) { __typename ... on ContractDeploy { address state zswapState unshieldedBalances { tokenType amount } } ... on ContractCall { address state zswapState entryPoint unshieldedBalances { tokenType amount } } ... on ContractUpdate { address state zswapState unshieldedBalances { tokenType amount } } } }"
  }
}
```

### Shielded Transactions Subscription

`shieldedTransactions(sessionId: HexEncoded!, index: Int): ShieldedTransactionsEvent!`

Subscribes to shielded transaction updates. This includes relevant transactions and possibly Merkle tree updates, as well as `ShieldedTransactionsProgress` events. The `index` parameter can be used to resume from a certain point.

**Example:**

```json
{
  "id": "3",
  "type": "start",
  "payload": {
    "query": "subscription { shieldedTransactions(sessionId: \"1CYq6ZsLmn\", index: 100) { __typename ... on ViewingUpdate { index update { __typename ... on MerkleTreeCollapsedUpdate { start end update protocolVersion } ... on RelevantTransaction { start end transaction { id hash } } } } ... on ShieldedTransactionsProgress { highestIndex highestRelevantIndex highestRelevantWalletIndex } } }"
  }
}
```

**Event Types:**

**ShieldedTransactionsEvent** (union type):
- `ViewingUpdate`: Contains relevant transactions and/or zswap Merkle tree collapsed updates
  - `index`: Next start index into the zswap state (Int!)
  - `update`: Array of ZswapChainStateUpdate items ([ZswapChainStateUpdate!]!)
    - `MerkleTreeCollapsedUpdate`: Zswap Merkle tree collapsed update
      - `start`: Start index (Int!)
      - `end`: End index (Int!)
      - `update`: Hex-encoded Merkle tree collapsed update (HexEncoded)
      - `protocolVersion`: Protocol version (Int!)
    - `RelevantTransaction`: Transaction relevant to the wallet
      - `start`: Start index (Int!)
      - `end`: End index (Int!)
      - `transaction`: The relevant transaction (Transaction!)
- `ShieldedTransactionsProgress`: Synchronization progress information
  - `highestIndex`: The highest end index of all currently known transactions (Int!)
  - `highestRelevantIndex`: The highest end index of all currently known relevant transactions (Int!)
  - `highestRelevantWalletIndex`: The highest end index for this particular wallet (Int!)

### Unshielded Transactions Subscription

`unshieldedTransactions(address: UnshieldedAddress!, transactionId: Int): UnshieldedTransactionsEvent!`

Subscribes to unshielded transaction events for a specific address. Emits events whenever transactions involve unshielded UTXOs for the given address.

**Parameters:**
- `address`: The unshielded address to monitor (must be in Bech32m format).
- `transactionId`: Optional. The transaction ID to start from (defaults to 0).

**Example:**

```json
{
  "id": "4",
  "type": "start",
  "payload": {
    "query": "subscription { unshieldedTransactions(address: \"mn_addr_test1...\") { __typename ... on UnshieldedTransaction { transaction { hash block { height } } createdUtxos { owner value tokenType intentHash outputIndex } spentUtxos { owner value tokenType intentHash outputIndex } } ... on UnshieldedTransactionsProgress { highestTransactionId } } }"
  }
}
```

**Event Types:**

- **UnshieldedTransaction**: When UTXOs are created or spent, includes transaction details and affected UTXOs
- **UnshieldedTransactionsProgress**: Periodic synchronization progress updates

**UnshieldedTransactionsEvent**

Event payload for the unshielded transaction subscription:
- `UnshieldedTransaction`: Contains transaction details and UTXOs created/spent
  - `transaction`: The transaction that created and/or spent UTXOs
  - `createdUtxos`: UTXOs created in this transaction for the subscribed address
  - `spentUtxos`: UTXOs spent in this transaction for the subscribed address
- `UnshieldedTransactionsProgress`: Progress information
  - `highestTransactionId`: The highest transaction ID of all currently known transactions for the subscribed address

### DUST Ledger Events Subscription

`dustLedgerEvents(id: Int): DustLedgerEvent!`

Subscribe to DUST ledger events. The `id` parameter allows resuming from a specific event.

**Example:**

```json
{
  "id": "5",
  "type": "start",
  "payload": {
    "query": "subscription { dustLedgerEvents { id __typename ... on DustInitialUtxo { output { nonce } } raw maxId } }"
  }
}
```

### Zswap Ledger Events Subscription

`zswapLedgerEvents(id: Int): ZswapLedgerEvent!`

Subscribe to Zswap ledger events. The `id` parameter allows resuming from a specific event.

**Example:**

```json
{
  "id": "6",
  "type": "start",
  "payload": {
    "query": "subscription { zswapLedgerEvents { id raw maxId } }"
  }
}
```

### DUST Generations Subscription

`dustGenerations(dustAddress: DustAddress!, startIndex: Int!, endIndex: Int!): DustGenerationsEvent!` *(@beta)*

Subscribe to a dust address's generation entries in the generation-tree index range `[startIndex, endIndex]` (inclusive; `endIndex` maps to `dustGenerationEndIndex`, which is exclusive, so pass `dustGenerationEndIndex - 1`). Owned entries are interleaved with collapsed Merkle tree updates that fill the non-owned gaps, plus owned-entry decay-time (dtime) updates.

`DustGenerationsEvent` is a union of:
- `DustGenerationsItem`: an owned generation entry (`commitmentMtIndex`, `generationMtIndex`, `owner`, `value`, `initialValue`, `backingNight`, `ctime`, `transactionId`, `transactionHash`) with an optional `collapsedMerkleTree` filling the gap before it.
- `DustGenerationsProgress`: `highestIndex` and an optional final `collapsedMerkleTree` (the trailing gap).
- `DustGenerationDtimeUpdateItem`: a decay-time update for an owned generation (`generationMtIndex`, `owner`, `nightUtxoHash`, `newDtime`, `transactionId`, `transactionHash`, `treeInsertionPath`).

**Example:**

```json
{
  "id": "7",
  "type": "start",
  "payload": {
    "query": "subscription { dustGenerations(dustAddress: \"mn_dust...\", startIndex: 0, endIndex: 100) { __typename ... on DustGenerationsItem { generationMtIndex collapsedMerkleTree { startIndex endIndex update } } ... on DustGenerationsProgress { highestIndex collapsedMerkleTree { startIndex endIndex update } } ... on DustGenerationDtimeUpdateItem { generationMtIndex newDtime treeInsertionPath } } }"
  }
}
```

### DUST Nullifier Transactions Subscription

`dustNullifierTransactions(nullifierLeBytesPrefixes: [HexEncoded!]!, fromBlock: Int, toBlock: Int): DustNullifierTransaction!`

Subscribe to transactions containing DUST nullifiers whose 32-byte little-endian form starts with one of the provided prefixes, so wallets can discover their own DUST UTXOs. Each event carries `nullifierLeBytes`, `commitmentLeBytes`, `transactionId`, `transactionHash`, `blockHeight`, `blockHash`, and the full `transaction`. If `toBlock` is set, the subscription finishes after reaching that block; otherwise it continues live.

### Shielded Nullifier Transactions Subscription

`shieldedNullifierTransactions(nullifierPrefixes: [HexEncoded!]!, fromBlock: Int, toBlock: Int): ShieldedNullifierTransaction!`

Subscribe to transactions containing shielded nullifiers matching one of the provided prefixes. Each event carries `nullifier`, `transactionId`, `transactionHash`, `blockHeight`, `blockHash`, and the full `transaction`. If `toBlock` is set, the subscription finishes after reaching that block; otherwise it continues live.

## Query Limits Configuration

The server may apply limitations to queries (e.g. `max-depth`, `max-fields`, `timeout`, and complexity cost). Requests that violate these limits return errors indicating the reason (too many fields, too deep, too costly, or timed out).

**Example error:**

```json
{
  "data": null,
  "errors": [
    {
      "message": "Query has too many fields: 20. Max fields: 10."
    }
  ]
}
```

## Authentication

- Shielded transactions subscription requires a `sessionId` from the `connect` mutation.

## Regenerating the Schema

If you modify the code defining the GraphQL schema, regenerate it:
```bash
just generate-indexer-api-schema
```
This ensures the schema file stays aligned with code changes.

## Migration from v1

If migrating from API v1:
1. Update endpoint URLs from `/v1/graphql` to `/v4/graphql` (though v1 redirects automatically)
2. Review field name changes (e.g., `chainState` → `zswapState` in contract actions)
3. Test thoroughly as some response structures may have evolved

## Related Documentation

- [DUST Generation Status Details](../../interactions/dust-generation-status/dust-generation-status-api-documentation.md)
- [QA Testing Guide](../../interactions/dust-generation-status/dust-generation-status-qa-testing-guide.md)

## Conclusion

This document offers a few hand-picked examples and an overview of available operations. For the most accurate and comprehensive reference, consult the schema file. As the API evolves, remember to validate these examples against the schema and update them as needed.
