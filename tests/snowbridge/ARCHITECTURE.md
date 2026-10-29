# Bridge components and flows

Midnight is the whole chain; the runtime coordinates bridge operations, the ledger
validates asset and contract state, and Compact contracts operate on those ledger
assets. The SP1 prover runs outside the chain and proves Midnight finality to
Ethereum.

These diagrams show the implemented flow, including the committee checks. View
this file in a Markdown viewer with Mermaid support to render the diagrams.

## 1. Where the components sit

Arrows here represent calls or data dependencies, rather than token transfers.

```mermaid
flowchart TB
    User["Wallet / application<br/>Builds and proves Compact transactions"]

    subgraph Midnight["Midnight node"]
        subgraph Runtime["Runtime — chain rules and coordination"]
            MP["Midnight pallet<br/>Ordinary ledger transactions"]
            BP["EthereumBridge pallet<br/>Deposits, exports, retry/completion"]
            BC["EthereumBeaconClient<br/>Verifies Ethereum finality and receipts"]
            MS["MidnightSystem<br/>Restricted bridge issuance"]
            Q["Outbound queue + MMR<br/>Commits withdrawal messages"]
        end

        subgraph Ledger["Ledger — validated assets and contract state"]
            Engine["Ledger engine / host interface<br/>Proofs, signatures, balances, replay checks"]
            Wallet["Native unshielded wallet UTXOs"]
            Consumer["Compact consumer<br/>Receive / balance / withdraw"]
            Escrow["Sealed Compact escrow<br/>Retire tokens + record recipient"]
        end

        Consensus["Node consensus / BEEFY<br/>Signs commitments to the MMR root"]
    end

    User --> MP
    MP --> Engine
    BP -->|"Verify event proof"| BC
    BP -->|"Issue verified deposit"| MS
    MS --> Engine
    BP -->|"Read committed retirement"| Engine
    Engine --- Wallet
    Engine --- Consumer
    Engine --- Escrow
    BP -->|"Export withdrawal"| Q
    Q --> Consensus
```

Compact contracts do not independently mint the bridged asset. Issuance passes
through the runtime’s restricted system transaction path. Their balances and
retirement records are validated and stored by the ledger.

## 2. How tokens travel in both directions

Solid arrows show asset movement or issuance. Dashed arrows show proofs and
messages.

```mermaid
flowchart TB
    EU["Ethereum user"]
    Custody["Ethereum Gateway / custody<br/>Locks original ERC-20"]

    EU -->|"Deposit 100 ERC-20"| Custody
    Custody -.->|"Finalized deposit event + receipt proof<br/>carried by relayer"| Ingress

    subgraph Midnight["Midnight"]
        Ingress["Runtime beacon client + bridge pallet<br/>Verify deposit; prevent duplicate issuance"]
        Issue["MidnightSystem → ledger<br/>Issue 100 native unshielded tokens"]
        W1["User wallet UTXO<br/>100"]
        C["Compact consumer"]
        W2["User wallet UTXO<br/>40"]
        E["Sealed Compact escrow<br/>40 permanently retired"]
        Export["Runtime bridge + outbound queue<br/>Read retirement and commit withdrawal"]

        Ingress --> Issue
        Issue --> W1
        W1 -->|"deposit: 100"| C
        C -->|"withdraw: 40<br/>60 remain in consumer"| W2
        W2 -->|"retire: 40<br/>Record Ethereum recipient"| E
        E -.->|"Committed retirement record"| Export
    end

    Export -.->|"SP1 finality proof + message/MMR inclusion<br/>carried by relayer"| Release
    Release["Ethereum Gateway<br/>Verify withdrawal and release custody"]
    Custody -->|"Unlock 40 after verification"| Release
    Release -->|"40 ERC-20"| Recipient["Ethereum recipient"]
    Release -.->|"Finalized success/failure receipt<br/>verified on Midnight"| Result

    Result["Runtime redemption status<br/>Success → Completed<br/>Failure → retry allowed with new nonce"]
    Result -.->|"Authorized retry"| Export
```

The escrow’s 40 tokens remain in an unspendable contract balance: retirement here
means permanent locking. The tested path from the consumer to escrow goes
**through a wallet UTXO**.

Other Compact contracts can use the same native token color through ordinary
unshielded operations; the consumer demonstrates this.

## 3. What SP1 and the committee protection verify

This is the proof path behind the withdrawal arrow above. MMR means *Merkle
Mountain Range*, the authenticated structure containing the message and committee
commitments.

```mermaid
flowchart TB
    Anchor["Previously accepted checkpoint<br/>Genesis, height, current committee, next committee"]
    Signed["Midnight BEEFY commitments<br/>Quorum signatures + committee payloads"]
    Witness["Runtime MMR witnesses<br/>Prove next-committee snapshot<br/>at each selected signed block"]

    subgraph Offchain["Off-chain relayer / SP1 prover"]
        Select["Collect commitments and witnesses<br/>Defer rotation boundary until snapshot matches"]
        Guest["SP1 guest verifies<br/>Signatures and quorum<br/>Pinned committee succession<br/>Runtime committee witness agreement"]
        Proof["Generate Groth16 proof<br/>New checkpoint + authenticated MMR root"]
    end

    Anchor --> Guest
    Signed --> Select
    Witness --> Select
    Select --> Guest
    Guest --> Proof

    subgraph Ethereum["Ethereum"]
        Client["MidnightSP1Client<br/>Check prior checkpoint, genesis and height<br/>Verify proof via Succinct verifier"]
        State["Accepted checkpoint + MMR root<br/>Current and next committee views"]
        Gateway["MidnightGateway<br/>Check message/MMR inclusion<br/>Validate route and prevent replay"]
        Pay["Release ERC-20 from custody"]
    end

    Proof --> Client
    Client --> State
    State --> Gateway
    Inclusion["Relayer supplies withdrawal message<br/>and inclusion proof"] --> Gateway
    Gateway --> Pay
```

SP1 authenticates **Midnight finality and committee transitions**. The Gateway
separately checks that the withdrawal message belongs to the authenticated
commitment. Ethereum-to-Midnight deposits and result receipts use the runtime’s
Ethereum beacon client.

“Avail-style” describes the prover/client architecture; this implementation does
**not** send tokens or data through Avail.

The complete tested round trip ended with **60 ERC-20 in Ethereum custody, 40 paid
to the Ethereum recipient, 60 native tokens in the Compact consumer, and 40
permanently retired in escrow**. Committee checks prevent unauthorized
substitutions and rollback, but cannot detect an isolated relayer being denied
newer blocks.

## Implementation references

- [Runtime wiring](../../runtime/src/ethereum_bridge.rs)
- [Bridge state machine](../../pallets/ethereum-bridge/src/lib.rs)
- [Compact consumer](compact/consumer.compact)
- [Compact escrow](compact/escrow.compact)
- [Ethereum SP1 client](contracts/src/MidnightSP1Client.sol)
- [Ethereum Gateway](contracts/src/MidnightGateway.sol)
- [SP1 design and validation](sp1/README.md)
