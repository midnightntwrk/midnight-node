# Runbook: AURA → BABE Consensus Migration

Live migration of a running Midnight network from AURA to BABE block
production. GRANDPA finality and BEEFY are unaffected.

**Roles.** _Network operator_ — coordinates the phases, drives governance
motions and runtime upgrades. _Validator operators_ — upgrade binaries,
manage keys.

All code paths, storage items, log lines, and tooling referenced below live
in the [midnight-node repository](https://github.com/midnightntwrk/midnight-node).

## Migration at a glance

```
      binary rollout          governance                 governance                    automatic
 Aura ──────────────► Aura ──────────────► Aura ──────────────────► ScheduledFlip ──────────────► Babe
   (all nodes v3)        v3 runtime upgrade   consensusEngine.scheduleFlip()      (last slot of an epoch)
                        (pallet activates)
```

| Phase | Action                                                                                      | Gate to proceed                                              |
| ----- | ------------------------------------------------------------------------------------------- | ------------------------------------------------------------ |
| 0     | Prerequisites: babe keys generated and permissioned candidates list updated                 | Updated permissioned-candidates list active                  |
| 1     | Binary rollout: source-network checks, 100 % binary rollout, monitoring                     | Exit checklist all green                                     |
| 2     | Verify permissioned candidates with babe keys observed (1.1), then v3 runtime upgrade (1.2) | Upgrade block **finalized**; ≥ 1 committee rotation after it |
| 3     | Governance: `consensusEngine.scheduleFlip()` — **point of no return**                       | —                                                            |
| 4     | Flip commits automatically at an epoch boundary                                             | Post-flip verification                                       |

---

## Phase 0 — Prerequisites

Complete well ahead of any governance action. Nothing here changes which
engine produces blocks.

### 0.1 Generate, load, and submit BABE keys — every validator

This can and should be executed even before Midnight release that contains
migration code.

1. **Generate a fresh, unique sr25519 key pair.** Never reuse any existing
   key (see the invariant below).
2. **Load it into the keystore** (key type `babe`), one of:

- `BABE_SEED_FILE` (config key `babe_seed_file`) pointing at the secret —
  the node inserts it at startup and logs `BABE pubkey: <ss58>`;
- `midnight-node key insert --chain <chainspec> --keystore-path <path>
--scheme sr25519 --key-type babe --suri '<secret>'`.

Do **not** use the `author_insertKey` RPC — unsafe-classified; a
production validator must never expose unsafe RPC methods.

3. **Submit the BABE public key** to the network operator.
   When submitting BABE public key, attach also cross-chain public key,
   so the network operator will be able to attach the BABE key to
   a proper item in permissioned-candidates list.

**Verify (per validator):**

- [ ] `BABE pubkey: …` in the startup log, or a keystore file with the
      `62616265` prefix. Do **not** trust `author_hasKey` /
      `author_hasSessionKeys` — they answer through an AURA fallback.

### 0.2 Update permissioned-candidates list

The network operator prepares the updated `permissioned-candidates-list`
and drives governance motion submit it to Cardano. Two Cardano epoch
boundaries after update operation have to be observed before the list
becomes active.

### 0.3 Gate: permissioned-candidates with BABE keys observed by the Midnight nodes

Query `systemParameters_getAriadneParameters` with the current Cardano
epoch as parameter on a RPC Midnight node and confirm each candidate entry
carries a `babe` key and that no two candidates share the same key.

## Phase 1 — Binary rollout

### 1.1 Verify the network you are upgrading from

- [ ] `sessionCommitteeManagement` on-chain storage version is **1** (the
      committee v1→v2 migration is combined in with the migration on this branch -
      it must never ship before).
- [ ] `state_getRuntimeVersion` reports `spec_version` at least **2.1.0**
      (`2_001_000`)
- [ ] The running runtime has **no** `consensusEngine` pallet (metadata lists
      no such pallet; `consensusEngine.engineState()` does not exist). The v3
      upgrade is the pallet's first deployment; a runtime that already carried
      a version of it would reject the dual-digest blocks the upgraded binary
      authors. **Stop and reassess if the pallet is present.**

### 1.2 Roll out the migration-aware binary — every node

1. Upgrade **all** nodes: validators, full, RPC, archive, boot nodes.
2. Confirm each node starts and syncs.

**Verify:**

- [ ] 100 % of nodes on the migration-aware version. This is a hard gate for
      Phase 1: from the runtime-upgrade a non-upgraded full node cannot
      participate in the network.
- [ ] Blocks authored by upgraded validators carry two pre-runtime digests
      (AURA, then BABE `SecondaryPlain`) — already on the 2.1.0 runtime.
      Cadence unchanged.

### 1.3 Monitor readiness (Grafana)

Two things must be observable across the network throughout the migration:

- [ ] **BABE key readiness** — how many nodes report `1` on
      `midnight_babe_key_registered` (`1` = the keystore holds a BABE key
      registered for a permissioned candidate on Cardano). Every validator
      must report `1` before the Phase 2.1 gate.
- [ ] **Node versions** — the versions of the nodes in the network. 100 %
      must be on the migration-aware version (v3) before Phase 1 (gate 1.4).
- [ ] Additionally, watch the per-session committee-membership log (target
      `committee-membership`, `IS` / `IS NOT in the committee`) for
      unexpected dropouts after key rotations.

### Phase 1 gate:

- [ ] 1.3 source-network checks green.
- [ ] 100 % of all nodes on the migration-aware binary; every validator's
      blocks carry the dual pre-runtime digests.
- [ ] Every validator: a BABE key generated and loaded.
- [ ] Candidate list audited: no session key shared between any two
      candidates, past candidates included.
- [ ] Governance bodies briefed; motion signers available; full sequence
      rehearsed on a lower environment (see
      [Rehearsal](#rehearsal-in-lower-environments)).

---

## Phase 2 — Runtime upgrade to v3 (consensus-engine pallet, babe-pallet, session-keys)

### 2.1 The runtime upgrade

**Pre-flight (mandatory).** Run the upgrade under `try-runtime` against a
snapshot of the target network. Assert
`sessionCommitteeManagement.currentCommittee` and `.queuedCommittee` survive
non-empty and in the new key shape, and that `babe.genesisSlot` holds the
activation sentinel (`18446744073709551615`).

**Action.** Deploy the v3 runtime (adds babe pallet, consensus-engine pallet,
the `babe` session key; runs migrations) through the standard governance
runtime-upgrade flow.

The first block executed by the new runtime is the **activation block**.
From it on, runtime rejects any block from a non-upgraded author.

**Verify:**

- [ ] Runtime log on the upgrade block: `translating committee & session
 keys and initializing QueuedCommittee` and `Consensus-engine pallet
 activated: pre-seeded pallet-babe GenesisSlot…` (target
      `consensus-engine`).
- [ ] `consensusEngine.engineState()` is present and returns `Aura`;
      `babe.genesisSlot()` is `18446744073709551615`; cadence unchanged.
- [ ] Every validator has authored at least one **finalized** block since
      the upgrade; no spike in rejected blocks.
- [ ] After the next committee rotation (session boundary, 300 block in Mainet):
      `babe.authorities()` is non-empty and matches the registered keys.
      BABE `NextEpochData` consensus digests now appear at session boundaries.

### Phase 2 gate:

- [ ] The activation block (the block that executed the runtime upgrade)
      is **finalized** by GRANDPA. The runtime cannot check this itself.
      Network operator has to verify it.
- [ ] `babe.authorities()` is non-empty (≥ 1 committee rotation since the
      upgrade).

---

## Phase 3 — Schedule the flip (`Aura` → `ScheduledFlip`)

**This is the point of no return** — the action that triggers the flip
itself. No further governance call follows: block production continues on
AURA until the runtime commits the flip at the next last-slot-of-an-epoch
it reaches with BABE authorities populated.

**Action.** Drive a federated-authority motion for
`consensusEngine.scheduleFlip()`:

1. Council: propose, vote (2/3), close.
2. Technical Committee: same, for the identical call.
3. Anyone: `federatedAuthority.motionClose(motion_hash,
  proposal_weight_bound)` (a too-low bound fails with
   `MotionWeightBoundTooLow`).

**Verify:**

- [ ] `MotionDispatched.motion_result` is success **and**
      `consensusEngine.engineState()` returns `ScheduledFlip` (the pallet
      emits no events — always re-read the state).
- [ ] `babe.authorities()` is non-empty (otherwise the flip is postponed
      epoch after epoch — see troubleshooting).

---

## Phase 4 — The flip (automatic, `ScheduledFlip` → `Babe`)

No action — watch. At the last slot of the epoch the runtime commits the
flip; if no block lands in that exact slot, it fires at a later epoch's
last slot. The flip block is the last AURA block; its child is the first
BABE block. No restarts anywhere.

**Watch for**, in order:

```
Consensus engine flip at the last slot (<slot>) of the epoch; BABE genesis slot <slot+1>, entering Babe state.
                                                             (runtime, target consensus-engine)
consensus flip to BABE observed at block #N (<hash>)          (target babe-authoring)
seeded BABE epoch tree and zero chain-weight at flip block #N (<hash>): epochs <X> and <Y>
                                                             (target babe-authoring)
handing block authoring over from AURA to BABE at <hash>      (validators only)
```

**Verify:**

- [ ] `consensusEngine.engineState()` returns `Babe`.
- [ ] New blocks do not carry a AURA pre-digest.
- [ ] Cadence holds ~6 s; GRANDPA finality advances across the boundary.
- [ ] All validators author over the next epoch.
- [ ] Sidechain and BABE epoch boundaries coincide.
- [ ] Downstream consumers (indexer, RPC clients, explorers) ingest
      post-flip blocks.

**Post-flip notes:** a node restarted after the flip starts directly in
BABE (`chain already on BABE at startup`). Nodes interpret BABE pre-runtime
digests only in blocks whose `pallet-version` digest does not report a
runtime older than `3.0.0` (the one that introduced `consensusEngine`); any
block executed by an older runtime is treated as a plain AURA block whatever
its header carries, so pre-upgrade history syncs the same way it always did,
including over warp sync. BABE RPC is not wired (no
`babe_epochAuthorship`) — query storage instead; no BABE import-queue
metrics; BABE equivocation reporting is disabled.

---

## Troubleshooting

| Symptom                                                                                                                                                    | Cause                                                                                                                                                     | Action                                                                                                                                                                                                                                                                                                                     |
| ---------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Motion closes but `MotionDispatched.motion_result` carries `InvalidEngineState`; `engineState` unchanged                                                   | Call dispatched from the wrong state (e.g. `scheduleFlip` twice)                                                                                          | Check `engineState` before opening motions; the rehearsal tooling refuses wrong-state motions for this reason                                                                                                                                                                                                              |
| After the runtime upgrade: a validator's blocks are rejected (`BABE pre-runtime digest required in state 'Aura'`)                                          | That validator runs a pre-migration binary (or otherwise doesn't emit the digest)                                                                         | Upgrade the binary. The chain loses that validator's slots until fixed; other validators are unaffected                                                                                                                                                                                                                    |
| `failed to seed BABE epoch tree at …; BABE import/authoring may stall` (ERROR, target `babe-authoring`)                                                    | Epoch-tree bootstrap at the flip failed on that node                                                                                                      | Investigate that node; restarting it after the flip re-attempts the bootstrap path                                                                                                                                                                                                                                         |
| A validator stops authoring after the flip; earlier `WARN … none match the requested public … falling back to AURA` from `aura-to-babe-migration-keystore` | On-chain BABE key matches neither the keystore's BABE key nor its AURA key (e.g. registered one key, loaded a different one)                              | Load the registered key into the keystore, or fix the registration and wait for a rotation. `midnight_babe_key_registered` catches this **before** the flip — keep it green                                                                                                                                                |
| `midnight_babe_key_registered == 0` on a validator at the Phase 1 gate                                                                                     | Keystore key missing or not registered on Cardano, registration not yet observed, or key mismatch                                                         | Complete 0.1 for that validator, or wait out the observation lag; re-check before 2.1                                                                                                                                                                                                                                      |
| A committee member never appears in the applied validator set; node logs `Could not set_keys for … error: … DuplicatedKey` at session rotation             | The permissioned-candidates list published on Cardano (a governance action) contains a session key already owned by another — possibly former — candidate | **Treat as an incident — the chain is in undefined behavior and there is no in-band fix**. Prevention — the uniqueness audit of the governance-published candidate list at the Phase 1.1 gate and before every list change — is the only protection. Escalate immediately; the failure is log-only, with no on-chain event |
| `author_hasKey`/`author_hasSessionKeys` report a BABE key that isn't there                                                                                 | These RPCs answer through the migration keystore's AURA fallback                                                                                          | Use the metric, the `BABE pubkey:` startup log, or the `62616265…` keystore file instead                                                                                                                                                                                                                                   |
| Node panics importing a block with an unexpected digest combination (e.g. `AURA pre-runtime digest present in state 'Babe'`)                               | An author emitted digests inconsistent with the on-chain state — misconfigured or malicious author                                                        | The block is invalid and rejected network-wide; identify the author and follow up                                                                                                                                                                                                                                          |
| Need to abort the migration                                                                                                                                | —                                                                                                                                                         | Before `scheduleFlip` nothing is committed: the network runs AURA with dual digests indefinitely. The consequential action is `scheduleFlip`; once in `ScheduledFlip` the flip commits automatically at an epoch boundary. Reverting any state requires an emergency runtime upgrade via governance                        |

---

## Rehearsal in lower environments

`local-environment/` in the midnight-node repository drives the full flow
against a local or forked network:

```bash
# Aura -> ScheduledFlip (only after the runtime-upgrade block is finalized)
npm run consensus-upgrade-schedule-flip:local-env -- \
 --technical-uris //One //Two //Three \
 --council-uris //Four //Five //Six \
 --executor-uri //One
```

The command refuses wrong-state motions and verifies the transition
afterwards. Combine with `image-upgrade` / `governance-runtime-upgrade` /
`full-upgrade` to rehearse the complete sequence (binary rollout → runtime
upgrade → schedule → flip) against forked network state. The
local environment's validators are deliberately configured as a key-setup
test matrix (fully configured, missing keystore key, unregistered key, …) —
use it to observe every failure mode above before the real run. See
[local-environment/README.md](https://github.com/midnightntwrk/midnight-node/blob/main/local-environment/README.md)
and [fork-testing.md](https://github.com/midnightntwrk/midnight-node/blob/main/docs/fork-testing.md).

---

## Quick reference

| What                       | Where                                                                                                                                                                                                                                                                       |
| -------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Rollout staging            | binary rollout (dual digests appear) → **all registrations observed** (1.1 green) → v3 runtime upgrade → **activation block finalized** → ≥ 1 rotation → schedule → flip                                                                                                    |
| Engine state               | `consensusEngine.engineState()` → `Aura` / `ScheduledFlip` / `Babe` (storage query; no runtime API or RPC exposes all three states)                                                                                                                                         |
| Governance call            | `consensusEngine.scheduleFlip()` (root, via `federatedAuthority.motionApprove` × 2 bodies + `motionClose`)                                                                                                                                                                  |
| BABE authorities           | `babe.authorities()`                                                                                                                                                                                                                                                        |
| Activation sentinel        | `babe.genesisSlot()` == `u64::MAX` between the runtime upgrade and the flip                                                                                                                                                                                                 |
| Validator readiness metric | `midnight_babe_key_registered` (validators with Prometheus; 1 = keystore key registered on Cardano)                                                                                                                                                                         |
| BABE key type              | `babe` (keystore file prefix `62616265`), sr25519; loaded via `BABE_SEED_FILE` or `midnight-node key insert` (never `author_insertKey`)                                                                                                                                     |
| Cardano registration       | permissioned-candidates list `babe_pub_key` (all candidates are permissioned)                                                                                                                                                                                               |
| Flip timing                | Last slot of an epoch while in `ScheduledFlip` and `babe.authorities()` non-empty; BABE genesis = first slot of next epoch                                                                                                                                                  |
| Log targets                | `consensus-engine` (activation, flip, postponement), `babe-authoring` (flip, epoch-tree seed, handover), `babe-predigest` (digest attachment), `aura-to-babe-migration-keystore` (key fallbacks), `babe-key-readiness` (probe), `committee-membership` (session membership) |
