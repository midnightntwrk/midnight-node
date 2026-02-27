#performance #sync
# Adaptive sync verifier to accelerate block sync

Replace the partner-chains `AuraVerifier` import queue with an `AdaptiveVerifier` that, when enabled via `BUCKEL_UP=true`, skips the expensive Postgres-backed `check_inherents` while the node is catching up. Full inherent verification resumes automatically once the node is synced. Disabled by default -- operators must opt in. Additional optimizations: parallelized inherent data queries via `futures::join!` and increased DB pool sizes (5 to 20).

## How "catching up" is determined

The chain produces blocks in fixed 6-second time slots. Every slot corresponds to a specific moment in real time, so your machine's system clock (`SystemTime::now()`) can calculate what slot number the chain *should* be on right now:

```
current_slot = milliseconds_since_unix_epoch / 6000
```

The verifier compares the **best block's slot** (the most recent block your node has imported) against the **current slot** (derived from the system clock). If your best block is more than 1000 slots (~100 minutes) behind, the node is considered "catching up" and inherent checks are skipped for speed.

Once your best block is within 1000 slots of the current time, the node is considered synced and **every** imported block gets full verification -- including old blocks from forks or reorgs.

This is a node-level check ("is my node behind?"), not a per-block check. This means an attacker cannot bypass verification by sending blocks with old slot numbers once the node is caught up.

## What `check_inherents` validates (the expensive part we skip)

The `check_inherents` runtime API creates inherent data from our **local Postgres/db-sync** and compares it against the inherent extrinsics already in the block. It is a "second opinion" check -- asking "does my local view of Cardano agree with what the block author claimed?" Specifically it validates 6 inherent data providers:

| Inherent | What it checks | Failure mode |
|---|---|---|
| **McHash** (mainchain reference) | The Cardano block hash referenced by the sidechain block matches what our db-sync says is the stable block for that slot | `McStateReferenceInvalid`, `McStateReferenceRegressed` |
| **Ariadne** (authority selection) | The validator committee data matches our local computation from Cardano SPO registrations | `InvalidValidatorsHashMismatch` (fatal) |
| **CNight observation** (tDUST tokens) | Token UTXOs and next Cardano position match our db-sync data | `InherentError::Other` (data mismatch) |
| **Federated authority** (council/TC) | Council and technical committee members match our observation of Cardano | `CouncilMembersMismatch`, `TechnicalCommitteeMembersMismatch` |
| **Token bridge** | Bridge transfer data matches our db-sync | `IncorrectInherent` (fatal) |
| **Timestamp** | Block timestamp within acceptable range | Standard substrate check |

Every one of these queries Postgres. That is the ~50-70ms per block cost.

## What still runs even when we skip `check_inherents`

**`execute_block` always runs during import.** The Substrate client calls `Core::execute_block()` for every block it imports (the verifier runs *before* import; it does not control whether the runtime executes the block). This means:

1. **All inherent extrinsics are fully executed** -- The runtime applies every extrinsic in the block, including inherents. Each pallet's dispatch logic runs. If an inherent extrinsic is malformed or internally inconsistent, execution fails and the block is rejected.

2. **State root verification** -- After executing all extrinsics, the runtime computes the resulting state root and compares it to the block header's `state_root`. Any discrepancy (from tampered extrinsics, missing inherents, wrong data) causes the block to be rejected.

3. **Mandatory inherent presence** -- Inherent extrinsics have `DispatchClass::Mandatory`. The runtime rejects blocks missing required inherents.

4. **Aura seal verification** -- The runtime's `execute_block` strips the seal and verifies the pre-digest matches an authorized author for that slot.

## Why it is safe for blocks far from the tip

The critical distinction:

- **`execute_block`** asks: *"Is this block internally valid? Do the extrinsics produce the claimed state?"*
- **`check_inherents`** asks: *"Does my local Postgres agree with what the block author put in these inherents?"*

For finalized blocks (blocks far from the tip), `check_inherents` is redundant because:

1. **GRANDPA finality** -- These blocks have been finalized by a 2/3+ supermajority of validators. Each of those validators ran both `check_inherents` AND `execute_block` against their own Postgres instances before voting to finalize. The network has already settled the question of whether this data is correct.

2. **`execute_block` catches structural invalidity** -- A block with missing inherents, wrong extrinsic format, or invalid state transitions will fail execution regardless. What we cannot catch without `check_inherents` is a block where the *data values* (e.g., which Cardano block hash is referenced) differ from our local db-sync -- but finality already guarantees those values are what 2/3+ of the network agreed on.

3. **Our local Postgres could be the one that is wrong** -- During initial sync, our db-sync might be slightly behind Cardano, have stale cache entries, or have different timing than what validators saw when they originally produced these blocks. Running `check_inherents` against lagging local data could cause *spurious* rejections of perfectly valid finalized blocks.

## What could theoretically go wrong

- **Long-range fork attack**: A malicious peer sends us blocks on a fake fork that was never finalized. We would import them without inherent checks only while the node is catching up. Once the best block is near wall clock, all blocks (including old forks/reorgs in the unfinalized region) get full verification. Additionally:
  - GRANDPA will not finalize them (no justifications from 2/3+ validators)
  - `execute_block` still validates internal consistency
  - `ForkChoiceStrategy::LongestChain` means we will switch to the real chain when we see it

- **Corrupted db-sync**: If our Postgres has wrong data, we will only discover it when we get close to the tip and `check_inherents` starts running. This is acceptable since the blocks we synced are already finalized by the network.

- **Disabled by default**: The adaptive sync is behind the `BUCKEL_UP=true` environment variable. Without it, every block gets full inherent verification.

## Protection summary

| Protection | Always runs | Skipped during fast sync |
|---|---|---|
| Aura seal extraction | Yes | -- |
| `execute_block` (full runtime execution) | Yes | -- |
| State root verification | Yes | -- |
| Mandatory inherent presence | Yes | -- |
| GRANDPA finalization | Yes | -- |
| `check_inherents` (Postgres comparison) | -- | Yes, when `BUCKEL_UP=true` (resumes once best block is within 1000 slots of wall clock) |

## All optimizations

| Optimization | File | Description |
|---|---|---|
| **AdaptiveVerifier** | `node/src/service.rs` | Behind `BUCKEL_UP=true`. Checks if the node is syncing (best block vs wall clock). Skips `check_inherents` while catching up, full verification once synced |
| **Parallelized inherent data queries** | `node/src/inherent_data.rs` | 4-5 independent Postgres queries in `ProposalCIDP` and `VerifierCIDP` run concurrently via `futures::join!` instead of sequentially |
| **Increased Postgres pool sizes** | `node/src/main_chain_follower.rs` | All connection pools increased from 5 to 20 to support parallel queries without contention |

PR: https://github.com/midnightntwrk/midnight-node/pull/696
