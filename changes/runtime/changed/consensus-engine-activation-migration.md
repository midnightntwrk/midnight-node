#runtime #consensus #migration

# Activate `pallet-consensus-engine` through the runtime upgrade

The migration sequence is: roll out the migration-aware node → runtime upgrade → governance
`schedule_flip` → automatic flip. The state machine is `Aura` → `ScheduledFlip` → `Babe`, and
`schedule_flip` (call index 0) requires `Aura`.

- From the very first block executed by a runtime that contains the pallet, every
  `Aura`/`ScheduledFlip` block must carry the AURA pre-runtime digest followed by a matching
  BABE `SecondaryPlain` one; after the flip (`Babe`) no AURA pre-digest may be present.
- The upgrade activates the pallet from the runtime's `MigrateV1ToV2AddBabeSessionKeys`
  migration, which calls `pallet_consensus_engine::Pallet::activate` to pre-seed
  `pallet_babe::GenesisSlot` with a non-zero sentinel so pallet-babe does not self-initialize
  its genesis epoch from the first BABE pre-digest it sees. Runtime migrations run before any
  `on_initialize`, so the sentinel is in place before pallet-babe inspects the upgrade block.
  The flip overwrites it with the real genesis slot. The activation is deliberately not a
  version-gated migration of `pallet-consensus-engine` itself: FRAME initializes a brand-new
  pallet's on-chain storage version to the in-code one before any migration runs, so such a
  migration never fires; the committee pallet's 1 → 2 transition is what identifies this upgrade
  exactly once. Chains that have the pallet from genesis run no migration; pallet-babe then
  self-initializes at block 1, which is harmless.
- `ConsensusEngineApi` exposes only `active_engine`; block authors do not consult the runtime
  about the pre-digest.

The runtime knows nothing about finality, so the operational rule that the runtime-upgrade
(activation) block must be finalized before `schedule_flip` is enforced by the runbook
(`docs/aura-to-babe-migration-runbook.md`).

PR: TBD
Issue: TBD
