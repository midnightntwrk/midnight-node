# Snowbridge token PoC: components and node integration

This is an isolated, fresh-genesis token-bridge PoC. The complete bidirectional
acceptance run passed on 2026-10-23: Ethereum deposit, native issuance, real-proof
Compact use, retirement, failed Ethereum release, authenticated retry and successful
settlement. The saved `ethereum-pos/run-fllxkhdn/validation.json` reports
`completeBridgeE2E: true` and `outboundRedemptionLive: true`.
The broader [acceptance strategy](../../docs/proposals/snowbridge-e2e-testing-strategy.md)
also includes recovery and production-hardening scenarios beyond this local profile.

The bridge runtime is opt-in through `ethereum-bridge-poc`. Normal ledger pins
remain unchanged. `prepare-node.py` creates an owned build checkout under
`artifacts/snowbridge/node` with explicit patched ledger and migration dependencies.
The migration patch makes the new state shape compile; migration of an existing
chain is **not validated or supported by this PoC**. Use fresh genesis only. The
standalone component harness retains its own workspace and lockfile. Original
research notes are preserved.

## SP1 outbound finality

The optional SP1/Avail-style prover and receiver integration is documented in
[the SP1 runbook](sp1/README.md). It reuses this complete token flow and substitutes
real Groth16 finality proofs for direct Ethereum BEEFY signature checks.
The fresh SP1 bidirectional run with committee-state protection passed on 2026-10-26:
[`run-mjrg6180/validation.json`](evidence/run-mjrg6180.validation.json)
records `completeBridgeE2E: true`, `sp1FinalityProved: true` and
`committeeStateVerified: true`. The guest binds the signed successor to the runtime
MMR and the relayer defers rotation boundaries until the snapshot confirms it.
Use a newly built guest key and a fresh receiver; existing immutable receivers do
not acquire the new checks automatically. See the SP1 runbook for the remaining
eclipse/freshness limitations.

## Local end-to-end runbook

For an operator-paced flow with Polkadot.js submissions and a local read-only
Compact/Ethereum state viewer, use the [manual walkthrough](MANUAL.md).
The commands below run the automated acceptance profile.

All chain execution, proving, relaying and validation runs locally: Geth and
Lodestar in Docker, the release Midnight node on the host, and host-side Compact,
Rust, Go and Solidity tools. Cardano observation uses the node's development mock
follower. No public Ethereum/Cardano network, cloud prover, AWS account or real
funds are needed. Initial setup downloads source dependencies, toolchains,
proving data and Docker images; an air-gapped bootstrap is not provided or tested.
The harness is an automated finite acceptance run, not a persistent bridge UI or
production relayer service.

### Prerequisites

The validated host is macOS arm64. Use Node 24.14.0 (minimum 22), Rust 1.98.1
with the targets/components in `rust-toolchain.toml`, Python 3, Git, curl, unzip,
tar, Docker with Compose, and the node's native build dependencies described in
[Rust setup](../../docs/rust-setup.md). A clang toolchain capable of building the
Wasm runtime is required; the validated machine used Homebrew LLVM on `PATH`.
Compact 0.34.0, Foundry 1.8.6 and Go 1.25.3 are prepared by the scripts on this
platform. This PoC uses its own Compact compiler, not the repository's 0.31 submodule.

Docker must be running. Keep the host awake throughout the test; the local PoS
validators need to continue producing slots. Cold builds take substantially
longer than the roughly 40–50 minute live acceptance run. Use a checkout of this
bridge branch, including its dependency patches and standalone test workspaces.

### First run from the repository root

```bash
# Select Node 24 with your installed version manager first.
# On the validated Apple Silicon host, expose the installed LLVM:
export PATH="/opt/homebrew/opt/llvm/bin:$PATH"
unset SKIP_WASM_BUILD
node --version
cargo --version
docker compose version

# Fetch pinned dependencies; compile/prove fixtures; run component checks.
./tests/snowbridge/run.sh

# Stage patched ledger dependencies, test hosts/runtime, build release + Wasm,
# and smoke-test an owned fresh Midnight node.
./tests/snowbridge/node-test.sh

# Start fresh Ethereum PoS + Midnight chains and run the full bridge flow.
python3 tests/snowbridge/pos-test.py
```

Run these commands sequentially and stop if a stage fails. `run.sh` alone covers
components and Anvil execution; `node-test.sh` alone covers node integration.
**Only `pos-test.py` runs the full live bidirectional acceptance flow.** It uses
the already built `target/release/midnight-node`; rebuild through `node-test.sh`
after changing runtime, node or ledger code. When sources and dependencies are
unchanged, repeating only `pos-test.py` creates another independent fresh run.

The default ports must be free:

| Service | Local port | Override |
| --- | --- | --- |
| Ethereum execution RPC | 18545 | `SNOWBRIDGE_POS_EL_PORT` |
| Ethereum beacon REST | 19596 | `SNOWBRIDGE_POS_CL_PORT` |
| Midnight RPC | 19944 | `SNOWBRIDGE_NODE_PORT` |
| Midnight P2P | 30334 | `SNOWBRIDGE_P2P_PORT` |

The earlier Anvil stage also uses 18545 and shuts down before the PoS stage.
Use only one harness at a time in this checkout: generated fixtures, build
artifacts and Foundry's latest broadcast paths are shared.

### Expected flow and acceptance

1. Create fresh DUST-funded Midnight genesis and read its actual chain bindings
   and BEEFY validator roots.
2. Deploy the local Ethereum contracts, deposit 100 test tokens and wait for PoS finality.
3. Prove/deploy the sealed Compact escrow, bootstrap the beacon client and freeze
   the registry through the development governance bodies.
4. Verify the finalized deposit in Midnight and issue 100 native tokens exactly once.
5. Deploy the consumer, deposit/query 100, withdraw 40 and retire 40 into escrow,
   using ordinary proofs, signatures and DUST fees.
6. Export retirement 0 through the SDK queue and relay actual BEEFY/MMR proofs.
   Deliberately fail the first release and authenticate its finalized receipt.
7. Retry with a fresh nonce, release 40 and authenticate completion on Midnight.
   Reject duplicates, premature retry, stale failure replay and retry after completion.
8. Independently check finalized receipts/balances and the serialized issuance;
   save the report and stop the run's node and containers.

The command prints its exact run directory under
`artifacts/snowbridge/ethereum-pos/run-*`. Success requires exit code zero and
that run's `validation.json` containing `completeBridgeE2E: true`,
`outboundRedemptionLive: true` and `outboundResult.phase: "Completed"`.
Expected balances are Ethereum custody 60 / recipient 40 and Compact consumer
60 / sealed escrow 40 / wallet 0. The sealed 40 is retired backing, not spendable
Compact funds. Substage reports such as `status.json` and node-smoke evidence
describe narrower checks and are not the final acceptance report.

### Monitoring, cleanup and evidence

Read `midnight-live.log` in the printed run directory for progress; Ethereum
finality waits can take several minutes without new output. `containers.log`,
`deploy.log`, `release-*.log`, receipt-proof directories and the node log help
diagnose failures. The Midnight database/log directory is recorded in
`midnight-node-smoke.json` on successful completion. Child command logs may only
be written when that command returns.

The harness stops its owned services in its cleanup handlers. After a hard
interruption, inspect and stop the specific Ethereum run with:

```bash
python3 tests/snowbridge/pos-stack.py status artifacts/snowbridge/ethereum-pos/run-REPLACE
python3 tests/snowbridge/pos-stack.py down artifacts/snowbridge/ethereum-pos/run-REPLACE
```

If a Midnight process survived a hard interruption, identify it by the owned
`artifacts/snowbridge/local-node/run-*` base path before stopping that process.
Do not remove unrelated containers or nodes. Preserve failed runs for diagnosis
and start a new fresh run after fixing the cause; do not rewind a PoS database or
disable validator slashing protection to reuse it. Existing Midnight databases
are supported only for explicit readback, not continuation of this acceptance flow.

Preserve the complete successful Ethereum run directory, its referenced Midnight
database/log directory, the tested node binary, `node-build-sources.json`, and
the source/patch changes when sharing the result. `validation.json` records
evidence and binary hashes. These artifacts are Git-ignored; committing source
alone does not preserve the evidence. Do not rerun `pos-report.py` after teardown
expecting an offline audit: it queries the live Ethereum endpoint. Saved file
hashes can be checked offline without restarting the chains.

### Other platforms

Linux and other architectures have not been validated end to end. Component
scripts accept `COMPACTC`, `FORGE` and `ANVIL`; the current PoS deploy/release
helpers specifically use `artifacts/snowbridge/tools/foundry/forge`, so a matching
platform binary must also be available at that path. Set `GO` to a Go 1.25.3
binary outside macOS arm64. See `AGENTS.md` for the Linux clang/Wasm workaround.

## Individual stages and tooling

Use Node 22+ (validated with 24.14.0), the repository's Rust toolchain, Python 3,
Git, curl, unzip and tar. From the repository root:

```bash
./tests/snowbridge/run.sh
```

The initial run downloads pinned sources, npm dependencies, Compact, Foundry,
Solidity and any missing Rust/proving dependencies. Automatic binary downloads
are provided for macOS arm64. Elsewhere set `COMPACTC`, `FORGE` and `ANVIL` to
matching local binaries. Compact must be 0.34.0; Foundry was validated at 1.8.6.
Archive checksums are recorded in `tool-archives.json` and checked before extraction.
The default Node 20 installation on this development machine cannot run these
Compact dependencies; select the installed Node 24 first.

Stages can also be run separately:

```bash
./tests/snowbridge/prepare.sh
./tests/snowbridge/test.sh
./tests/snowbridge/live.sh
python3 tests/snowbridge/report.py
# Requires the component fixture generated above:
./tests/snowbridge/node-test.sh
# Live bidirectional PoS profile (requires Docker and the built PoC node):
python3 tests/snowbridge/pos-test.py
```

`node-test.sh` runs the existing system-transaction regression tests, bridge
state-machine and real beacon-client vector tests,
then the real ledger host tests in separate and unified databases, builds the
release node including Wasm, and runs `node-smoke.py`. The smoke test owns a fresh
node/database, requires both bridge pallets in live metadata, waits for at least
three finalized blocks and queries the ledger root. It stops its own node on exit.
Ports default to RPC 19944 and P2P 30334; override with `SNOWBRIDGE_NODE_PORT` and
`SNOWBRIDGE_P2P_PORT`. Logs/databases remain under `artifacts/snowbridge/local-node`.
It does not activate a bridge registry or submit a cross-chain transfer.

`live.sh` starts its own disposable Anvil on `127.0.0.1:18545`, refuses to use an
occupied port, broadcasts against that chain, reads actual receipts/balances, and
stops its own process on exit. Set `SNOWBRIDGE_PORT=18546` to use another port.
Chain ID is 31337; the fixture uses public test accounts only. It does not connect
to a public Ethereum network or send any real funds.


`pos-test.py` owns a fresh Geth 1.17.1 / Lodestar 1.40.0 compose project pinned by
image digest. It uses eight validators with the mainnet SSZ preset (512-member
sync committee, 32-slot epochs), four-second slots, and Electra from genesis.
It generates a trusted checkpoint, deposits 100 test tokens, waits for finality,
and constructs the receipt/ancestry proofs using the pinned Snowbridge Go code.
The wire adapter includes the transaction index required by this SDK. The
helper reconstructs actual SSZ state proofs and checks them against beacon state
roots; it does not substitute an authenticated header or receipt.

The ingress profile also prepares a genesis with a development NIGHT/DUST fee payer and
no bridged tokens or contracts, submits a DUST-funded Compact escrow deployment, and uses both development governance bodies to bootstrap the beacon
client and freeze the bridge registry. Its driver submits the signed beacon
update, deposit proof and materialization calls, checks finalized ledger counts,
and rejects duplicate receipt submission and issuance. It saves the serialized
ledger issuance for a separate Rust decoder/replay test. The extended driver also
prepares a known wallet recipient and six separate DUST fee outputs, then submits
consumer deployment, deposit, balance query, withdrawal and escrow retirement.
It checks finalized contract state and balances. The earlier ingress-only evidence
used an unused synthetic outbound checkpoint and fixture chain-binding hashes.
The current bidirectional profile replaces those with the actual chain hashes and
validator roots, then exercises release and receipt settlement as described below.

PoS runs, receipts, proofs and node-driver logs are retained under
`artifacts/snowbridge/ethereum-pos/run-*`. The orchestrator stops only its own
containers and node. For interactive debugging, use `pos-stack.py up`, followed by
`pos-stack.py status <run-directory>` and `pos-stack.py down <run-directory>`.
`prepare-proof.sh` installs a checksummed Go 1.25.3 toolchain on macOS arm64; on
other systems, set `GO` to a matching installed binary. Allow roughly 40–50 minutes after dependencies and the node are built, including
Ethereum bootstrap, deposit and two result-receipt finality waits, plus the
two-body governance transactions.

The ignored `write_synthetic_issuance_for_fee_preflight` test is restricted to
`artifacts/snowbridge/compact-fee-preflight/ledger`. It supplies a clearly marked
synthetic issuance only to debug DUST-funded contract proving before starting a
live stack. The PoS orchestrator never calls it; live issuance always comes from
the node's finalized `SystemTransactionApplied` event.

## Implemented behavior

| Component | Implementation and validated scope |
| --- | --- |
| Solidity gateway | One configured ERC-20, canonical 96-byte deposit envelope, no fees/ETH, bounded amounts and 64 lifetime deposits. Uses existing exact-custody transfer and unlock handlers. V1 ingress, arbitrary agents and unsupported outbound commands are disabled. |
| BEEFY adapter | Full `q(N) = N - floor((N-1)/3)` signature quorum; interactive paths disabled. Actual ECDSA, authority membership, commitment and MMR verification; fixed-width standalone MMR leaf, canonical unused header fields. Five signed payload entries are preserved. Component tests use synthetic keys; the live profile reads the actual validator checkpoint and relays consensus-generated signatures. |
| Ledger patch | Serialized `CreditEthereumAsset`, ordinary native unshielded UTXO issuance, permanent deposit replay markers, duplicate-existing-output rejection, amount/NIGHT/capacity checks and cost accounting. The experimental node routes issuance through a caller-restricted Ethereum host and `MidnightSystem`; existing governance/Cardano allowlists remain unchanged. |
| Runtime ingress | PoC-only signed relay call allowance, separate from the safe-mode governance exception. One-shot root activation validates the committed empty escrow; signed proof submission uses Snowbridge’s real verifier; canonical decoding, quarantine, nonce replay protection and separate 64-deposit/65-message caps. Stored deposits alone reach the issuance capability; failed materialization rolls back and remains retryable. Fixed PoC weights still require benchmarking. |
| Ledger hosts | Validate the immutable escrow and read one redemption through fixed schema checks and a single map lookup. Reject unsupported operation hash, authority, domain/color, range and backing balance. SCALE response is 179 bytes. Both database modes, rollback/retry and serialized issuance replay are exercised. |
| Compact escrow | Compiled at 0.34.0, single `retire` operation, sealed maintenance authority, append-only bounded records, no mint/send/upgrade operation. Token color derives from the actual escrow address. |
| Compact consumer | Ordinary receive, query and send operations for that native color. This permissionless consumer is a test fixture, not an application authorization design. |
| Funded ledger round trip | Issue 100, deposit 100 into consumer, query balance, withdraw 40 to a signed wallet UTXO, retire 40 into escrow. Real Compact proofs, signatures and default strict ledger validation. Unfunded deposit/retire calls fail. |
| Replay and committed state | Serialize issuance and six transactions, deserialize and replay from a fresh in-memory genesis; roots match. Decode the Rust-produced escrow state with the generated Compact decoder and check its record, balance, sole operation and sealed authority. |
| Live PoS and Compact ingress | Real Geth/Lodestar finality and receipt proof, live Midnight issuance, then real-proof Compact deposit/query/withdraw/retire. Finalized balances 60/40, permanent duplicate rejection, and persisted-state readback. No outbound release. |
| Live bidirectional PoS bridge | Fresh Geth/Lodestar and Midnight chains, actual chain bindings and consensus-generated BEEFY/MMR proofs. Deposit 100, use Compact, retire 40, authenticate a failed release, retry with a new nonce, pay 40 and authenticate completion. Finalized Ethereum custody 60 and recipient 40; Compact consumer 60 and sealed escrow 40. |
| Live Ethereum execution | Actual Anvil deployment, deposit 100, proof-checked release 40, intentional failed release 10. Independent RPC observer confirms custody 60, recipient 40 and success/failure logs; pinned SDK decodes the freshly emitted registration and deposit logs. |

Ledger transaction/state serialization tags are explicitly changed to
`system-transaction[v9-ethereum-poc]` and `ledger-state[v18-ethereum-poc]`. This
requires a fresh ledger genesis and matching consumers. The stock node and JS
ledger package cannot apply this new issuance transaction. The JS package is used
only to build ordinary contract intents and decode unchanged contract state;
issuance and full ledger replay run in the patched Rust ledger.

The funded fixture sets the ledger fee price to zero to isolate token balancing
from DUST provisioning. It retains default proof/signature/balance validation.
Component replay is in-memory genesis replay. Separate node host tests cover both
database modes and persistence, but indexer restart and cold disk recovery remain
unvalidated. Initial issuance is invoked directly by the fixture;
it is not authenticated by an Ethereum receipt proof.

## Validation and evidence

The [public evidence snapshot](evidence/README.md) contains historical reports,
their SHA-256 manifest and a small real Groth16 proof/checkpoint fixture. Raw run
artifacts remain ignored. The snapshot records earlier local runs; rerun the
harness to validate a new build.

The complete fresh-genesis run
[`ethereum-pos/run-fllxkhdn/validation.json`](evidence/run-fllxkhdn.validation.json)
passed on 2026-10-23 with `completeBridgeE2E: true`. The one-command
`pos-test.py` harness exited successfully and stopped its owned node and containers.
It used actual Ethereum/Midnight genesis hashes, validator roots and live BEEFY
signatures, real Ethereum finality/receipt proofs, and ordinary DUST-funded
Compact proofs. No verifier repair or stack recovery was needed.

After depositing 100 tokens and retiring 40 through Compact, release nonce 1
deliberately failed. Midnight verified its finalized failure receipt before
allowing retry nonce 2, which paid exactly 40 to the immutable recipient.
Midnight then verified the finalized success receipt and marked the redemption
`Completed`. Duplicate export, premature retry, duplicate receipts, replay of
the old failure and retry after completion were rejected. The independent final
report checked canonical Ethereum receipts and finalized custody 60 / recipient
40 against Compact consumer 60 / sealed retirement 40. All 65 evidence hashes
and the tested release binary hash were subsequently rechecked.

Additional targeted checks passed: 13 pallet tests, three runtime relay/queue
tests, nine Solidity tests, and five JavaScript tests. The release node and Wasm
built successfully. Evidence and logs remain under `artifacts/snowbridge/`.

Earlier ingress-only milestones are recorded below for provenance.

The live Compact ingress run `ethereum-pos/run-glroxxvt/validation.json` passed
on 2026-10-23. An actual finalized Ethereum deposit backed 100 native tokens on
Midnight. Five normal, DUST-funded transactions deployed a consumer, deposited
100, queried its balance, withdrew 40 to the signed wallet, and retired those 40
into the sealed escrow. Final state contains consumer balance 60, escrow balance
40, no wallet UTXO of that color, and immutable retirement record 0. Ethereum
custody remains 100. Reissuance is rejected even after the original UTXO is spent.

All five transactions finalized before a JavaScript state-checker error
(`ChargedState` was treated as `StateValue`). After fixing the shared checker,
we reopened the same Midnight database as a non-authoring node, rechecked all
five successful events against canonical finalized history, and read/decoded
both committed contracts. The separate issuance replay and Ethereum custody audit
also passed. The original failure, repaired readback and evidence hashes remain
in that run directory. No transactions were resubmitted during readback. That
milestone established inbound bridging and Compact usability only; the later
fresh-genesis bidirectional run above also validates Ethereum redemption.

The live ingress run `ethereum-pos/run-to4hbrfp/validation.json` passed on
2026-10-22: Geth/Lodestar finalized an actual 100-token ERC-20 deposit; the live
Midnight beacon client verified its update and receipt; governance activated the
actual Compact escrow; the node issued 100 native tokens through the dedicated
ledger host. Duplicate receipt and issuance calls failed. An independent Rust
decoder checked the emitted issuance, owner, color and amount, applied it to the
fresh ledger genesis, and rejected replay. Ethereum custody remained 100. Normal
ledger fees were paid with a real DUST proof. The driver and both owned Ethereum
containers stopped after evidence capture. This establishes live ingress only.

On 2026-10-22 the release node and its Wasm built successfully, and the fresh
local Midnight node finalized three blocks with both bridge pallets in metadata
and a working ledger-root RPC. This is live Midnight block-production validation,
not a cross-chain transfer. The node uses the existing mock Cardano follower;
validator selection fell back to the configured single Alice authority.

`test.sh` runs the complete JavaScript suite through `npm test`, the native SP1
verifier suite, Solidity component tests and the public captured Groth16 proof,
then the Rust ledger fixtures (including the real-proof funded round trip) and
committed-state decoding. No SP1 proving toolchain is needed to verify the saved
proof. Both the fixture and its checkpoint can be overridden with
`SNOWBRIDGE_SP1_PROOF` and `SNOWBRIDGE_SP1_INITIAL`.
The negative cases include invalid amounts, lifetime capacity, deposit replay
after spending, conflicting deposit payloads, unfunded Compact calls, duplicate
and insufficient signers, stale/altered commitments, noncanonical proofs, disabled
entry points, wrong origin, command mutation, nonce replay and failed token release.

`live.sh` uses the real adapted verifier; it does not mock `_verifyCommitment` or
write its authenticated root through storage cheats. It enforces a 30,000,000
block gas limit and the normal code/initcode limits. Each BEEFY update and Gateway
submission must fit 5,000,000 gas. The 4-member authority fixture rotates set IDs
while retaining the same keys; changing membership under live consensus remains
untested. The small-set quorum test checks selection, not proof generation for
each size. MMR and message inclusion use single-leaf fixtures.

`pallet-ethereum-bridge` has 13 ordinary tests, including real BLS/header/receipt vectors
from the pinned SDK and rejection of unproven Anvil logs. Its state-machine tests
use explicit verifier/ledger doubles. `ethereum_escrow` uses actual generated
Compact state and actual issuance hosts; neither substitutes for a live transfer.

Outputs under `artifacts/snowbridge/` (ignored by Git):

- `node-validation.json`, `node-smoke-evidence.json`: node integration results when `node-test.sh` completes successfully.
- `node-build-sources.json`, `pallet-tests.log`, `host-tests.log`, `node-build.log`: staged source hashes and node test/build logs.
- `manifest.json`: source revisions, tool versions, source/patch hashes and evidence hashes.
- `tests.log`, `live.log`, `prepare.log`: complete stage output when using `run.sh`.
- `live-evidence.json`: actual transaction inputs, receipts, balances, per-transaction gas, code sizes/hashes and explicit consensus limitations.
- `deposit-logs.json`: freshly emitted logs decoded by the pinned SDK.
- `compact-ledger/evidence.json`: balances, replay roots, fixture addresses and committed redemption.
- `compact-ledger/issuance.bin`, `proven-*.bin`, `escrow-state.bin`: serialized Rust ledger evidence.
- `compact/`: compiler output, schema, prover/verifier keys and ZKIR.
- `solidity-out/` and `solidity-cache/`: compiler artifacts and local broadcast cache.

Foundry also writes ignored broadcast records under `contracts/broadcast/`.
The checked-in `fixtures/deposit-logs.json` is a separate static decoder vector
from an earlier live run, not a claim about the latest deployment.
`fixtures/failed-release-logs.json` likewise preserves both Gateway events from
a real failed release; `outbound.test.mjs` guards against confusing the command
failure diagnostic with the authenticated dispatch result.

## Outbound implementation and acceptance profile

The opt-in runtime now exports committed escrow retirements through the SDK V2
outbound queue. The FRAME message queue carries a bounded attempt number around
the canonical SDK message; the Ethereum topic remains the stable redemption ID.
The processor rejects stale attempts, changed money fields, wrong origins and
trailing bytes, and wraps SDK processing plus nonce binding in one storage
transaction. Consumed processing weight is retained on rollback.

The PoC accepts one message per block, so its message inclusion proof is empty.
The SDK computes the ABI message hash and commitment. The next block's fixed
113-byte, version-1.0 MMR leaf includes that commitment. Ethereum authority leaves
are enabled only with `ethereum-bridge-poc`; ordinary runtime builds retain their
existing leaf layout. The relayer keeps every item in the actual signed BEEFY
payload and converts the node's historical MMR proof using the pinned upstream
Snowbridge conversion, checking the signed root before submission.

`pos-test.py` now requests the complete bidirectional profile. It starts Midnight
with DUST-funded genesis before deploying Ethereum contracts, reads the actual
chain hashes and validator roots, then prepares and proves the escrow deployment.
After the existing 100-token ingress and Compact round trip it exports retirement
0 for 40 tokens, deliberately fails its first Ethereum release, verifies that
finalized failure on Midnight, retries with a new nonce, and verifies successful
payment and completion. The release script supplies an explicit gas allowance:
the Gateway must have its 200,000-gas command budget available even when the
test token intentionally reverts quickly. The final report requires custody 60, recipient 40,
spendable Compact balance 60, and permanently sealed retirement 40. An attempted
run is not validation: only a completed `validation.json` with
`completeBridgeE2E: true` establishes that this profile passed.

Regression checks cover forged/mismatched receipts, duplicate and stale results,
retry exhaustion, immutable recipient/amount, a persistent safety halt for
contradictory authenticated results, and SDK-write rollback after an acceptance
failure. Solidity component tests still use synthetic validator keys; the live
profile consumes signatures from the running node's BEEFY gadget.

## Work beyond the local PoC

The [SP1 feasibility investigation](../../docs/proposals/snowbridge-sp1-feasibility.md)
checks the sibling BEEFY prover against saved live commitments and describes the
minimal receiver/prover integration. That integration is now implemented and
validated in the [SP1 runbook](sp1/README.md); production operation, migration and
live multi-validator membership changes remain outstanding.

Implementation notes from the live runs:

- Read the runtime's actual authority proof (`keysetCommitment`) and preserve
  all five signed BEEFY payload entries; a fixture checkpoint does not validate
  live consensus interoperability.
- Supply enough outer transaction gas for the Gateway's command budget even
  when an intentionally failing token call returns quickly. Gas estimation for
  that failure path underestimated the required allowance.
- Select result receipts by Gateway address and exact dispatch-event signature.
  A failed release also emits `CommandFailed`; counting Gateway logs alone is
  incorrect. The captured two-log receipt is now a regression fixture.
- Decode `ChargedState.data.state` for Compact committed-state checks. The first
  ingress run exposed this checker error; the final fresh run needed no repair.
- Keep the attempt binding and SDK queue writes atomic, and only authorize
  retry after a verified finalized failure. The live run exercised that sequence
  with stable amount, recipient and redemption ID but distinct nonces.

Benchmark the fixed PoC weights and proof bounds, validate existing-chain ledger
migration and independent indexer/cold replay, exercise changed validator
membership, authority-boundary proof selection and recovery across restarts/reorgs, and implement durable relayer
cursors and multi-message proof APIs before treating this as a supported bridge.
The local acceptance profile deliberately uses one authority, one frozen asset,
one outbound message per block, at most 64 retirements and four attempts each.
