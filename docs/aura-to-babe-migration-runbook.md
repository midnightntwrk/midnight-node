# Runbook: AURA → BABE Consensus Migration

Live migration of a running Midnight network from AURA to BABE block
production. No chain restart, no node restarts. GRANDPA finality and BEEFY
are unaffected.

**Roles.** *Network operator* — coordinates the phases, drives governance
motions and runtime upgrades. *Validator operators* — upgrade binaries,
manage keys. *Candidate-list maintainer* — publishes the
permissioned-candidates list on Cardano (governance action; all candidates
are permissioned).

All code paths, storage items, log lines, and tooling referenced below live
in the [midnight-node repository](https://github.com/midnightntwrk/midnight-node)
(migration work on branch `feat-aura-to-babe-migration`).

## Migration at a glance

```
         governance                    governance                governance                    automatic
 Aura ──────────────────► v3 Runtime ──────────────► ArmedBabe ──────────────► ScheduledFlip ──────────────► Babe
      v3 runtime upgrade                arm babe                schedule_flip              (last slot of an epoch)
                        
```

| Phase | Action | Gate to proceed |
|---|---|---|
| 0 | Prerequisites: source-network checks, 100 % binary rollout, keys, monitoring | Exit checklist all green |
| 1 | Verify key registrations observed (1.1), then session-keys runtime upgrade (2.2) | 2.1 checks green **before** the upgrade; ≥ 1 committee rotation after it |
| 2 | Governance: `consensusEngine.armBabe()` | First **finalized** `ArmedBabe` block |
| 3 | Governance: `consensusEngine.scheduleFlip()` — **point of no return** | — |
| 4 | Flip commits automatically at an epoch boundary | Post-flip verification |

There is no rollback extrinsic. `arm_babe` is a marker of intent — no flip
is committed while armed. `scheduleFlip` is the consequential action: after
it, the flip commits automatically. Reverting any state requires an
emergency runtime upgrade via governance.

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
3. **Submit the public key** to the candidate-list maintainer. The
  maintainer re-publishes the permissioned-candidates list on Cardano with
  each candidate's `babe_pub_key` (any time before the Phase 2.1 gate;
  arming is not a prerequisite for registration).

**Verify (per validator):**

- [ ] `BABE pubkey: …` in the startup log, or a keystore file with the
     `62616265` prefix. Do **not** trust `author_hasKey` /
     `author_hasSessionKeys` — they answer through an AURA fallback.

> **HARD INVARIANT — session keys must be unique across all candidates.
> Delivering duplicated keys breaks the chain.** No session key (`babe`,
> `aura`, or `grandpa`) may ever appear on more than one candidate in the
> published list. A duplicate delivered on chain is rejected log-only
> (`Could not set_keys for <account>, error: … DuplicatedKey`), no event
> fires, and the chain is left in **undefined behavior with no fix**.
> Prevention is the only protection: audit the candidate list for
> duplicates before every change and at the Phase 2.1 gate. Key ownership
> is never purged, so a key once used by a *past* candidate conflicts
> forever.

### 0.2 Verify the network you are upgrading from

- [ ] `sessionCommitteeManagement` on-chain storage version is **1** (the
     committee v1→v2 migration is combined in with the migration on this branch -
     it must never ship before).
- [ ] `state_getRuntimeVersion` reports `system_version` < **3**.

### 0.3 Roll out the migration-aware binary — every node

1. Upgrade **all** nodes: validators, full, RPC, archive, boot nodes.
2. Confirm each node starts and syncs.

**Verify:**

- [ ] 100 % of nodes on the migration-aware version. This is a hard gate for
     Phase 1: once armed, blocks from a non-upgraded author are rejected
     network-wide, and a non-upgraded full node cannot import post-flip
     blocks.

### 0.4 Monitor readiness (Grafana)

Two things must be observable across the network throughout the migration:

- [ ] **BABE key readiness** — how many nodes report `1` on
     `midnight_babe_key_registered` (`1` = the keystore holds a BABE key
     registered for a permissioned candidate on Cardano). Every validator
     must report `1` before the Phase 2.1 gate.
- [ ] **Node versions** — the versions of the nodes in the network. 100 %
     must be on the migration-aware version before Phase 1 (gate 0.2).
- [ ] Additionally, watch the per-session committee-membership log (target
     `committee-membership`, `IS` / `IS NOT in the committee`) for
     unexpected dropouts after key rotations.

### Phase 0 exit checklist

- [ ] 0.2 source-network checks green.
- [ ] 100 % of all nodes on the migration-aware binary.
- [ ] Every validator: fresh BABE key generated and loaded; public key
     submitted to Cardano and past two epoch boundaries.
- [ ] Candidate list audited: no session key shared between any two
     candidates, past candidates included.
- [ ] Session-keys runtime (Phase 1.2) built, `try-runtime`-verified against
     a network snapshot, staged for the governance upgrade flow.
- [ ] Governance bodies briefed; motion signers available; full sequence
     rehearsed on a lower environment (see
     [Rehearsal](#rehearsal-in-lower-environments)).

---

## Phase 1 — Runtime upgrade to v3 (consensus-engine pallet, babe-pallet, session-keys)

### 1.1 Gate: every registration observed by the Midnight nodes

Cardano data reaches the midnight nodes with an observation lag — allow time
after the last candidate-list change. Check **all three**; do not proceed
while any fails:

1. **Every permissioned candidate has a `babe` key registered.**
  Query `systemParameters_getAriadneParameters` on a Midnight node and
  confirm each candidate entry carries a `babe` key. A malformed key shows
  up as `Permissioned candidate 0x… has an invalid 'babe' key of N bytes`
  (target `babe-key-readiness`).
  *On failure:* the maintainer re-publishes the corrected list; wait for
  observation.
2. **No key bytes are claimed by two candidates — hard invariant.**
  `pallet_session` maps each (key type, key bytes) pair to exactly one
  validator account, so if two candidates publish the same key bytes for
  the same key type, the second registration is rejected with
  `DuplicatedKey`. Because the maintainer publishes the list as a whole,
  this is a document review rather than a chain query: check the list
  about to be published against itself — no key bytes twice within it —
  and against every list published before it. Keys from candidates that
  have since left still count — ownership entries are never purged.
  *On failure:* **stop** — replace the conflicting key via a corrected
  list before anything else. Delivering it on chain has no fix (0.3).
3. **`midnight_babe_key_registered == 1` on every validator** — proves each
  keystore holds exactly the key registered for it, as observed by that
  validator's own node.
  *On failure:* fix that validator's keystore or public key in permissioned
  candidtes list(0.3), or wait out the Cardano observation lag (two epoch
  boundaries).

### 1.2 The runtime upgrade

**Pre-flight (mandatory).** Run the upgrade under `try-runtime` against a
snapshot of the target network. Assert
`sessionCommitteeManagement.currentCommittee` and `.queuedCommittee` survive
non-empty and in the new key shape.

**Action.** Deploy the v3 runtime (adds babe pallet, consensus-engine pallet,
the `babe` session key and runs `MigrateV1ToV2AddBabeSessionKeys`) through the
standard governance runtime-upgrade flow. Rebuild tooling metadata again
(`SessionKeys` shape changed). Note this ordering — upgrade only after the 1.1
checks — is enforced by release management, not on chain.

**Verify:**

- [ ] Every validator has authored at least one **finalized** block since
    the upgrade.
- [ ] Runtime log on the upgrade block: `translating committee & session
     keys and initializing QueuedCommittee`.
- [ ] `consensusEngine.engineState()` is present and returns `Aura`; cadence unchanged.
- [ ] After the next committee rotation: `babe.authorities()` is non-empty
     and matches the registered keys. (BABE `NextEpochData` consensus
     digests now appear at session boundaries — expected.)

---

## Phase 2 — Arm BABE (`Aura` → `ArmedBabe`)

**Action.** Drive a federated-authority motion for
`consensusEngine.armBabe()`:

1. Council: propose, vote (2/3), close.
2. Technical Committee: same, for the identical call.
3. Anyone: `federatedAuthority.motionClose(motion_hash,
  proposal_weight_bound)` (a too-low bound fails with
  `MotionWeightBoundTooLow`).

Arming runs on the v3 runtime with session keys including babe and nodes v3.
From the next block, every author attaches a BABE `SecondaryPlain`
pre-digest alongside the AURA one; blocks without it are rejected.

**Verify:**

- [ ] `MotionDispatched.motion_result` is success **and**
     `consensusEngine.engineState()` returns `ArmedBabe` (the pallet emits
     no events — always re-read the state).
- [ ] Runtime log on the motion block:
     `BABE armed: pre-seeded pallet-babe GenesisSlot…` (target
     `consensus-engine`).
- [ ] New blocks carry two pre-runtime digests (AURA, then BABE
     `SecondaryPlain`).
- [ ] Cadence unchanged (~6 s); every validator still authoring; no spike
     in rejected blocks.

**Gate to Phase 3:**

- [ ] The first `ArmedBabe` block is **finalized** by GRANDPA.

---

## Phase 3 — Schedule the flip (`ArmedBabe` → `ScheduledFlip`)

**This is the point of no return** — the action that triggers the flip
itself. No further governance call follows: block production continues on
AURA until the runtime commits the flip at the next last-slot-of-an-epoch
it reaches with BABE authorities populated.

**Action.** Drive a federated-authority motion for
`consensusEngine.scheduleFlip()` (same flow as Phase 1).

**Verify:**

- [ ] `consensusEngine.engineState()` returns `ScheduledFlip`.
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
BABE (`chain already on BABE at startup`). BABE RPC is not wired (no
`babe_epochAuthorship`) — query storage instead; no BABE import-queue
metrics; BABE equivocation reporting is disabled.

---

## Troubleshooting

| Symptom | Cause | Action |
|---|---|---|
| Motion closes but `MotionDispatched.motion_result` carries `InvalidEngineState`; `engineState` unchanged | Call dispatched from the wrong state (e.g. `scheduleFlip` while still `Aura`) | Check `engineState` before opening motions; the rehearsal tooling refuses wrong-state motions for this reason |
| After arming: a validator's blocks are rejected (`BABE pre-runtime digest required in state 'ArmedBabe'`) | That validator runs a pre-migration binary (or otherwise doesn't emit the digest) | Upgrade the binary. The chain loses that validator's slots until fixed; other validators are unaffected |
| `failed to seed BABE epoch tree at …; BABE import/authoring may stall` (ERROR, target `babe-authoring`) | Epoch-tree bootstrap at the flip failed on that node | Investigate that node; restarting it after the flip re-attempts the bootstrap path |
| A validator stops authoring after the flip; earlier `WARN … none match the requested public … falling back to AURA` from `aura-to-babe-migration-keystore` | On-chain BABE key matches neither the keystore's BABE key nor its AURA key (e.g. registered one key, loaded a different one) | Load the registered key into the keystore, or fix the registration and wait for a rotation. `midnight_babe_key_registered` catches this **before** the flip — keep it green |
| `midnight_babe_key_registered == 0` on a validator at the Phase 1.1 gate | Keystore key missing or not registered on Cardano (0.1), registration not yet observed, or key mismatch | Complete 0.1 for that validator, or wait out the observation lag; re-check before 1.2 |
| A committee member never appears in the applied validator set; node logs `Could not set_keys for … error: … DuplicatedKey` at session rotation | The permissioned-candidates list published on Cardano (a governance action) contains a session key already owned by another — possibly former — candidate; the committee pallet delivers whatever it observes | **Treat as an incident — the chain is in undefined behavior and there is no in-band fix**. Prevention — the uniqueness audit of the governance-published candidate list at the Phase 1.1 gate and before every list change — is the only protection. Escalate immediately; the failure is log-only, with no on-chain event |
| `author_hasKey`/`author_hasSessionKeys` report a BABE key that isn't there | These RPCs answer through the migration keystore's AURA fallback | Use the metric, the `BABE pubkey:` startup log, or the `62616265…` keystore file instead |
| Node panics importing a block with an unexpected digest combination (e.g. `BABE pre-runtime digest present in state 'Aura'`) | An author emitted digests inconsistent with the on-chain state — misconfigured or malicious author | The block is invalid and rejected network-wide; identify the author and follow up |
| Need to abort the migration | — | There is no un-arm call, but `arm_babe` is only a marker of intent — no flip is committed while armed. The consequential action is `scheduleFlip`; once in `ScheduledFlip` the flip commits automatically at an epoch boundary. Reverting any state requires an emergency runtime upgrade via governance |

---

## Rehearsal in lower environments

`local-environment/` in the midnight-node repository drives the full flow
against a local or forked network:

```bash
# Aura -> ArmedBabe
npm run consensus-upgrade-arm-babe:local-env -- \
 --technical-uris //One //Two //Three \
 --council-uris //Four //Five //Six \
 --executor-uri //One

# ArmedBabe -> ScheduledFlip (only after Phase 2 and finalized dual-digest blocks)
npm run consensus-upgrade-schedule-flip:local-env -- \
 --technical-uris //One //Two //Three \
 --council-uris //Four //Five //Six \
 --executor-uri //One
```

Both commands refuse wrong-state motions and verify the transition
afterwards. Combine with `image-upgrade` / `governance-runtime-upgrade` /
`full-upgrade` to rehearse the complete sequence (binary rollout → arm →
session-keys upgrade → schedule → flip) against forked network state. The
local environment's validators are deliberately configured as a key-setup
test matrix (fully configured, missing keystore key, unregistered key, …) —
use it to observe every failure mode above before the real run. See
[local-environment/README.md](https://github.com/midnightntwrk/midnight-node/blob/main/local-environment/README.md)
and [fork-testing.md](https://github.com/midnightntwrk/midnight-node/blob/main/docs/fork-testing.md).

---

## Quick reference

| What | Where |
|---|---|
| Rollout staging | session-keys runtime → arm → **first finalized `ArmedBabe` block** → **all registrations observed** (2.1 green) → ≥ 1 rotation → schedule → flip |
| Engine state | `consensusEngine.engineState()` → `Aura` / `ArmedBabe` / `ScheduledFlip` / `Babe` (storage query; no runtime API or RPC exposes all four states) |
| Governance calls | `consensusEngine.armBabe()`, `consensusEngine.scheduleFlip()` (root, via `federatedAuthority.motionApprove` × 2 bodies + `motionClose`) |
| BABE authorities | `babe.authorities()` |
| Validator readiness metric | `midnight_babe_key_registered` (validators with Prometheus; 1 = keystore key registered on Cardano) |
| BABE key type | `babe` (keystore file prefix `62616265`), sr25519; loaded via `BABE_SEED_FILE` or `midnight-node key insert` (never `author_insertKey`) |
| Cardano registration | permissioned-candidates list `babe_pub_key` (all candidates are permissioned) |
| Flip timing | Last slot of an epoch while in `ScheduledFlip` and `babe.authorities()` non-empty; BABE genesis = first slot of next epoch |
| Log targets | `consensus-engine` (arm, flip, postponement), `babe-authoring` (flip, epoch-tree seed, handover), `aura-to-babe-migration-keystore` (key fallbacks), `babe-key-readiness` (probe), `committee-membership` (session membership) |
