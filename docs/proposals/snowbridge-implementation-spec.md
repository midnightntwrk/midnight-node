# Snowbridge PoC implementation specification, draft v1

22 October 2026. Companion to the [architecture](snowbridge-runtime-ledger-integration-spike.md),
[Ethereum reuse](snowbridge-ethereum-contracts-spike.md),
[Compact](snowbridge-compact-token-usability-spike.md), and
[E2E strategy](snowbridge-e2e-testing-strategy.md) reports. This document makes
recommended implementation choices; it is not a deployed protocol or a claim of
passing compatibility tests. Code blocks are specifications/pseudocode unless
explicitly stated otherwise.

## Decisions to implement first

| Area | Draft v1 decision | Evidence still required |
| --- | --- | --- |
| Solidity/relayer base | Upstream Snowbridge `caeea22dbee4d4da06266b205a61c89484c4e89f`; port only the standalone proof construction from DataHaven `4bebe7032d866ba6b57cda3767afaf586a92680e` | Compile and execute the compatibility gate below |
| Proof entry point | Preserve upstream V2 submit ABI; zero unused parachain proof fields | Live message/MMR proof accepted by the adapted hook |
| BEEFY | Fiat–Shamir entry point only, patched to verify full quorum; disable interactive mutation paths | Relayer bitfield/signature compatibility, rotation, rejection tests, gas |
| Token | One conventional ERC-20, one native unshielded color, integer base units, no ETH asset/fee bridging | Native issuance and wallet/contract/replay fixture |
| Retirement | A separately deployed Compact escrow with one `retire` operation, sealed maintenance authority and append-only records | Compiled artifact, exact initial-state verification and funded application |
| Escrow reading | New versioned, schema-specific bounded host accessor over committed ledger state | Compiler-generated layout, measured bound, both database modes |
| Queue linkage | Runtime `ProcessMessage` wrapper around the public outbound processor, with atomic nonce-to-attempt recording | Queue rollback/retry tests; switch to SDK callback only if the wrapper cannot satisfy them |
| Local Ethereum | Geth `v1.17.1`, Lodestar `v1.40.0`, mainnet preset, Fulu/Osaka active from genesis, matching the inspected upstream recipe | Actual light-client update and execution-receipt proof; source support is not interoperability proof |

These choices refine the alternatives in the earlier reports. A failed gate
should produce a documented decision change, not a second, weaker verification
path. Revisit production parameters and governance separately.

## 1. Repository changes and dependency order

| Repository/component | Concrete work | Depends on / completion artifact |
| --- | --- | --- |
| `midnightntwrk/midnight-ledger` | New serialized `CreditEthereumAsset` variant, validation/application, cost accounting, duplicate-output rejection, serialization compatibility and JS binding support where exposed | Existing pinned ledger baseline; new crate revision and schema fixtures |
| `midnight-node/ledger` | Restricted Ethereum constructor/executor host capability; replay support; bounded escrow accessor using existing typed contract state | New ledger revision; versioned host functions and storage-mode tests |
| `midnight-node/pallets/midnight-system` | Dedicated Ethereum capability invoking `apply_and_emit`; do not widen Cardano/governance allowlists | Host capability; authority matrix and replayable events |
| New `midnight-node/pallets/ethereum-bridge` | Frozen route registry, pending/consumed deposits, redemption/attempt storage, verified result receipts, queue wrapper | Protocol below; pallet tests and weights |
| `midnight-node/runtime` | Ethereum client/inbound/outbound/message queue configuration; fixed-width outbound MMR root; Ethereum authority conversion; bridge origin and limits | Pin-compatible SDK integration; runtime Wasm, metadata, dev chain spec |
| `midnight-node/node` and runtime APIs | Archive/MMR proof serving and any missing outbound proof API plumbing; API-version checks | Runtime APIs and actual historical proof fixture |
| Editable Snowbridge fork | Narrow Gateway ingress/policy/initialization patches; standalone verification hook; BEEFY quorum/path patch; local deploy script | Solidity diff, ABI/bytecode manifest, actual-verifier tests |
| Same Snowbridge fork, `relayer/` | Midnight metadata/endpoints, full signed BEEFY payload, standalone MMR builder, V2 receipt route to custom pallet; preserve beacon/receipt proof generation | ABI/runtime metadata, live proofs and resumable cursors |
| `midnight-node/util/toolkit-js` + Rust toolkit | Pinned compiler; escrow and independent consumer fixtures; funded input composition; updated ledger libraries/replay | Compiled operations/verifier keys, deployment and transaction fixtures |
| Selected indexer/wallet packages | Recognize new serialized ledger transition, consume matching ledger version, display the color and reconcile outputs | Cold replay and exact finalized balance checks |
| `midnight-node/tests` / local orchestration | Compatibility fixture producer, independent observer, live flows and fault harness | Gates in the E2E report |

The separate indexer/wallet repositories and their exact revisions have not been
selected by this spike. Resolve them from the intended client deployment before
claiming browser-wallet readiness; the Rust toolkit is the initial client.
Do not invent repository ownership or assume the installed npm packages already
support the new ledger variant. No Compact language change is proposed.

Use editable checkouts, not Cargo cache edits. Preserve the node's per-crate
ledger pins: `Cargo.toml` patches ledger-v9, onchain-state, onchain-runtime and
other crates from distinct releases. A single blanket ledger git override can
create incompatible type/serialization instances. Update `Cargo.lock` and
matching JS dependencies deliberately. [Dependency declarations](../../Cargo.toml)

Delivery order: proof fixtures and escrow/compiler fixture first; ledger schema
and host capabilities next; pallets/runtime and relayer together; client/indexer
replay before enabling issuance. For this fresh-devnet PoC, distribute compatible
node/client artifacts and regenerate genesis before use. A live-network upgrade
would need a separate staged activation/migration plan. An older host must fail
explicitly on an unsupported capability, and an older client must not silently
skip the unknown issuance. Rebuild runtime metadata after storage/API changes.

## 2. Domain, asset identity and canonical encodings

All definitions here are proposed v1. `K` means Ethereum Keccak-256, **not** NIST
SHA3-256. `ABI(...)` means standard Solidity `abi.encode` with the exact declared
types, never packed encoding. `TAG(s)` is ASCII right-padded with zero bytes to
exactly 32 bytes; all tags below fit. Addresses are raw 20-byte values, genesis
hashes are raw 32-byte hashes, and integers use their declared unsigned types.
JSON hex is only a representation of those bytes.

```text
bridgeDomain = K(ABI(
  bytes32 TAG("MN/SB/DOMAIN/V1"), uint256 ethereumChainId,
  bytes32 ethereumGenesisHash, address gatewayProxy,
  bytes32 midnightGenesisHash, bytes32 deploymentSalt))

assetId = K(ABI(bytes32 TAG("MN/SB/ASSET/V1"),
               bytes32 bridgeDomain, address erc20))
tokenDomain = K(ABI(bytes32 TAG("MN/SB/COLOR/V1"), bytes32 assetId))
color = Compact.tokenType(tokenDomain, escrowContractAddress)

bridgeOrigin = K(ABI(bytes32 TAG("MN/SB/ORIGIN/V1"), bytes32 bridgeDomain))
depositId = K(ABI(bytes32 TAG("MN/SB/DEPOSIT/V1"),
                 bytes32 bridgeDomain, uint64 inboundNonce))
redemptionId = K(ABI(bytes32 TAG("MN/SB/REDEEM/V1"),
                    bytes32 bridgeDomain, bytes32 escrowContractAddress,
                    uint64 sequence))
attemptId = K(ABI(bytes32 TAG("MN/SB/ATTEMPT/V1"),
                 bytes32 redemptionId, uint32 attemptNumber))
```

The registry binds `assetId` to exactly one token, color, escrow and immutable
configuration. No mutable ticker/decimal mapping participates in authorization.
Do not substitute an EVM hash for Compact's actual token-color derivation; use
the existing ledger/Compact implementations and a cross-language vector.
[Compact library][compact], [token command](../../util/toolkit/src/commands/show_token_type.rs)

`depositId` deliberately identifies the source event, not its content. Store the
authenticated token, amount, recipient and origin alongside it. A conflicting
payload under an existing ID is an integrity error, not a second deposit. Define
the provisional issuance output intent identifier as:

```text
K(ABI(bytes32 TAG("MN/SB/OUTPUT/V1"), bytes32 depositId,
      bytes32 assetId, bytes32 color, bytes32 midnightRecipient,
      uint128 amount))
```

Use output index zero and an unshielded user owner. Confirm the ledger's output
identifier serialization in its fixture. The consumed-deposit marker survives
spending that output. A different sender resubmitting a proof changes no identity.

### Ethereum deposit envelope

Keep upstream `OutboundMessageAccepted(uint64 nonce, Payload payload)` and its
SDK decoder. Pass exactly 96 bytes as the raw `xcm` argument:

```solidity
abi.encode(uint8(1), bytes32(bridgeDomain), bytes32(midnightUserAddress))
```

ABI words 0/1/2 are version/domain/recipient. Version must be 1 with canonical
zero high bytes; reject trailing bytes, other versions, wrong domains and a
recipient failing the ledger's structural address checks. It is a raw unshielded
user key, not Bech32 text, a shielded address, or a contract address. Whether
someone controls that key remains the depositor's responsibility.

At the public call boundary, exactly one `assets[]` entry is:

```solidity
abi.encode(uint8(0), address(erc20), uint128(amount))
```

In the resulting authenticated event this becomes `Asset {kind: 0, data:
abi.encode(address(erc20), uint128(amount))}`. These are different layouts; test
both rather than feeding the public argument directly to the event decoder.
The event's asset amount is authoritative. The envelope contains no second
amount/color to reconcile. Require empty claimer, zero ETH value/execution fee/
relay fee, one native ERC-20 asset, positive bounded amount, configured token,
and raw-payload kind zero. Retain upstream exact custody-balance-delta checks.
[Solidity types][types], [SDK decoder][inbound-message]

Validate these conditions on Ethereum before custody movement and again in
Midnight. For a verified unexpected payload that nevertheless locked funds,
persist a bounded quarantine record containing its identity/digest and emit an
explicit reason; it is an unresolved custody liability, never an issuance or
automatic refund. Reserve capacity before accepting supported deposits. The
PoC must stop new deposits on capacity exhaustion, not silently discard them.

For deterministic v1 fixtures, enforce a shared **64-deposit lifetime cap** in
the Gateway and runtime, with **10^12 base units per deposit**. Count successful
deposit calls on Ethereum before committing custody and allow the runtime to
retain all 64 records permanently. This avoids requiring Ethereum to know the
runtime's transient queue availability. Registration does not consume a deposit
slot; bound its repetitions separately. The total issued fixture supply then
stays within 64 × 10^12 units, well below the ledger's signed balance bounds.

`CreateAsset` registration (`network = 0`) is a distinct message: validate the
preconfigured token, record/acknowledge registration idempotently, and mint
nothing. Registration must never let an Ethereum caller install an arbitrary
Midnight color or change the frozen registry. Do not require contiguous deposit
nonces: registration and other accepted source messages occupy the same stream.

### Outbound redemption message

```text
Queue Message:
  origin   = bridgeOrigin
  id       = redemptionId           // stable topic across retries
  fee      = 0
  commands = [UnlockNativeToken {token, recipient, amount}]

Ethereum InboundMessage:
  origin = bridgeOrigin
  nonce  = assigned by outbound queue processing
  topic  = redemptionId
  commands = [{kind: 2, gas: configured bounded gas,
               payload: ABI(address token, address recipient, uint128 amount)}]
```

The runtime constructs this from the registry and committed escrow record;
callers cannot supply commands. The initial SDK gas meter assigns 200,000 to
`UnlockNativeToken`; retain that as a fixture input and measure sufficiency for
the selected token. It is a handler limit, not the total `v2_submit` gas budget.
The accepted message's exact ABI bytes, including gas, determine its leaf hash.
[Outbound message types][outbound-types]

`attemptId` is runtime bookkeeping. It is not an extra field silently added to
Snowbridge's ABI. Persist `(outboundNonce → redemptionId, attemptNumber,
attemptId, messageHash)`; `topic = redemptionId` and nonce identify each result.

## 3. Runtime state and transitions

All storage and entry-point names below are proposed. Freeze the active registry
for this PoC; changes require a new disposable deployment and domain. Privileged
runtime upgrades remain an explicit trust assumption, not something the escrow
can prevent.

```text
Deposits[depositId] = Pending(authenticated fields)
                   | Issued(outputId, authenticated fields or their canonical digest)
Quarantine[sourceIdentity] = bounded reason + authenticated payload digest
Redemptions[redemptionId] = immutable {sequence, assetId, amount, recipient}
                           + currentAttempt + phase
Attempts[(redemptionId, attemptNumber)] = Queued | Accepted(nonce, messageHash)
                                      | Failed(receiptIdentity) | Succeeded(receiptIdentity)
ByNonce[nonce] = (redemptionId, attemptNumber, attemptId, messageHash)
```

Deposits: a verified supported event atomically becomes `Pending` with inbound
replay protection. `materialize_deposit(id)` reads only these stored fields,
applies the restricted serialized issuance, emits its bytes and changes the
record to `Issued` in one canonical transition. Capacity/cost failure leaves
`Pending`. Repeated calls to an issued deposit return an explicit already-issued
result with no second event or ledger effect. Unexpected duplicate content fails
closed. Test canonical-root rollback across the external ledger database.

Redemption records become eligible when a successful escrow action has committed
to the canonical ledger state. `export_redemption(sequence)` takes only the
sequence; it reads the configured escrow at the runtime's current ledger root,
derives the ID, and copies immutable fields. The runtime may enqueue within an
unfinalized block; that state is fork-local. Ethereum may act only after the
required finalized BEEFY/MMR proof. Do not let offchain best-head observation
stand in for that proof.

| Current state | Input and checks | Atomic result |
| --- | --- | --- |
| No runtime record | Valid committed retirement; no existing ID; queue ticket valid | Store immutable redemption; queue attempt 0; phase `Queued(0)` |
| `Queued(a)` | Queue processes matching bounded message successfully | Assign nonce and persist `ByNonce`; phase `AwaitingResult(a, nonce)` |
| `Queued(a)` | Yield/overweight/error | No accepted nonce mapping or committed outbound effect; remain queued or explicitly stalled |
| `AwaitingResult(a,n)` | Verified finalized Gateway result, matching topic and nonce, `success=true` | Mark attempt succeeded and redemption `Completed`; permanently terminal |
| `AwaitingResult(a,n)` | Same verification, `success=false` | Mark attempt failed and redemption `RetryEligible(a)` |
| `RetryEligible(a)` | Permissionless `retry_redemption(id)`; checked counter increment; valid new queue ticket | Atomically queue `a+1` with identical monetary fields and topic; new nonce comes later |
| Any pending state | Timeout, relayer claim, absent transaction or outer revert | No new attempt or refund; reconcile/resubmit the existing message |
| Any state | Duplicate identical verified result | No-op or explicit already-processed response; no additional transition |
| Any state | Wrong/stale nonce/topic, contradictory result, or result for an unaccepted attempt | Reject; preserve terminal/current state and emit diagnostic evidence where appropriate |

A duplicate old failure after a new attempt exists must not make the new attempt
retryable. A success for an old attempt already proven failed is a contradiction,
not permission to edit history. Halt new exports for investigation if an
authenticated contradiction is ever observed; do not create a payout/refund path.
Bound `attemptNumber` and total stored attempts in the devnet configuration.
At its cap, remain explicitly stalled. Capacity is not authorization to discard
old nonce bindings or reset IDs.

The false-result rule relies on exactly one ordinary ERC-20 unlock command and
the handler's reverted transfer. It does not generalize to arbitrary commands,
malicious/rebasing tokens or partially successful batches.

### Queue wrapper decision

The pinned outbound pallet exposes the public `ProcessMessage` trait, but its
`do_process_message` explicitly does not roll back on error. Implement a wrapper
as the message queue's processor:

1. Bound and fully decode the queue bytes; check origin, stable topic, expected
   immutable command and `Queued(currentAttempt)` before delegation.
2. Charge wrapper work through the provided weight meter, including decode and
   storage access. Exhaustion must yield before mutating monetary state.
3. In one FRAME storage transaction, call the underlying public processor, read
   the resulting nonce and accepted message, verify it is exactly the expected
   new message, then write `ByNonce` and the accepted attempt state.
4. Commit only on complete success. Roll back all underlying queue-pallet writes
   on errors/mapping failure; consumed meter work is not refunded. Do not scrape
   emitted events as the authoritative nonce mapping.

Single-threaded runtime execution makes the read-after-delegate feasible, but
the fixture must confirm queue error semantics and access to the resulting
message. If that cannot be implemented cleanly using public storage/traits, add
a narrow accepted-message callback to an editable SDK fork. Do not guess the
nonce at enqueue time. Reject an unexpected origin; no other pallet should have
an unrestricted path to this deployment's outbound processor.
[Processor][processor], [outbound implementation][outbound]

### Delivery receipts

Add a permissionless `submit_redemption_receipt(proof)` using the same Ethereum
light-client verifier and receipt/log validation as Snowbridge. Decode the exact
Gateway `InboundMessageDispatched` event, including `success`; validate the
configured emitting proxy, authenticated log, nonce, topic and accepted attempt.
Use `(finalized execution block hash, transaction index, log index)` as receipt
evidence identity. Relayer/reward address is not the release recipient.

Own attempt records must not depend on `PendingOrders`: stock reward receipt
processing can remove that order without checking success. With fees zero,
reward cleanup is separate and idempotent; a missing order must not reject an
otherwise valid safety-critical result receipt. A reward-relay submission is not
automatically a call to the custom receipt endpoint. Update its routing or add
an explicit worker. Preserve terminal nonce bindings through the PoC lifetime.

## 4. Compact escrow and runtime reading boundary

### Contract sketch

Use a small fixed lifetime capacity for the fixture: **64 redemption records**,
one token and at most **10^12 base units per retirement**. These are proposed
test limits, not economics or a production capacity recommendation. Once full,
the escrow rejects new retirement actions; existing records remain exportable.
Wallet splitting can consume slots, so document this liveness limitation.

Deposit and retirement caps count different actions: partial retirements can
fill the escrow while spendable supply remains. The orchestrated fixture must
budget its retirement slots before deposits. Hitting that limit is a failed
capacity/liveness condition for existing holders, not proof that their funds
can be recovered by redeploying an escrow with a different color. Production
needs a scalable record design before accepting unrestricted deposits.

The sketch below deliberately avoids committing to uncompiled Compact syntax:

```text
immutable configuration established by constructor:
    schemaVersion = 1
    bridgeDomain: Bytes<32>
    tokenDomain: Bytes<32>

public ledger data:
    nextSequence: Uint<64> = 0
    totalRetired: Uint<128> = 0
    records: Map<Uint<64>, {amount: Uint<128>, recipient: Bytes<20>}> = empty

only exported spending/effect circuit:
    retire(amount: Uint<128>, ethereumRecipient: Bytes<20>) -> Uint<64>:
        disclose amount and ethereumRecipient
        assert 0 < amount <= 10^12
        assert ethereumRecipient != zero-address
        assert nextSequence < 64
        sequence = nextSequence
        assert !records.member(sequence)
        color = tokenType(tokenDomain, kernel.self())
        receiveUnshielded(color, amount)
        records.insert(sequence, {amount, recipient: ethereumRecipient})
        totalRetired = checked_add(totalRetired, amount)
        nextSequence = sequence + 1
        return sequence
```

The configuration is ordinary ledger data with no circuit that changes it, not
a presumed special immutable-language keyword. The compiled operation must
bind funded receipt and record insertion to the same successful segment.
No witness authorizes payment; real signed inputs/compatible contract effects
fund the receipt. There is no payer-based withdrawal right, so a separate caller
identity check is unnecessary for this retirement operation. A payer may choose
another Ethereum recipient. All effects/recipient must remain bound by the
signed transaction; a copied request cannot redirect somebody else's funds.

The sequential key avoids caller-selected-ID collisions but introduces
contention: two transactions built for the same next value cannot both commit
that record. Rebuild the losing transaction after inspecting committed state;
an outer extrinsic success is not enough. Do not add a permissionless overwrite,
delete, refund, send, mint, config update, or maintenance circuit.

### Sealing is a ledger property, not just source-code review

The inspected `ContractMaintenanceAuthority::new()` uses **empty committee,
threshold 1, counter 0**. Maintenance verification requires enough signatures
and membership for each signer; ordinary maintenance therefore cannot satisfy
that configuration. Do not use threshold zero and do not substitute a supposedly
destroyed private key. The ledger supports inserting verifier keys and IR through
maintenance, so omitting an upgrade circuit alone is insufficient.
[Authority type][authority], [maintenance verification][verify], [updates][structure]

At activation, a trusted setup check and the runtime accessor must establish:

- Exact escrow address and initial deployment serialization/nonce from the
  manifest; expected domain and token namespace; empty records, zero sequence,
  zero total retired and zero bridge-token balance at the activation block.
- Exactly the expected operation/IR/verifier-key versions for `retire`, and no
  extra entry point or alternate verifier version. A hash of the `.compact`
  source alone does not authenticate deployed behavior.
- Empty maintenance committee and threshold one. Pin the operations/schema
  fingerprints generated with the selected compiler and ledger.

This initial-state check is essential: the ledger rejects nonzero deployed
contract balances, but it does not thereby prove arbitrary application records
were created by paid calls. A malicious initial map could contain unfunded
redemptions. Never register a contract by address and source filename alone.
After activation, recheck sealed authority/configuration during access and rely
on the reviewed operation invariant for append-only funded records. Runtime
governance/ledger upgrades can still change the rules; this is not immunity to
the chain's trusted administration. [Deployment validation][verify]

### Bounded accessor specification

Existing `LedgerApi::get_contract_state` fetches and serializes the whole typed
contract state. It is suitable for initial exploration, but checking output
length after serialization is not a bound on the work already performed.
Choose a new versioned host capability provisionally named
`get_bridge_redemption_v1(state_key, configured_contract, sequence)`.
[Current implementation](../../ledger/src/ledger_9/mod.rs)

Implementation requirements:

1. Load the canonical ledger at the supplied runtime-owned state key; find the
   contract using existing `LedgerState::index`/typed state access. The extrinsic
   must not accept a user-selected state root or contract address.
2. Decode only the pinned schema and bounded record path. Check the fixed
   configuration, sealed authority, operation identity, count <= 64,
   sequence < count, positive bounded amount and nonzero recipient.
3. Confirm the ledger's actual unshielded balance covers `totalRetired` and the
   requested amount. This is a defense in depth check; balance alone does not
   authenticate a record or prevent reuse. Cross-check sum-of-records against
   `totalRetired` in fixtures and initial/full validation.
4. Return a fixed-size typed result or an explicit error. Charge a measured
   worst-case host/runtime weight; test both database modes. No mutation, Compact
   execution or client-supplied witness is performed by this accessor.

Proposed success payload, encoded in this order using SCALE fixed-width values
(integers little-endian; arrays have no length prefix):

```text
magic: [u8;4] = "MNBR", version: u8 = 1,
bridgeDomain: [u8;32], escrow: [u8;32], sequence: u64,
amount: u128, recipient: [u8;20], color: [u8;32],
totalRetired: u128, count: u16, ledgerBalance: u128
```

Success payload length is **179 bytes**, excluding the host's outer Result
encoding. Runtime decoding must consume all bytes. Missing contract/record,
schema mismatch, unsupported code, unsealed authority, bad range and budget
exhaustion are errors and cannot enqueue a command.

The exact state-tree paths are **not yet specified**: they must come from the
0.34.0 compiler's generated layout and a committed-state fixture. Check that
layout plus Rust decoder vectors into the implementation. Do not infer offsets
from the pseudocode or trust a JS-rendered JSON object at runtime. This artifact
and a measured bound are blocking requirements for the accessor.

### Deployment order without circular identifiers

1. Start fresh Ethereum and Midnight networks with bridge issuance/exports
   inactive. Midnight genesis must not depend on addresses that depend on that
   genesis. Record both genesis hashes and actual BEEFY bootstrap sets.
2. Deploy Ethereum verifier/executor/implementation/proxy. Add a narrow
   namespaced bridge configuration initialized atomically with the proxy:
   chain/genesis identifiers, Midnight genesis, salt and token allowlist. Compute
   `bridgeDomain` using `address(this)` in proxy context. Do not add an externally
   callable reinitializer or store the proxy-dependent domain as an implementation
   immutable. Upgrades/V1 remain disabled for the PoC.
3. Compute `assetId` and `tokenDomain`, which do not depend on escrow. Deploy the
   sealed escrow with those constructor inputs, empty records and no mint path.
   Its address is determined by the ledger deployment object.
4. Compute `color = tokenType(tokenDomain, escrowAddress)` after deployment;
   `retire` computes the same value using `kernel.self()`. No constructor needs
   to embed its own address or resulting color.
5. Activate the runtime registry once after checking exact escrow initial state
   and proofs/bootstrap. Register the ERC-20 through the dedicated Gateway and
   test its registration event. Submit the first deposit only after activation.

Guard the pre-activation operational window in the local deploy driver: no test
user is funded/started until setup is complete. A production deposit-enable
handshake would require a deliberate additional policy. Redeploying a proxy or
changing genesis changes the domain; previously locked/issued assets are not
automatically migrated to the replacement.

## 5. First executable compatibility gate

Implement this before wiring the whole ledger bridge. It is a diagnostic
runtime/message producer plus the actual adapted Solidity verifier. Fixture-only
authority to inject an outbound message must not enter the acceptance/production
runtime; label its artifacts as transport verification, not authenticated
retirement or a complete bridge.

### Selected baseline and patch surface

- Node/SDK/ledger: the exact revisions in the architecture report; four distinct
  Midnight BEEFY authorities; preserve complete signed payload entries, including
  Midnight's custom entries. Use Ethereum authority conversion and a fixed
  32-byte outbound MMR extra field.
- Solidity: upstream commit above, compiler `0.8.34`, explicitly selected
  `evm_version = "prague"` for the fixture, pinned dependency submodules and
  production compiler settings recorded with bytecode. This target is a proposal
  to test on the Osaka-active EL, not an already successful compilation.
- BEEFY: enable `submitFiatShamir` only. Patch its required signature count and
  `createFiatShamirFinalBitfield` consistently to verify
  `q(N) = N - floor((N-1)/3)` distinct members. For N=4, verify three. Disable
  `submitInitial`, `commitPrevRandao`, and `submitFinal` mutations so the weaker
  interactive path cannot remain reachable. Preserve all membership, padding,
  signature, stale-commitment and authority-transition checks.
- Relayer: use the same upstream tree's existing `EnableFiatShamir` path, adapted
  to the changed bitfield helper and Midnight proofs/metadata. The existence of
  this writer is source-confirmed; the adapted path is untested.
- Local Ethereum: Geth `v1.17.1` and Lodestar `v1.40.0`, the versions pinned by
  upstream `set-env.sh`; real Engine API, mainnet preset, Fulu/Osaka at genesis.
  Generate matching explicit fork versions/epochs in runtime and relayer;
  matching fork names alone is insufficient. Use isolated chain ID 31337 and
  generated local genesis, not the upstream recipe's Sepolia ID.

The SDK's `ForkVersions` includes Fulu and upstream has that local recipe. These
facts justify this first candidate, but do not establish working receipt proof
compatibility. Export EL/CL config, `genesis_validators_root`, actual checkpoints
and runtime light-client configuration in the manifest.
[Client versions][set-env], [Ethereum recipe][deploy-ethereum],
[SDK forks][beacon-types], [BEEFY relayer][beefy-writer]

### Fixture steps and pass criteria

1. **Wire-only:** encode the 96-byte envelope, call-input asset, emitted event,
   identifiers and unlock tuple independently in Rust and Solidity; bytes and
   hashes must agree. Check malformed/trailing data. Save expected hex, not just
   two functions calling a shared encoder. Include Compact color separately.
2. **Actual signed commitment:** collect a live Midnight BEEFY commitment and
   authority proofs; deploy BeefyClient with the actual previous checkpoint and
   verify a later update. Never set `latestMMRRoot` through storage cheats.
3. **Message inclusion:** inject one diagnostic unlock, capture its message
   block, following MMR leaf and historical message proof, then prove inclusion
   against the client's authenticated root. All unused `Verification.Proof`
   header scalars/hashes are zero, digest list empty, head-proof position/width
   zero and head-proof list empty; reject any noncanonical values.
4. **Actual Gateway execution:** seed custody through the Gateway's normal
   ERC-20 deposit path, submit that proved command and check exact custody/recipient
   deltas, nonce consumption and success receipt. Do not use MockGateway's
   commitment bypass. Then exercise a conventional token that deliberately
   reverts its release and verify the nonce-consuming false receipt.
5. **Reverse verification:** prove the deposited event and both result logs
   using the real Ethereum client on Midnight. Registration, deposit and result
   logs must be decoded into their distinct message types/routes.
6. **Negative and rotation:** mutate proof/signature/amount/recipient/origin,
   duplicate signers, supply only two signatures for N=4, attempt disabled entry
   points, advance the root before submission and rotate the BEEFY authority set.
   Verify rejection or regeneration as specified without bypasses.

Use a **proposed 5,000,000 gas ceiling per BEEFY update and per Gateway submission**,
and a 30,000,000 local block gas limit. Budget them as separate transactions.
Retain real code/initcode size limits. These are acceptance targets, not measured
usage or statements about a public network's current limits. If the four-member
fixture exceeds the budget, stop and assess the verifier/encoding before scaling
the authority set. Record deployment gas, calldata bytes, proof size and the
per-stage update/submission gas. The upstream Foundry 8-billion gas setting must
not conceal failure. [Foundry configuration][foundry]

Required output: reproducible commands, exact versions and patch commits,
genesis/checkpoint/deployment manifest, raw commitment and proof files, expected
encoding hex, successful and negative assertion results, gas report and a list
of remaining bypasses. No public testnet is selected yet; choose one only after
checking that its live fork is supported by the finalized implementation.

## 6. Handoff exit conditions and unresolved work

This session delivers the draft protocol, state machine, escrow/accessor design,
repository work map and compatibility gate. The source review additionally
established maintenance sealing mechanics, the distinction between call-input
and event-asset encoding, the danger of unfunded initial escrow records, and the
deployment order needed to avoid self-referential configuration.

The next agent must still produce, in order:

1. Actual compiler `0.34.0` alignment (the inspected managed binary was
   `0.26.108`), compiled escrow/consumer artifacts and generated schema vectors.
2. Passing real-verifier transport fixtures with recorded gas and fork settings.
3. Serialized ledger issuance plus strict cold/incremental replay and storage
   rollback fixtures; new dependency pins for every affected consumer.
4. Runtime state-machine and bounded-accessor implementation, complete round
   trip and failure/retry tests from the E2E strategy.

Neither this document nor a packaged copy establishes that any of those gates
has passed. Remaining production work includes upgrade/recovery governance,
pause policy, sustained storage/queue capacity, public-network compatibility,
economic security, independent audit and supported wallet/indexer rollout.

[compact]: https://github.com/LFDT-Minokawa/compact/blob/1f671fc27818df2b2676b3a97f85b2b821756243/compiler/standard-library.compact
[authority]: https://github.com/midnightntwrk/midnight-ledger/blob/400e40b92550638831486cecc71861f9b4e56642/onchain-state/src/state.rs
[verify]: https://github.com/midnightntwrk/midnight-ledger/blob/48a978d7fc4342e8f5536eb95f47139477e1a0e8/ledger/src/verify.rs
[structure]: https://github.com/midnightntwrk/midnight-ledger/blob/48a978d7fc4342e8f5536eb95f47139477e1a0e8/ledger/src/structure.rs
[types]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/contracts/src/v2/Types.sol
[inbound-message]: https://github.com/shieldedtech/polkadot-sdk/blob/9c7455eabd0cc2e77da867e837dfb466b11fbcf6/bridges/snowbridge/primitives/inbound-queue/src/v2/message.rs
[outbound-types]: https://github.com/shieldedtech/polkadot-sdk/blob/9c7455eabd0cc2e77da867e837dfb466b11fbcf6/bridges/snowbridge/primitives/outbound-queue/src/v2/message.rs
[outbound]: https://github.com/shieldedtech/polkadot-sdk/blob/9c7455eabd0cc2e77da867e837dfb466b11fbcf6/bridges/snowbridge/pallets/outbound-queue-v2/src/lib.rs
[processor]: https://github.com/shieldedtech/polkadot-sdk/blob/9c7455eabd0cc2e77da867e837dfb466b11fbcf6/bridges/snowbridge/pallets/outbound-queue-v2/src/process_message_impl.rs
[set-env]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/web/packages/test/scripts/set-env.sh
[deploy-ethereum]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/web/packages/test/scripts/deploy-ethereum.sh
[beacon-types]: https://github.com/shieldedtech/polkadot-sdk/blob/9c7455eabd0cc2e77da867e837dfb466b11fbcf6/bridges/snowbridge/primitives/beacon/src/types.rs
[beefy-writer]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/relayer/relays/beefy/ethereum-writer.go
[foundry]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/contracts/foundry.toml
