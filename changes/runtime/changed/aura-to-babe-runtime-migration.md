#runtime #consensus #babe #migration #partner-chains

# Runtime support for the AURA→BABE migration

- **`pallet-babe`** is added (index 3, right after `Aura`) and `opaque::SessionKeys` gains a
  `babe` key. Genesis/chain-spec construction, the partner-chains CLI and the permissioned
  candidates configs (`dev`, `devnet`, `govnet`, `guardnet`, `local`, `perfnet`, `stagenet`,
  mock bridge data) carry it. `qanet`, `preview`, `preprod` and `mainnet` are regenerated from
  Cardano once candidates publish `babe` keys.
- **`pallet-consensus-engine`** (index 10) drives the state machine `Aura` → `ScheduledFlip` →
  `Babe`. Governance calls `schedule_flip` (requires `Aura`, else `InvalidEngineState`); the flip
  commits automatically at the last block of an epoch, postponed while `pallet-babe` has no
  authorities, and writes the BABE genesis slot and epoch config. `ConsensusEngineApi` exposes
  `active_engine`; `ConsensusEngine::current_slot()` reads the active engine's slot.
- **BABE-compatible session rotation.** BABE accepts exactly one epoch-change announcement per
  epoch, in its first block. `pallet-session-validator-management` used to catch up skipped
  epochs (a sidechain epoch without blocks) with one rotation per block; the second rotation would
  announce another epoch change and every later block would fail BABE import
  (`UnexpectedEpochChange`). It now catches up in a single rotation: a late rotation stamps the
  queued (and promoted) committee with the current epoch, so the inherent selects for the epoch
  after the current one and no further rotation is due until the next epoch; the committee due in
  a skipped epoch serves the current one. As an independent safety net
  `pallet_session::Config::ShouldEndSession` is now `ConsensusEngine`, which forwards to the
  committee pallet and, once in `Babe`, holds back any second rotation within a BABE epoch
  (`LastRotationBabeEpoch`, recorded in `on_finalize` from `pallet_babe::EpochStart`; a rotation
  enacted in the flip block itself is not counted, since its pre-genesis slot saturates to BABE
  epoch 0 and the first BABE block's rotation must go through).
- **Digest guards** (keyed on the pre-runtime engine id): `Aura`/`ScheduledFlip` blocks must carry
  the AURA pre-digest followed by a matching BABE `SecondaryPlain` one; `Babe` blocks must carry no
  AURA pre-digest. Index 10 makes the guards run before `Scheduler` and `Session`, so they see
  parent-state authorities.
- **Single migration `MigrateV1ToV2AddBabeSessionKeys`** (committee storage v1 → v2) replaces the
  partner-chains `V1ToV2Migration`: translates `CurrentCommittee`/`NextCommittee` and
  `pallet-session` keys to the new shape (BABE key copied from AURA key), seeds `QueuedCommittee`
  from `CurrentCommittee`, and activates the consensus engine (pre-seeds
  `pallet_babe::GenesisSlot` with a sentinel so `pallet-babe` does not self-initialize). Has
  try-runtime checks. The generic `AuthorityKeysMigration` stays in
  `pallet-session-validator-management` for future key-shape changes.
- **Session and committee plumbing** the migration builds on:
  - Stock `pallet_session` replaces `pallet-partner-chains-session`; `SessionCommitteeManagement`
    implements `SessionManager`/`ShouldEndSession` and registers committee keys automatically at
    genesis and on each rotation (`pallet_session::historical` enabled, `set_keys` extrinsic
    disabled). Runtime upgrade to `2.1.0`.
  - Rotation tracks `NextCommittee` → `QueuedCommittee` (handed to `pallet_session`) →
    `CurrentCommittee` (the active set); BEEFY stakes are matched against these.
  - A failed `set_keys` aborts the rotation: the previous authority set is kept, the failed
    committee is consumed, and genesis panics if the initial committee cannot be registered.
  - Block authors come from upstream `pallet_authorship` (index 9) via
    `FindAccountFromAuthorIndex<Self, Aura>`.
- The activation block must be finalized before `schedule_flip` (enforced by the runbook).

Metadata is regenerated.

PR: https://github.com/midnightntwrk/midnight-node/pull/2113
Issue: https://github.com/midnightntwrk/midnight-node/issues/1742
PRs: https://github.com/midnightntwrk/midnight-node/pull/1800, https://github.com/midnightntwrk/midnight-node/pull/1876, https://github.com/midnightntwrk/midnight-node/pull/2078
