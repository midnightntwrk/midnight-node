#node #consensus #babe #aura

# Node support for the AURA→BABE migration

A node can verify, import and author blocks of both engines across the flip. Operator procedure:
`docs/aura-to-babe-migration-runbook.md`.

- **Pre-digest:** every AURA block carries an additional BABE `SecondaryPlain` pre-runtime digest
  (same slot and author index), emitted unconditionally by `BabePreDigestProposerFactory`. The
  runtime requires it from the first block of the runtime that adds the pallet.
- **Single import queue, dispatched per block:** `EngineDispatchVerifier` / `EngineDispatchBlockImport`
  route each block to the AURA or BABE pipeline by the engine that authored it, read from the block
  header (`node/src/engine_digests.rs`). BABE pre-digests are only trusted when the block's
  `pallet-version` digest is ≥ `ACTIVATION_SPEC_VERSION` (3_000_000), so older blocks stay AURA
  and warp/gap sync works. The BABE epoch tree is seeded on the verify path.
- **Single authoring task:** `run_authoring_supervisor` runs AURA until the flip, then BABE; one-way.
- **Starts on runtimes without `BabeApi`** (placeholder BABE configuration), so binaries can be
  rolled out before the runtime upgrade.
- **Keystore fallback:** `AuraToBabeMigrationKeystore` answers BABE requests with the AURA key when
  the requested BABE key is unusable (warns off the hot path). Covers only the migration window.
- **Committee membership watcher** is engine-agnostic (uses `ConsensusEngineApi::active_engine`).
- **Metric `midnight_babe_key_registered`:** 1 if the keystore holds a BABE key registered on
  Cardano as a permissioned candidate's `babe` key, else 0 (reads the plain keystore, not the wrapper).
- **Consensus crates:** the forked AURA crates are replaced by consensus-agnostic
  `sc-partner-chains-consensus` (`PartnerChainsBlockImport`, `PartnerChainsVerifier`,
  `PartnerChainsProposerFactory`).

PR: https://github.com/midnightntwrk/midnight-node/pull/2113
Issue: https://github.com/midnightntwrk/midnight-node/issues/1757
