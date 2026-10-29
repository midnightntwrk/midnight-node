# SP1 BEEFY finality for the token bridge

This replaces only the outbound BEEFY signature-verification layer. The existing
Midnight issuance, Compact consumer/escrow, MMR/message inclusion, Ethereum custody,
and authenticated failure/retry/completion flow remain in place. Enable it with
`SNOWBRIDGE_SP1=true` on a **fresh** local deployment.

The receiver follows the verification boundary used by Avail Vector: an application
light client pins a guest key and delegates cryptographic verification to Succinct.
It is a Midnight BEEFY guest, not Avail's GRANDPA guest or a connection to Avail DA.
No guardian, verifier replacement, checkpoint override, or application callback is
exposed by `MidnightSP1Client`.

## Validated committee protection — 2026-10-26

The [public evidence guide](../evidence/README.md) describes report provenance
and the raw artifacts omitted from Git. These are saved results, not a new live run.

The fresh full bidirectional run passed:
[`run-mjrg6180/validation.json`](../evidence/run-mjrg6180.validation.json).
It records `completeBridgeE2E: true`, `sp1FinalityProved: true`,
`committeeStateVerified: true`, and redemption `Completed`. All **197** evidence
hashes and the release-node binary hash were checked after teardown. The owned
Midnight node and Ethereum containers are stopped.

| SP1 update | Verified source blocks | Local proof time | Ethereum submission gas |
| --- | --- | --- | --- |
| Initial release, intentionally failed | 148 → 252 | 224.91 s | 311,458 |
| Retry, paid 40 tokens | 428 | 278.74 s | 291,570 |

Both are real 356-byte Groth16 proofs accepted by the unmodified v6.1.0 verifier.
The first update crosses a live session rotation from committee ID 0 to ID 1;
Ethereum's current/next committee views were checked against the proved roots.
The retry extends that accepted checkpoint. Final balances are Ethereum custody
60 / recipient 40 and Compact consumer 60 / sealed retired escrow 40 / wallet 0.
Duplicate issuance/export/results, premature retry, stale failure replay and retry
after completion were rejected. The live profile still uses one validator key;
changed membership and counts are covered by native tests, not a live validator fleet.

Focused validation passed: **25 native tests**, **12 JavaScript tests**, **14
Solidity component tests**, and **one real-proof Solidity test**. The latter
reproduces the rotation boundary found during the first attempted run: block 176
has a preceding runtime snapshot, so selection defers it and proves blocks 184
and 256. That real Groth16 proof took 232.30 s and passed EVM verification,
tampering and replay checks. Its [public report](../evidence/committee-boundary-validation.json)
records the original capture hashes; the [evidence guide](../evidence/README.md)
explains the public proof fixture and omitted raw captures. The guest remained strict throughout;
the correction was in relayer selection.

Validated guest ELF SHA-256:
`c2c00e9bda3555dc4b44f6e5a3d6b5a774dcb5ae78c9a50dbe1d8785f1bebc4f`.
Program verification key:
`0x00b14d8d06202aa774f702bdd5ea82d64da5effb5dfee74a9cff01886f24fc66`.
These are local PoC measurements and identities, not production committee benchmarks.

## Committee-state protection

The `ethereum-address-stake-v2-committee-mmr` guest requires a runtime committee
witness for **every** selected BEEFY commitment, including intermediate rotations.
The initial checkpoint pins current and next authority IDs, slot counts and
stake-bearing roots. A rotation can only promote that authenticated next set;
an unannounced set, conflicting successor, skipped set or return to an old set
is rejected. Multiple consecutive rotations can be verified inside one batch.
The Ethereum receiver exposes `currentCommittee()` and `nextCommittee()` for
inspection; its checkpoint changes only after SP1 verification.

Each witness proves the **last** MMR leaf under that commitment's signed `mh`
root. Its parent number must be exactly the signed block minus one, its version
must be the supported standalone v1.0 layout, and its runtime next-set ID, count
and address-only root must match the signed `nb`/`ns` payload. This bridges the
runtime's address-only MMR format to the payload's address-plus-stake format
without treating them as interchangeable. Missing next-set payloads fail closed
in the deployed guest, even though the base wire verifier can parse them.
This uses the runtime's existing commitment; no runtime metadata change is needed.

The live rotation test exposed a boundary detail: the MMR leaf at the first block
of a new session can still contain the preceding session's next-set snapshot,
while the signed payload already announces the new successor. The relayer defers
that boundary target and selects a later signed commitment whose MMR snapshot
confirms the successor. Historical selection applies the same strict witness
checks before advancing its state. The guest does not accept a stale snapshot or
weaken its committee equality check to accommodate the boundary.

Committee checks use the state at the signed source block. A later source session
may start while a Groth16 proof is being generated or finalized on Ethereum;
that does not invalidate an earlier finalized commitment. These checks establish
committee authenticity and ordering, not that the destination has reached the
latest source tip.

`recoverCommitteeProofs` fetches each witness at the specific captured block and
persists `committee-BLOCK.json`. The host embeds the witnesses in `BatchInput`;
the guest independently verifies them. Current RPC authority lists cannot repair
a missing proof or override either committee. The fixed-tip proof algorithm is
cross-checked against Substrate's MMR implementation across 257 tree sizes.

This provides committee substitution and rollback protection under the BEEFY
quorum assumption. It is **not complete network eclipse prevention**: an isolated
relayer can still be stalled or served an older, otherwise valid prefix. There is
no authenticated source-time expiry in this version. Operators still need an
independently authenticated recent bootstrap checkpoint, diverse independently
operated source connections and freshness monitoring. A compromised signing
quorum can fabricate mutually consistent signed state; checking two values
committed by that same quorum is not an independent consensus trust anchor.

The new guest has a new program key. Rebuild it with `prepare-sp1.sh` and deploy a
fresh receiver; an existing receiver has an immutable old key and does not acquire
these checks automatically. Old saved jobs without committee witnesses must be
regenerated. Full production migration and multi-validator live membership changes
remain outside this local profile.

## Previous validated result — 2026-10-25 (before committee-state protection)

The full fresh-chain bidirectional run passed:
[`run-oiulvrxd/validation.json`](../evidence/run-oiulvrxd.validation.json).
It reports `completeBridgeE2E: true`, `sp1FinalityProved: true`, and redemption
`Completed`. All 145 evidence hashes and the node binary hash were rechecked.
The harness stopped its owned node and Ethereum containers after the final audit.

| SP1 update | Verified commitments | Local proof time | Ethereum submission gas |
| --- | --- | --- | --- |
| Initial release, intentionally failed | 5 → 285 | 216.79 s | 311,442 |
| Retry, paid 40 tokens | 305 → 473 | 218.86 s | 294,318 |

Both use real 356-byte Groth16 proofs and the unmodified v6.1.0 verifier. The second
proof extends the first accepted checkpoint. Ethereum deposit 100 → native issuance
→ Compact receive/query/withdraw/retire → failed release → authenticated retry →
release 40 → authenticated completion all passed. Final balances are custody 60,
recipient 40, consumer 60, sealed retired escrow 40 and wallet 0. Duplicate issuance,
deposit, export and result receipts, premature retry, stale failure replay and retry
after completion were rejected.

Separate checks passed: 17 native verifier/selection tests, 10 JavaScript tests,
13 baseline/receiver Solidity tests, and one real-proof Solidity test with tampering
and replay checks. Historical recovery retrieved 36 justifications from finalized
source blocks and matched every byte against the live captures. Reopening the
finalized first proof job reconciled its existing transaction. Recovery evidence
is included as `sp1/recovery-check.json` in the live report.

The preliminary captured-block proof took 1,036 seconds including the cold circuit
download and passed the real Solidity verifier. These are local single-authority
measurements, not production committee or cost benchmarks.

## Build and run

First prepare the existing PoC dependencies and release node as described in
[the main runbook](../README.md). On the validated macOS arm64 host, with Node 24,
Go 1.25.3, protoc, Rust 1.98.1 and Docker available:

```bash
bash tests/snowbridge/prepare-sp1.sh
SNOWBRIDGE_SP1=true \
SNOWBRIDGE_SP1_KEY="$PWD/artifacts/snowbridge/sp1-program.json" \
python3 tests/snowbridge/pos-test.py
```

The installer checks compiler archive hashes, registers the `succinct` rustup
alias, builds with the standalone Cargo lockfile, and derives the guest key from
the actual ELF. It leaves `../bridge-test` untouched. CPU proving is explicit;
there is no mock or paid remote-prover fallback. The first Groth16 proof downloads
approximately 6.2 GB of public circuit data into `~/.sp1/circuits` (about 7.8 GB
extracted). Subsequent proofs reuse it. Allow substantially more time than the
original direct-signature test. The live orchestration allows three hours.

If Homebrew clang points at a removed SDK on macOS, set `CC=/usr/bin/clang` and
`SDKROOT` to the installed Xcode SDK before running the harness. Linux installers
and remote proving are outside this tested profile.

Success requires the run's `validation.json` to have `completeBridgeE2E: true`,
`outboundRedemptionLive: true`, `sp1FinalityProved: true`, and a completed redemption.
It must contain two real, finalized SP1 submissions, including the intentional
failed release followed by retry. Expected balances remain Ethereum custody 60 /
recipient 40 and Compact consumer 60 / retired escrow 40.

## Pins and proof protocol

- SP1 SDK, guest and build crate: **6.10.0**; circuit and Solidity verifier: **v6.1.0**.
- Unmodified `sp1-contracts`: `d3629729c3216eb51bd4859d027a8eb729399fa4`.
- BEEFY verifier and test fixtures adapted from `../bridge-test` revision
  `e64f7f285545fe15fa94b80853ed374d9595d878` (Apache-2.0).
- Authority leaves are explicitly `Ethereum address || little-endian u64 stake`.
  This choice is compiled into the guest; the witness cannot select another profile.
- `BatchInput` contains a trusted checkpoint, 1–64 ordered justifications and one
  runtime MMR committee witness per justification,
  with a maximum encoded input of 4 MiB. Each justification retains the original
  SCALE wire format, slot bitmap, strict >2/3 quorum and signature checks.
- Public output retains the research's SCALE `Output`: 181 bytes without a next
  set or 225 with one. The previous-state hash covers the **initial** checkpoint;
  the new checkpoint and MMR root come from the final verified commitment.
- The Solidity decoder checks exact length, canonical Option tags and set bounds.
  The receiver checks genesis, previous checkpoint and increasing height, then
  calls the real verifier before publishing state. Gateway inclusion still uses
  `verifyMMRLeafProof(bytes32,bytes32[],uint256)` against the latest accepted root.

The local bootstrap deliberately supports one stake-one key with identical current
and next membership. It authenticates that key against **both** independently read
address-only MMR roots before computing the stake roots. It never trusts a signed
`cs` payload as its own initial anchor. Changed membership is covered by native
guest-verifier tests; it is not claimed as a live multi-validator test.

## Relayer state and recovery

The listener starts before ingress/Compact proving, so session announcements are
captured while that work runs. Captured source proofs and the observed source cursor
are separate from Ethereum's accepted destination checkpoint. Historical recovery
scans finalized blocks for BEEFY justifications and advances a durable scan cursor
only after a successful batch. An unavailable announcement fails proving; a current
RPC authority list cannot repair the trust chain.

For each requested message root, the prover selects authenticated commitments
reachable from the receiver's checkpoint, including any necessary authority
transitions. A job saves its input, proof, public output and new checkpoint. The
relayer persists a signed transaction and hash before broadcast, reconciles pending
jobs against Ethereum, and advances `destination.json` only after a successful,
canonical **finalized** receipt. Repeating a submitted job can rebroadcast identical
bytes. A revert or failed proof never advances destination state.

To reconcile or resume the latest saved proof job against a still-running local stack:

```bash
node tests/snowbridge/sp1-relay.mjs artifacts/snowbridge/ethereum-pos/run-REPLACE BLOCK
```

This is a single-writer local acceptance harness. Resuming the entire token-flow
orchestrator, replacement transactions, concurrent relayers, checkpoint migration,
production weights and an operator service are not implemented. A different writer
advancing the latest root may require a new MMR inclusion proof. Keep the PoC's
existing single-asset and one-message-per-block restrictions.

## Focused checks

```bash
cargo test --locked --manifest-path tests/snowbridge/sp1/Cargo.toml -p midnight-beefy
node --test tests/snowbridge/sp1-relay.test.mjs
artifacts/snowbridge/tools/foundry/forge test --root tests/snowbridge/contracts
```

`SP1ClientTest` uses an explicitly bounded test verifier for decoder/state tests.
It does not establish real proof acceptance. `test.sh` also runs `SP1ProofTest`
against the public captured Groth16 proof. To run that check alone, supply the
matched proof/checkpoint pair (without these variables, a direct Forge run skips
this test):

```bash
SNOWBRIDGE_SP1_PROOF="$PWD/tests/snowbridge/fixtures/sp1-committee-boundary-proof.json" \
SNOWBRIDGE_SP1_INITIAL="$PWD/tests/snowbridge/fixtures/sp1-committee-boundary-initial.scale" \
artifacts/snowbridge/tools/foundry/forge test --root tests/snowbridge/contracts \
  --match-contract SP1ProofTest
```

The live relayer additionally rejects altered proof bytes, altered public values,
wrong guest key, wrong prior checkpoint, wrong genesis and replay against the
**real deployed** verifier/receiver. The final report hashes all SP1 job artifacts,
checks their transaction receipts and MMR roots against the actual release proofs,
and includes the guest ELF hash and deployed receiver/verifier code hashes.

The historical fixture helper `sp1-capture-checkpoint.mjs` derives an initial state
only for the saved one-authority run after checking both recorded roots. It is
for offline fixture reproduction; live initialization uses the fresh node.
