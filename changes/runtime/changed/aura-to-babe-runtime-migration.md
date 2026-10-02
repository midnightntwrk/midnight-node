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
