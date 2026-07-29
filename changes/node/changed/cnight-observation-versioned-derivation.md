#node #runtime

# Version-gate cNIGHT observation: freeze v1, add gated one-block v2

The cNIGHT observation inherent is derived by the block author and re-derived
byte-for-byte by every importing node (`check_inherent`), so any change to the
derivation is a consensus change. Post-1.0 work (sliding-window bulk cache,
fetch-determinism fix) had altered the derivation in place. Mainnet-sync testing
showed it re-derives everything that happened on mainnet up to the tested
instant, but equivalence beyond that point is not guaranteed by construction — a
divergence risk for the v1 derivation that authored finalized mainnet history.

This factors the derivation into explicit, versioned functions selected by the
parent runtime `spec_version`:

- `derive_inherent_v1` reproduces `release/node-1.0.1` verbatim — the frozen
  baseline, latent row-vs-tx skip bug preserved on purpose — for blocks before
  the v2 activation, so finalized history stays replayable.
- `derive_inherent_v2` implements "one Cardano block per inherent":
  deterministic by construction, no cross-block truncation and no row-limit
  completeness flag, cursor always a real position. An oversized block (more
  cNIGHT events than the envelope) drains across several inherents at whole-tx
  boundaries inside the block.
- The IDP reads `spec_version` at `parent_hash` and picks v1/v2; `create_inherent`
  and `check_inherent` follow the same parent, so they never disagree. The
  activation `spec_version` is a placeholder (`u32::MAX`), so v2 is DISABLED until
  the v2 runtime upgrade ships and pins it: this lands as a no-op — every block
  derives v1 — and changes no live behaviour.

Guard tests: unit tests pin the frozen v1 truncate/cursor semantics and the v2
one-block selection; a db-sync-gated harness (`tests/cnight_v1_replay.rs`)
re-derives v1 over real Cardano data and, with a ground-truth fixture, asserts
byte-equality against on-chain inherents.

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
