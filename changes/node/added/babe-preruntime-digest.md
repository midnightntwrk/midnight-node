#node

# Emit a BABE SecondaryPlain pre-runtime digest on every AURA block

Block authors attach a BABE `SecondaryPlain` pre-runtime digest to every block the AURA slot
worker produces. The digest carries the block's AURA slot and the same author index as the AURA
pre-digest (`slot % n_authorities`).

Emission is not gated on chain state: running a migration-aware node is the declaration of
intent to move to BABE. Blocks authored before the runtime upgrade already carry the digest,
which a runtime without `pallet-consensus-engine` ignores, and so does the runtime-upgrade
block itself, whose author cannot yet ask the new runtime anything — which is what lets the
pallet require the digest from its first block.

- Adds `BabePreDigestProposerFactory` (`node/src/babe_pre_digest_proposer.rs`), a proposer
  wrapper that reads the AURA authority count at the parent and appends the pre-digest.
- `local-environment` gains `consensus-upgrade-schedule-flip`, which expects the engine in
  `Aura`.

PR: https://github.com/midnightntwrk/midnight-node/pull/1951
Issue: https://github.com/midnightntwrk/midnight-node/issues/1751
