#runtime #testing
# Optional `fork-transition` feature for syncing a forked chain from genesis

A chain forked locally by `mock-authorities convert` contains one block no
honest proposer could have produced: its seal is signed by a key absent from the
parent's on-chain authority set, and its state root reflects a hand-built delta.
A node full-syncing that chain from true genesis therefore stops at the fork
point, which made it impossible to test a fresh node against a forked network.

The new `fork-transition` cargo feature (off by default) builds a runtime that
relaxes exactly three things at exactly one configured block height:

- `AuraApi::authorities` reports the fork's mock set for the fork block's
  *parent*, so the fork block's seal verifies;
- `check_inherents` is skipped for the fork block, and *only* that block. It
  carries a storage delta rather than the inherents a proposer would have
  produced, and the macro-generated `check_extrinsics` panics on a body with no
  inherents rather than returning an error, so there is no graceful alternative.
  Inherent checking asks whether a proposer honestly reflected main-chain state;
  the fork block had no proposer. Seal verification and the state-root check are
  untouched, and the delta must reproduce the header's root or the block is
  rejected;
- `Core::execute_block` applies that delta instead of running `Executive`, which
  reproduces the header's state root exactly rather than asserting it.

A syncing node picks it up through the chain spec's stock `codeSubstitutes`
field; the released node binary gains no new switch. The height and authority
set are compile-time (`MIDNIGHT_FORK_HEIGHT`, `MIDNIGHT_FORK_AURA_AUTHORITIES`),
so a fork runtime only works for the fork it was built for, and a build without
them is inert.

The delta travels as an opaque, magic-prefixed block-body blob rather than a
dispatchable, so the feature adds no pallet, call variant, or metadata change: a
runtime built without it is byte-identical to the released one.

The runtime must be built from the source revision matching the forked chain's
on-chain `spec_version`: `codeSubstitutes` is keyed by it, and a mismatch makes
the substitute silently inert - the fork block is then rejected as "Bad
signature" rather than reported as a misconfiguration.

**A runtime built with this feature must never be released.**

Fork points are chosen, not assumed: a verifying node resolves the fork block's
inherited main-chain reference through the mock follower, which only ~2% of real
Cardano hashes satisfy, so `mock-authorities` gained `--fork-at` to fork at a
compatible parent (typically a few dozen blocks back).

PR: <link to PR>
