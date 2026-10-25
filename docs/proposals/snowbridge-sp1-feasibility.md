# SP1 verification of Midnight BEEFY messages: integration feasibility

## Finding and evidence

This is feasible without changing the BEEFY signed-message format, quorum rule,
Compact token flow, SDK outbound messages or Ethereum custody/retry protocol.
Replace the Ethereum BEEFY signature-verification layer with an SP1-backed
Midnight light client. Keep the SP1 cryptographic verifier unchanged, and expose
the existing MMR-verification interface to the Gateway.

It is not a drop-in deployment of Avail's Vector contract or guest. Avail's
inspected guest verifies GRANDPA/Ed25519 header justifications. Midnight signs
BEEFY/secp256k1 commitments with its own authority/stake payloads. The examples
in `../bridge-test` already implement the latter, including strict quorum and
checkpoint transitions, but have no EVM receiver or EVM-verifiable proof mode.

The investigation read `bridge-test` revision
`e64f7f285545fe15fa94b80853ed374d9595d878` without modifying that repository.
The native experiment used saved consensus-generated BEEFY proofs from the
successful local run `ethereum-pos/run-fllxkhdn`, not synthetic signatures:

| Check | Result |
| --- | --- |
| Original example verifier against PoC payloads | All 21 rejected: `current authority commitment mismatch`. |
| Same verifier with only the authority-leaf profile adapted | All 21 accepted, blocks 261–417, set IDs 1 and 2. |
| Tampered signature, MMR root, trusted authority roots and stale replay | All 84 negative cases rejected. |
| Both roots actually relayed for failed/successful Ethereum releases | Matched the verified outputs. |
| SP1 proof generation for those live inputs | Not run. |
| SP1 proof verification on Ethereum | Not implemented/tested. |

This is a one-authority development chain, including a set-ID transition with
unchanged membership. It does not validate changed membership or production
validator counts. Initial keys were checked against both recorded live current
and next BEEFY authority roots; the stake value was checked as 1, matching this
runtime. We did not accept the signed `cs` field as its own trust anchor.

Reproduce the experiment from the node repository with the sibling checkout and
original saved run present:

```bash
python3 tests/snowbridge/sp1-compat.py artifacts/snowbridge/ethereum-pos/run-fllxkhdn
```

The script checks the original run's evidence hashes, pins the sibling revision
and verifies its native verifier source is unmodified. It stages a separate copy
under `artifacts/snowbridge/sp1-feasibility`, compiles it alongside the original,
and records source/lock hashes in
[`native-compatibility.json`](../../artifacts/snowbridge/sp1-feasibility/native-compatibility.json).
This command runs native verification only. The copied crate is an explicit
compatibility experiment, not a deployed SP1 guest or a replacement production verifier.

## The narrow authority-format mismatch

There are three different roots here; they must not be conflated:

| Purpose | Leaf bytes in this PoC |
| --- | --- |
| Standard BEEFY MMR authority root used by existing Solidity client | Ethereum address (20 bytes). |
| Midnight signed `cb` / `nb` authority-and-stake roots | Ethereum address (20 bytes) + little-endian u64 stake. |
| `bridge-test` expectation for ordinary Midnight `cb` / `nb` | Compressed SEC1 key (33 bytes) + little-endian u64 stake. |

The opt-in PoC sets `BeefyAuthorityToMerkleLeaf` to `BeefyEcdsaToEthereum`.
`runtime/src/beefy.rs::compute_authority_set` uses that converter too, then appends
the stake. The example instead assumes the normal `RawBeefyId` converter.

The experiment changed only the example's authority leaf construction to match
the bytes this PoC already signs. It retained all payload bytes, ECDSA verification,
slot-based strict >2/3 quorum, bitmap checks and authority-transition checks.
Select this profile explicitly in a separately pinned guest/program key. Do not
add a witness-controlled fallback that accepts either root convention. For the
ordinary runtime, retain the original raw-key profile. Changing the PoC runtime
back would unnecessarily disrupt the existing direct-Solidity bridge baseline.

## Proposed verification boundary

```mermaid
flowchart LR
    A[Midnight BEEFY justification] --> B[Relayer: collect witness and trusted destination checkpoint]
    B --> C[SP1 guest: verify quorum and checkpoint transition]
    C --> D[Groth16 or PLONK proof + public values]
    D --> E[Midnight receiver calls unchanged ISP1Verifier.verifyProof]
    E --> F[Check prior state and chain; store authenticated MMR root]
    F --> G[Gateway verifies MMR/message inclusion, origin and nonce]
    G --> H[Execute token release]
```

The receiver accepts `(proofBytes, publicValues)` and pins both the verifier
implementation/route and the Midnight bridge program verification key. A relayer
must not supply an arbitrary program key, verifier address, initial authority set
or trusted prior state. The receiver must:

1. Verify the proof with `verifyProof(programVKey, publicValues, proofBytes)`.
2. Decode canonical public values for that exact program version.
3. Require the proven previous checkpoint hash to equal the receiver's stored
   checkpoint hash, the genesis identity to match, and block height to advance.
4. Derive/store the new checkpoint hash and the proven MMR root atomically, then
   emit an accepted-root event. No external application callback occurs first.
5. Offer `verifyMMRLeafProof(bytes32,bytes32[],uint256)` and optionally the existing
   `latestMMRRoot`/`latestBeefyBlock` getters.

Steps 1–3 may perform read-only prechecks in another order, but no state update or
consumer-visible acceptance can precede all of them. Other contracts consume only
accepted state and still prove inclusion of the particular message. A valid SP1
proof of an MMR root does not authenticate arbitrary calldata beside it.

`MidnightGateway._verifyCommitment` already calls the three-argument
`verifyMMRLeafProof` interface on `BEEFY_CLIENT`. A new contract implementing the
same ABI can serve that call without inheriting the old `BeefyClient` class.
Keep the existing MMR library, 113-byte leaf, message decoding, origin, replay and
custody checks. The current latest-root-only design still requires regenerating
inclusion proofs if another root update wins the transaction race.

Use a fresh deployment for the first experiment: the Gateway's client address is
immutable, and its address participates in the bridge domain. This is not a plan
to migrate the existing PoC's custody or change its trusted root in place.

## What can remain unchanged

Reuse the actual Succinct verifier implementation, not a mock and not a rewritten
SNARK verifier. Avail's contract uses the same generic
[`ISP1Verifier`](https://github.com/succinctlabs/sp1-contracts/blob/d3629729c3216eb51bd4859d027a8eb729399fa4/contracts/src/ISP1Verifier.sol)
boundary. The inspected [Vector contract](https://github.com/availproject/sp1-vector/blob/c649310e79de376926863504d49eddbf09532980/contracts/src/SP1Vector.sol)
checks trusted state and invokes `verifyProof` before storing accepted commitments.

For the smallest guest change, **retain the example's existing SCALE public
output**. `ISP1Verifier` accepts opaque public-value bytes; it does not require
Avail's ABI layout. Implement a bounded, canonical Solidity decoder for the
Midnight output and test it against Rust encoding. The current structure is
225 bytes with a next set and 181 without one. Decode little-endian integers,
require canonical Option encoding and exact length, and hash the exact checkpoint
bytes with the existing `midnight-vector-state-v1` domain. Both forms need tests.

A versioned ABI output is a reasonable later alternative, but changes the guest
and its program key. Do not simply run `abi.decode` on the existing SCALE bytes.
Do not deploy `SP1Vector.sol` unchanged: its header ranges, output variants and
GRANDPA authority semantics are different. Its guardian operations also include
verifier/key/genesis/commitment overrides; adopting those powers is a separate
trust-policy decision, not a prerequisite to using SP1 verification.

The existing Compact proving is separate from the proposed SP1 finality proof.
Neither Compact contracts nor their application proofs need to know about SP1.
The Ethereum-to-Midnight finality/receipt verifier and return-receipt retry state
machine can remain unchanged.

## Remaining implementation

| Component | Required work |
| --- | --- |
| Midnight guest | Pin the explicit PoC authority-root profile; rebuild and record its ELF/vkey. Preserve all existing signature and transition checks. |
| Prover | Add `.groth16()` or `.plonk()` and export proof bytes/public values/program key. The current runner offers only execute/core/compressed. |
| Ethereum receiver | Deploy the real compatible SP1 verifier; implement canonical output decoding, trusted-state transition checks and the existing MMR ABI. |
| Relayer | Connect listener → proving → EVM submission → finalized receipt; persist both observed-source and accepted-destination checkpoints. |
| Local test | Deploy the new receiver/Gateway on a fresh stack and replace only the direct `submitFiatShamir` relay stage. |

The listener currently advances a native checkpoint when it accepts a source
message. That is not the same as the destination accepting an SP1 proof. Proofs
must extend the destination's accepted checkpoint and be submitted in order.
Persist pending jobs and signed next-set announcements, recover historical
justifications after reconnect, and never repair a missing announcement by
trusting the latest RPC authority list. A failed Ethereum submission does not
authorize advancing the destination cursor.

The supplied installers/builds target Linux x86_64. This investigation ran native
compatibility on the current Mac; it did not install the SP1 compiler, use a remote
prover, or validate local Mac SNARK proving. A local Linux prover/container or
explicitly configured remote prover needs to be exercised next. The proof-system
version, circuit artifacts, verifier selector and program key must be tested as
a matched set; do not infer compatibility from SDK version strings alone.

The examples record CPU core proofs for synthetic 1/4/16-signature workloads.
Those results do not measure recursive EVM proof generation or Ethereum gas.
Succinct distinguishes [core/compressed proofs from Groth16/PLONK proofs](https://docs.succinct.xyz/docs/sp1/generating-proofs/proof-types).
For our single-authority fixture, succinct verification may cost more gas than
one direct ECDSA check. Measure realistic committee sizes, batching, prover cost
and update latency before claiming a production economic benefit.

## Acceptance gates for the integration

First obtain a real EVM-verifiable SP1 proof of one captured live commitment and
submit it to the real verifier plus receiver on local Ethereum. Compare the
accepted block, checkpoint and MMR root with the native verifier; reject altered
proof bytes/public values, a different guest vkey, wrong checkpoint/genesis,
stale updates and malformed output encodings. A mock verifier is useful for
receiver unit tests but cannot satisfy this gate.

Then exercise current→next committee transitions (including changed membership),
omitted/contradictory announcements, duplicate authority slots, invalid extra
signatures and strict quorum boundaries. Preserve the source chain's authenticated
slot semantics; replacing them with unique-key counts or arbitrary stake weighting
would change the protocol.

Finally repeat the existing live token test with this receiver: Ethereum deposit
100 → native issuance → Compact consumer → retire 40 → real SP1 root update →
MMR/message proof → deliberately failed release → authenticated failure → retry
→ release 40 → authenticated completion. The final evidence must include the
guest/vkey/verifier hashes, actual proof bytes, EVM receipts/gas, accepted-state
history and the same custody/consumer/recipient balances as the existing run.

## Security scope

Keeping Succinct's verifier unchanged preserves reuse of its proof-system
implementation. It does **not** automatically inherit Avail's complete bridge
security: the Midnight guest, checkpoint bootstrap, decoder, authority handling,
application inclusion proofs and upgrade policy remain our integration's
responsibility. A SNARK proves the configured quorum checks executed correctly;
it cannot make a dishonest source quorum honest or replace the chain's consensus
trust assumption. Genesis bytes in a checkpoint also do not retroactively add
domain separation to BEEFY's signed message. The examples' caveat about authority
key reuse across independent chains still applies.

The clean integration therefore keeps consensus and token semantics intact and
changes where their verification work occurs. It is a small, separable receiver
and prover integration, but it still needs the actual SNARK-to-EVM acceptance run
before being described as end-to-end working.
