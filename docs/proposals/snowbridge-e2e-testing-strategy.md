# Snowbridge bridge end-to-end testing strategy

Research recommendation, 22 October 2026. This extends the
[runtime/ledger design](snowbridge-runtime-ledger-integration-spike.md),
[Ethereum contract plan](snowbridge-ethereum-contracts-spike.md), and
[Compact feasibility report](snowbridge-compact-token-usability-spike.md).
No bridge implementation, running stack, or passing bridge test is delivered by
this document. Source revisions are those recorded in the main report.

The [draft v1 implementation specification](snowbridge-implementation-spec.md)
now supplies the candidate payloads/IDs, state machine, sealed escrow/accessor
boundary, fixed development limits and exact initial compatibility baseline.
Use those choices when instantiating the tests below, and preserve their status
as proposed until the corresponding fixtures pass.

**Build a layered suite with a real local Ethereum proof-of-stake network,
four Midnight validators, the adapted Snowbridge contracts and relayers, native
ledger transactions, and an observer that independently reconciles finalized
state.** A transfer succeeds only when its proofs, exact balances, application
effects, and replayed ledger roots agree. Logs and successful extrinsics alone
are insufficient.

The first investment should be compatibility fixtures and a funded Compact
consumer, followed by one complete round trip. Then add failure injection and
state-machine exploration. This isolates interoperability problems before a
large distributed test makes them difficult to diagnose.

## 1. Test layers and what each establishes

| Layer | Environment | Required evidence | Does not establish |
| --- | --- | --- | --- |
| Encoding and component | Rust tests, Foundry, Compact/compiler fixtures | Identical message bytes/hashes; authorization, replay, arithmetic, receipt semantics; rejected malformed inputs | Live finality, transport, or restart correctness |
| Ledger integration | Real ledger and host functions, runtime externalities, compiled Compact actions | Serialized issuance → native spend → funded consumer/escrow; block costs; rollback; replay roots | Ethereum proof compatibility |
| Local proof E2E | Real EL/CL, Midnight consensus, real verifiers, adapted relayers | Complete two-way transfer plus finalized success/failure receipt returned to Midnight | Public-network operational resilience or economic security |
| Fault/soak | Same stack with separate processes, partitions, crashes and competing relayers | At-most-once monetary effects and recovery when prerequisites return | Safety under arbitrary consensus compromise |
| Public testnet rehearsal | Independently deployed test contracts and compatible public Ethereum network | Real fork settings, gas limits, fees, RPC retention and latency | Production approval or an audit |

Mocks remain useful in component tests, but their reports must say which trust
boundaries are bypassed. Upstream `GatewayV2.t.sol` normally uses a MockGateway
with `setCommitmentsAreVerified(true)`; passing those tests is not evidence of
Midnight BEEFY/MMR verification. Keep the upstream regression tests, and add
tests deploying the actual adapted verifier and Gateway. [Upstream tests][gateway-tests]

## 2. Recommended local stack

| Service | Configuration and purpose |
| --- | --- |
| Ethereum execution and consensus | Start with Snowbridge's Geth + Lodestar local PoS recipe, real Engine API, local validator keys and archived execution/beacon data |
| Ethereum contracts | Dedicated chain ID; test ERC-20; adapted BeefyClient; AgentExecutor; Gateway implementation and proxy; initialized custody agents |
| Midnight | Four independent validator processes and an archive/RPC node, using the proposed bridge runtime, real GRANDPA/BEEFY and MMR indexing |
| Bridge workers | Beacon updates; Ethereum event proofs; pending-deposit materialization; redemption export; BEEFY updates; outbound message proofs; Ethereum delivery-result proofs |
| Contract/client tooling | Matching Compact compiler/runtime, Rust toolkit, proof generation required by the selected transaction path, isolated wallets, an ordinary consumer and immutable redemption escrow |
| Indexer | Compatible indexer and database, rebuilt from genesis in the acceptance profile; explicit support for the new serialized issuance |
| Observer | Separate read-only process polling canonical state, checking proofs/accounting, and writing an evidence bundle |
| Fault controller | Harness-owned process/network controls and test-only failpoints; separate from the observer and bridge authorization |

These worker roles may share binaries, but must expose distinct progress and
restart boundaries. Stock Snowbridge scripts target Polkadot/Bridge Hub; replace
their substrate topology, message routing and proof adapter. The runtime's new
materialization/export jobs are additional work. The upstream beacon-state
service may also be required by the pinned relayer; include it where its actual
configuration requires historical state. Do not assume the upstream one-command
testnet already implements this architecture. [Local stack][local-stack],
[orchestration][start-services], [relay configuration][start-relayer]

Use two profiles:

- **Development:** one EL/CL pair with local Ethereum validators and a small
  Midnight network. Suitable for proof plumbing and rapid reproduction.
- **Acceptance/fault:** four Midnight validators with distinct keys and storage,
  plus Ethereum validators distributed across multiple EL/CL pairs. For an
  explicit Ethereum partition experiment, four equally weighted groups make
  the fault assumptions easy to record. One pair running every Ethereum key
  cannot demonstrate survival of that pair's failure.

Four equal-weight Midnight BEEFY authorities require three distinct signatures
under the proposed full-quorum rule. Demonstrate continued finalization with
one unavailable validator and a safe stall with two unavailable, subject to the
configured consensus/session behavior. Test both enabled verifier submission
paths, or disable the unused path. Raising only Snowbridge's sampling minimum
does not establish full quorum; see the Ethereum report.

The upstream local Ethereum recipe uses a mainnet preset, eight genesis
validators and `execution.engineMock false`, with multiple forks activated at
genesis. Treat this as a starting point: explicitly align EL, CL, light client
and relayer fork schedules. A minimal SSZ preset or shortened slots/epochs is
acceptable only if every verifier supports that exact configuration. Retain a
production-preset compatibility run. Do not copy the script's network identity
or destructive reset behavior into a shared environment. [Ethereum recipe][deploy-ethereum]

Anvil is useful for Solidity unit/integration tests. Geth `--dev` simulates a
consensus client, so it cannot replace a real beacon-finality test. Likewise,
Snowbridge's mock-beacon script uses `execution.engineMock`; exclude it from the
proof acceptance profile. [Geth documentation][geth-dev], [mock script][mock-beacon]

Provision DUST separately and keep the ERC-20 fixture conventional, allowlisted,
non-rebasing and fee-free. Start with zero bridge supply and zero custody, zero
bridge ETH transfer value, and sponsored relay fees. Use release-mode Midnight
binaries. If the selected chain spec requires Cardano/partner-chain services,
reuse the repo's configured development dependencies; do not invent an Ethereum
dependency on Cardano or bypass Midnight committee configuration to make tests pass.

### Reproducible startup and readiness

Create an isolated directory/network and deterministic seed per run. Pin image
digests, source revisions, compiler versions, dependency locks, genesis files,
runtime Wasm, contract bytecode/ABIs, and fork schedules in `manifest.json`.
Record bootstrap beacon root, BEEFY set IDs and authority roots, application
origin, Gateway/proxy/agent addresses, escrow code/address, color mapping and
limits. Test keys are disposable; do not export real credentials into artifacts.

Preflight must fail on a mismatched compiler. In this checkout the requested
Compact version is `0.34.0`, but the inspected installed managed compiler
reported `0.26.108`. Resolve that before calling any fixture representative.
Also verify the actual supported host API and new ledger serialization in node,
runtime, toolkit, wallet and indexer. Use host/runtime API capability versions,
not hard-coded `spec_version` thresholds.

Readiness means progress, not just listening ports:

1. Ethereum reports a nonzero finalized checkpoint, execution is not optimistic,
   and its finalized execution hash agrees with the CL's execution payload.
2. Midnight produces finalized blocks and actual BEEFY commitments with the
   configured authority keys; an archive query and MMR proof can be served.
3. Bootstrap both verifiers from the declared local genesis/checkpoints, then
   accept a subsequent normal update through each actual verification path.
4. Confirm proxy initialization, agent ownership, exact bytecode, allowed origin
   and asset registry; verify unsupported command/V1 paths are disabled.
5. An actual funded Compact transaction applies; indexer and replay reach its
   finalized hash. An HTTP `/ready` response alone is not sufficient.

The Beacon API exposes finality checkpoints and execution-optimistic status;
epoch zero/zero-root before finality must not satisfy readiness.
[Beacon API][beacon-api]

No force-checkpoint, direct storage mint, synthetic finalized receipt or verifier
mock is allowed after bootstrap in the acceptance path. Deliberately exercising
an administrative recovery path belongs in a separately labeled test.

## 3. Observe each boundary, with durable identities

Propose structured bridge events/status queries and an append-only observer
timeline. These are implementation requirements, not existing bridge RPCs.

| Transition | Evidence the observer must retain and check |
| --- | --- |
| Ethereum deposit | Chain/Gateway identity, transaction and log index, exact token/amount/recipient envelope, agent custody delta, finalized execution block and beacon ancestry |
| Verified inbound message | Receipt proof and accepted light-client anchor; inbound nonce; decoded asset identity; durable pending-deposit record |
| Materialized issuance | Deposit ID consumed once; serialized `SystemTransactionApplied` bytes; actual native UTXO ID/owner/color/value; ledger root at finalized block |
| Ordinary contract use | Funded inputs, per-segment execution result, committed consumer balance/application state, recipient outputs and exact color |
| Redemption | Funded immutable escrow record in committed contract state; amount/color/Ethereum recipient; retirement ID; finality of source block |
| Queued and accepted export | Queue item identity, redemption/attempt ID, eventual accepted outbound nonce and topic, exact single-command bytes |
| Message commitment | Message block hash, message Merkle root/proof, containing MMR leaf/block/index, MMR root/proof and signed complete BEEFY commitment |
| Ethereum submission | Verifier update, Gateway receipt, nonce bit, `InboundMessageDispatched` topic/nonce/success, agent and recipient balance deltas |
| Completion or retry | Finalized Ethereum receipt proof returned to Midnight, matching live attempt, completed or eligible-to-retry state, subsequent nonce if any |

Link deposit IDs to the configured source chain, Gateway and nonce and to the
destination network/asset namespace. Link redemption IDs to immutable committed
records. Keep redemption ID, message topic, attempt ID and outbound nonce as
distinct fields. Queuing does not assign the final nonce immediately, and an
attempt failing on Ethereum is not a new redemption.

Use block hashes in every query. `midnight_contractState` defaults to best state;
pass the finalized block explicitly. Subscriptions accelerate observation, but
the observer must backfill blocks after disconnects and discard observations
from noncanonical forks. Persist cursors only with their observations. Read raw
node state as well as indexer state, and wait for the indexer to reach the same
hash before comparison. If its API cannot express that association, add a test
hook or report the assertion unavailable; comparing two moving tips is not proof.

Message proofs must be obtained at the outbound message block while that
historical state is available. MMR proof retrieval also requires the offchain
indexing/gadget path. Test the `on_finalize` outbound-root to next-block
`on_initialize` MMR-leaf mapping, including empty blocks, activation and delayed
relay. Do not derive the leaf index from the block number without checking the
configured MMR genesis. [Outbound API][outbound-api],
[node service](../../node/src/service.rs), [RPC wiring](../../node/src/rpc.rs)

Proposed metrics: finalized-head and verifier lag in blocks/slots; pending and
oldest deposit age; queue/attempt counts; finalized release awaiting receipt;
failed handler counts; retry counts; proof-generation/verification latency;
replay/indexer lag; and invariant violations. Put message IDs in structured
logs/traces, not high-cardinality metric labels. A stalled stage should report
its missing prerequisite and last verified block. Timeouts fail with evidence;
they never authorize minting, refunds, or a new withdrawal attempt.

## 4. Accounting and semantic oracles

Use a small integer reference model driven by finalized source events and
actual ledger effects, separate from relayer and pallet bookkeeping. Never
derive the expected balance by reading the very bridge counter being tested.
Cross-check it against custody `balanceOf`, enumerated fixture wallets/UTXOs,
contract ledger balances, escrow state, and bridge records.

For each asset, starting from zero, define:

```text
D = exact ERC-20 deposits into bridge custody
M = native value successfully materialized
R = native value permanently retired into redemption escrow
U = successful ERC-20 releases, finalized on Ethereum
C = spendable native value in wallets and ordinary contracts (excludes escrow)
P = D - M   // deposited value not yet materialized, including quarantined deposits
Q = R - U   // retired value not yet successfully released

C = M - R
custody = D - U = C + P + Q
P >= 0; Q >= 0
```

This is a **causally consistent finalized snapshot**, not two arbitrary latest
heads. The chosen Ethereum history must contain deposits used by the chosen
Midnight history; the chosen Midnight history must contain retirements behind
the chosen Ethereum releases. At stable checkpoints stop new test submissions,
drain the flow and establish those dependencies. During concurrent tests track
the event dependency graph and evaluate consistent cuts; label unmatched views
as incomplete rather than masking discrepancies with a numerical tolerance.

Use actual finalized releases for `U`, even if Midnight has not received their
success receipts. Otherwise a delayed acknowledgement falsely looks like
undercollateralization. Escrow retains native tokens: retirement is not burning,
so total ledger holdings including escrow equal `M`, not `C`.

Exclude unsolicited custody transfers in the baseline. Add a separate donation
test: independently identify surplus `X`, assert `custody = C + P + Q + X`, and
verify surplus never authorizes minting. Do not define `X` as an arbitrary
residual that can hide an error. If unsupported payloads can lock tokens, keep
them visible in `P` as quarantined liabilities and test the chosen recovery
policy; do not silently drop them from the accounting universe.

Aggregate conservation is necessary but insufficient: an attacker could receive
the wrong user's otherwise correctly backed tokens. Also assert, for every
deposit and redemption:

- Exactly the authenticated color, value and recipient are used, with no rounding
  or decimals conversion. No bridge call changes NIGHT/DUST issuance policy.
- Each deposit can materialize at most once, including after its original UTXO
  has been spent, and each redemption can cause at most one successful release.
- Every issuance has an authenticated deposit; every release has a funded
  retirement. Caller-controlled witnesses or application maps cannot authorize it.
- Receipt and application/redemption record occur in the same successful ledger
  segment. Partial failure creates neither an unfunded entitlement nor an export.
- A finalized false receipt permits only the specified next attempt. Success
  is terminal; stale/duplicate receipts and timeouts cannot create a new attempt.

Run cold ledger replay from genesis using normal transactions plus serialized
system events in execution order, checking roots at every affected block and
subsequent post-block update. Then repeat with snapshot restore + incremental
replay. Compare node, toolkit and compatible indexer/client interpretations.
Deleting or changing an issuance event in a replay fixture must cause divergence
at the affected block. This detects serialization/order omissions; reuse of the
same ledger engine means it is not an independent proof of ledger correctness.
[Fetcher](../../util/toolkit/src/fetcher/compute_task.rs),
[replay/cache](../../util/toolkit/src/fetcher/wallet_state_cache.rs)

## 5. First complete acceptance scenario

Use integer base units. The ordinary consumer is distinct from the issuer and
escrow, pins the bridge color, and has authenticated withdrawal logic with no
mint endpoint. Start with exact-value funding inputs to avoid making input
splitting a hidden prerequisite. Add change/multiple-input cases separately.

| Step | Required finalized result |
| --- | --- |
| Lock 100 on Ethereum for Alice | Custody increases by 100; unfinalized deposit cannot authorize issuance |
| Verify and materialize | Alice has exactly 100 native units, one deposit is consumed, replay roots match |
| Alice transfers 40 to Bob | Alice 60, Bob 40; normal ledger transaction, no bridge-specific spend |
| Bob pays consumer 40 | Bob 0, consumer 40, funded application entitlement committed |
| Separate balance-query call | `unshieldedBalance` sees 40 at the start of this call |
| Consumer sends 15 to Carol and later returns 25 to Bob | Carol 15, Bob 25, consumer 0; unauthorized caller fails |
| Bob retires 25 into escrow | Bob 0, escrow 25, exactly one immutable redemption record |
| Export and prove release | One `UnlockNativeToken`, correctly assigned nonce/topic, real message/MMR/BEEFY proof, Ethereum recipient gains 25, custody 75 |
| Prove finalized success back to Midnight | Redemption completed once; no replacement attempt is available |
| Restart/replay/indexer rebuild | Alice 60 + Carol 15 remain spendable; escrow 25 remains retired; custody 75; all roots/statuses converge |

Also test receive-then-query within one call: the balance query reflects the
start-of-execution snapshot, not the just-received total. Test a stale exact
balance assertion versus a threshold assertion under concurrent transactions.
Contract-to-contract sends need coordinated receiving effects; do not infer an
automatic callback from a recipient address. Details and source references are
in the Compact report.

The existing [minter script](../../scripts/tests/toolkit-tokens-minter-e2e.sh)
already builds a `receiveAndSendUnshielded` call with `--input-utxo`. Reuse its
transaction construction, but replace its final string-presence wallet check
with exact input/output, amount, contract-state and root assertions. Its
permissive mint/configuration contract is not the acceptance consumer.

## 6. Mandatory adversarial and recovery matrix

| Test family | Injection | Required result |
| --- | --- | --- |
| Proof integrity | Mutate receipt branch, log index, header, amount, recipient, message leaf, MMR branch/index/root, authority proof or signature | Rejected at the intended boundary; no unauthorized ledger/custody effect |
| Domain separation | Wrong Gateway, chain/deployment, configured origin, asset, destination namespace, authority set or bootstrap | Rejected; two deployments use distinct keys/configuration so shared development keys cannot conceal replay exposure |
| Quorum/rotation | Too few or repeated signers; malformed bitfields; old/new authority transition; altered complete BEEFY payload | Every enabled submission path enforces configured distinct quorum; only valid set transitions advance |
| Deposit replay | Concurrent materializers, restart, duplicate proofs, replay after issued UTXO was spent | One issuance; pending/consumed state and canonical root agree |
| Bad deposit | Unregistered token, wrong/oversized envelope, zero/overflow amount, malformed recipient, transfer-fee token | Defined reject/quarantine behavior; no incorrect mint; upstream exact custody-delta rejection retained |
| Bounded resources | Full ledger block, message queue saturation, oversized contract state, maximum supported amount and aggregate `i128` balance limits | Bounded cost; pending deposit survives deferred materialization; no fullness reset or nonce/order loss |
| Compact funding | Missing/wrong-color/insufficient input, double-spent UTXO, failed payment segment, arbitrary withdrawal caller | No funded application effect/redemption/export; authorized successful path still works |
| Escrow permanence | Alternate withdrawal/mint/maintenance/upgrade path; replayed record | Retirement cannot become spendable again; record consumed once; code policy enforced |
| Queue delay | Defer acceptance across blocks; process several messages; retry after enqueue/accept crash | Assigned nonce maps to exactly one attempt; delivery enqueue is not reported as final acceptance |
| Ethereum outer revert | Invalid proof, insufficient submission gas, transaction replacement/reorg | No finalized consumption; safe resubmission of the same message/nonce after reconciliation |
| Ethereum caught failure | Conventional test ERC-20 deliberately reverts on the selected release; Gateway emits `success=false` | Nonce consumed, no transfer, no completion; only a finalized verified false receipt enables one new attempt |
| Success ambiguity | Drop RPC response after release; delay result relay; restart competing senders | Reconcile canonical receipt; never create a new nonce on timeout; exactly one recipient gain |
| Receipt replay | Duplicate/stale/wrong-topic/wrong-nonce/forged-result receipt, including failure arriving after success | No extra retry, no false completion; records remain terminal where appropriate |
| Privileged surfaces | V1 ingress, foreign mint, arbitrary Agent call, upgrades and wrong-origin commands | Disabled/disallowed as configured; preserve upstream custody-agent arbitrary-call guard |
| Proof retention/races | Relay delayed past several roots; advance Ethereum BeefyClient root before message submission; prune/miss archive data | Regenerate a proof against an accepted current root when possible; otherwise explicit unavailable state, never a proof bypass |
| Reorg/finality | Orphan Ethereum deposit/release before finality; orphan Midnight issue/retirement before finality; CL optimistic response | No destination action authorized solely by orphaned/nonfinal data; observer backfills/retracts; finalized state converges |
| Availability | Stop each relay/indexer/RPC endpoint; partition validators; exhaust relayer fee funds | Safety invariants continue to hold; progress resumes after dependencies return, without double effects |

For proof mutation tests, start from an accepted fixture and change one field.
Some adapter-policy tests need a freshly valid proof around a semantically
forbidden payload; otherwise they only test cryptographic rejection and never
reach the policy under test. Cover both cases.

Preserve upstream handler-failure, partial-command, fee-on-transfer and
custody-agent regressions. The PoC must enforce exactly one unlock command per
redemption message: upstream partial command execution makes a coarse false
result insufficient to infer that no transfer happened in a multi-command
message. Stock outbound receipt processing does not by itself provide the
proposed success-aware retry state machine. [Gateway tests][gateway-tests],
[outbound queue][outbound]

For crash consistency, add failpoints around (a) pending-deposit persistence,
(b) ledger storage writes before canonical-root publication, (c) successful
materialization before response, (d) outbound acceptance/nonce association, and
(e) Ethereum release before receipt acknowledgement. Exercise both `DbSeparate`
and `DbUnified` with restart and fork rollback. Compare canonical roots and
reachable state; immutable unreachable database entries need not disappear.
FRAME storage rollback alone does not demonstrate atomicity of the external
ledger database. [Ledger dispatch](../../ledger/src/lib.rs)

Use deterministic partitions/fork scenarios for pre-finality reorgs. Resetting a
finalized chain's database is not a realistic reorg test; any finalized-history
violation is an explicit consensus-assumption failure scenario. Do not claim the
bridge stays safe after its trusted consensus is compromised.

## 7. Stronger assurance than a few example transfers

Add property-based state-machine tests over deposits, materialization, spending,
retirement, queue acceptance, submission, false/success receipts, duplicates,
crashes and reordered observations. The model must distinguish Ethereum outer
revert from nonce-consuming handler failure. Check the per-ID and conservation
properties after every transition; shrink and preserve failing seeds.

A small bounded model (for example TLA+/PlusCal, or an exhaustive transition
explorer) is particularly valuable for retry races: explore two relayers, two
attempts, delayed/duplicated receipts and process crashes. Specify that there is
at most one live attempt per redemption and that a replacement requires final
evidence that its predecessor transferred nothing. Check safety for every
interleaving in the bound; state eventual-delivery/fairness assumptions for
liveness. This is proposed work, not an existing verified model.

Maintain cross-language golden vectors: Gateway ABI → Rust decoding; Rust
outbound bytes → Solidity leaf/root; compressed BEEFY keys → Ethereum authority
addresses; full signed payload → accepted commitment; color derivation → Compact
color. Include live-generated proofs as a separate fixture class. Run both a
normal update and authority rotation; cover Ethereum sync-committee transitions
and fork boundaries with valid fixtures, then a long-running live profile where
practical. Do not silently shorten mainnet sync-committee periods to fit CI.

Measure gas, proof size, runtime weight, ledger cost, queue backlog, and latency
per stage at target authority/message sizes. Upstream Foundry uses an unusually
high gas limit; a passing unit test does not show that full-quorum verification
fits the intended chain. Enforce real transaction/block gas and contract-size
limits and a compatible EVM fork. These measurements may bound the PoC authority
set or force a verifier redesign. [Foundry config][foundry]

## 8. Harness, CI and handoff deliverables

Recommended new layout, to be implemented:

```text
tests/bridge/                 # local orchestration, manifest, fixtures, observer/model
tests/e2e/tests/snowbridge.rs  # opt-in live flow assertions using shared Rust tooling
scripts/tests/snowbridge-e2e.sh
artifacts/bridge/<run-id>/    # ignored generated evidence, retained by CI
```

Add an explicit opt-in bridge feature/profile; ordinary E2E tests should not
suddenly require an Ethereum stack. Reuse existing toolkit/client utilities and
the node/indexer assertion pattern from
[tests/e2e/README.md](../../tests/e2e/README.md). Run ledger-heavy scenarios
serially within a process (`--test-threads=1`) because the documented ledger
global-state limitation can invalidate concurrent contexts. Parallel CI jobs
must use distinct processes, ports, chains, keys and databases.

Do not use `util/toolkit/test-images.docker-compose.yml` as a runnable stack: it
explicitly exists for dependency tracking and has a failing guard service. Also,
`scripts/tests/indexer-api-e2e.sh` is a readiness probe, not a bridge/indexer
consistency test. The existing `c2m_bridge` tests concern the Cardano path and
are reusable harness examples, not Ethereum coverage.

Suggested gates:

- **Every bridge change:** deterministic Rust/Foundry/Compact tests, negative
  vectors, state-machine properties, ledger/root replay and API/version checks.
- **Bridge PR acceptance:** isolated real-consensus round trip, finalized false
  receipt/retry, duplicate materialization, cold replay and matching indexer view.
- **Nightly or explicit fault run:** process/partition matrix, both database
  modes, authority rotation, concurrent transfers, long relay outage and soak.
- **Before a public PoC demonstration:** clean-machine reproduction, measured
  gas/weight limits, supported public-fork rehearsal and reviewed trust settings.

The evidence bundle should contain a manifest, reproduction command/seed,
machine-readable pass/fail assertions, the complete event timeline, raw headers,
receipts and proofs, serialized ledger events, root comparisons, balance tables,
contract ABI/bytecode hashes, gas/weight measurements and stage-specific logs.
On failure retain the first violated invariant and last consistent snapshot.
Observer failure, unknown schema, skipped mandatory checks and missing artifacts
must fail the acceptance run. Logs alone are not a result.

Set deadlines from the manifest's slot/epoch/session settings and queue limits:
separate Ethereum finality, beacon import, materialization, outbound acceptance,
BEEFY update, release finality and result-proof budgets. Record expected stalls
while a dependency is intentionally disabled. After restoring it, assert progress
within an explicit recovery budget and without administrative state repair.
Calibrate those budgets from measurements; a retry loop that eventually passes
without a bound does not establish useful liveness. Report observed timings
separately from guarantees that depend on honest participation and scheduling.

Implementation order:

1. Fix tool-version alignment; build ABI/BEEFY/MMR vectors with real Solidity
   verification and documented bootstrap trust.
2. Implement one serialized issuance and its replay; compile/fund the independent
   consumer and immutable escrow; establish segment/rollback tests.
3. Bring up local PoS and Midnight, then deposit → materialize with accounting.
4. Add retirement → queue/nonce → message/MMR/BEEFY → Ethereum release.
5. Return the finalized result proof; establish failed-release retry and all
   per-redemption terminal-state rules before calling the round trip complete.
6. Add independent observation, indexer rebuild, crash/reorg/rotation coverage,
   state-machine exploration, then public-network rehearsal.

## 9. Acceptance claims and limits

The PoC is ready to demonstrate only when the complete scenario and mandatory
safety cases pass without bypassing verification, from a documented clean
bootstrap, with inspectable evidence. Report excluded configurations and test
coverage explicitly rather than treating unexecuted rows as successful.

The strongest defensible claim is: **under the declared bootstrap, consensus,
governance, token and storage assumptions, the tested implementation preserves
backing, authenticates each issuance/release, prevents duplicate monetary effects,
and recovers from the tested availability failures.** Finite E2E tests do not
prove those properties for all executions. The bounded model, property tests,
independent proof encodings and replay checks strengthen different parts of the
argument, while code review/audit remains necessary for production.

Full-quorum development signatures do not inherit Polkadot's validator economics
or slashing; the inspected Midnight configuration has no BEEFY equivocation
reporting system. Governance/checkpoint changes remain trusted. Permanent escrow
also trades refundability for simpler safety: a permanently blocked token or
unavailable verifier can leave funds retired indefinitely. The tests should
show that condition honestly, not resolve it with an unsafe timeout refund.

[local-stack]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/web/packages/test/README.md
[start-services]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/web/packages/test/scripts/start-services.sh
[start-relayer]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/web/packages/test/scripts/start-relayer.sh
[deploy-ethereum]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/web/packages/test/scripts/deploy-ethereum.sh
[mock-beacon]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/web/packages/test/scripts/start-mock-beacon.sh
[geth-dev]: https://geth.ethereum.org/docs/developers/dapp-developer/dev-mode
[beacon-api]: https://github.com/ethereum/beacon-APIs/blob/master/apis/beacon/states/finality_checkpoints.yaml
[gateway-tests]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/contracts/test/GatewayV2.t.sol
[outbound]: https://github.com/shieldedtech/polkadot-sdk/blob/9c7455eabd0cc2e77da867e837dfb466b11fbcf6/bridges/snowbridge/pallets/outbound-queue-v2/src/lib.rs
[outbound-api]: https://github.com/shieldedtech/polkadot-sdk/blob/9c7455eabd0cc2e77da867e837dfb466b11fbcf6/bridges/snowbridge/pallets/outbound-queue-v2/src/api.rs
[foundry]: https://github.com/Snowfork/snowbridge/blob/caeea22dbee4d4da06266b205a61c89484c4e89f/contracts/foundry.toml
