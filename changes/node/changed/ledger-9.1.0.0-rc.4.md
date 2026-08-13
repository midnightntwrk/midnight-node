#node #runtime #toolkit

# Bump ledger to 9.1.0.0-rc.4

Moves the midnight-ledger patch set from the per-crate 9.1.0.0-rc.3 tags to the
9.1.0.0-rc.4 release. Upstream's fix is dust registration accounting: a
registration's initial DUST is now valued at block time rather than the
transaction's declared `ctime`, and dust-generation uniqueness is keyed on the
initial nonce alone (`DustGenerationState::night_indices`) instead of the
`generating_set`. The dust spend circuit changed with it, so the `spend.zkir` /
`spend.verifier` artefacts are new.

Depends on ledger 7 being decommissioned first - see below.

## One tag for the whole stack

rc.4 tags only the workspace (`ledger-9.1.0.0-rc.4`); there are no per-crate rc.4
tags. Every L9 crate is therefore pinned to that single commit, which is also
more self-consistent than the previous mix of per-crate tags straddling several
commits. `midnight-transient-crypto-old` stays on `transient-crypto-2.2.0-rc.1`:
the 2.x crypto stack is frozen and rc.4 only ships 3.x.

`midnight-storage` and `midnight-storage-core` join the patch table. rc.4 turned
the ledger workspace's intra-repo dependencies into path deps, so the L9 crates
now carry their own copies of both rather than resolving the published ones -
and the in-repo copies are on `digest 0.11` while the crates.io releases of the
same version numbers (`midnight-storage` 2.0.2, `midnight-storage-core` 1.2.0)
are on `digest 0.10`. Cargo cannot patch a path dep inside a git source, so
without these two entries ledger 8 and ledger 9 end up with distinct
`DB`/`Storable` traits and the v8->v9 state translation stops type-checking.

## Why this needed ledger 7 gone

Only one `midnight-storage-core` can exist in the graph. Ledger 7's
`midnight-storage` 1.1.1 is a digest-0.10 crate and shares `storage-core` (and
its arena, see `ledger/helpers/src/fork/fork_7_to_8.rs`) with 8 and 9, so
pointing `storage-core` at rc.4 broke it and not pointing it there broke 8<->9.
Decommissioning ledger 7 removes `midnight-storage` 1.1.1 from the graph
entirely - it had exactly one dependent - and the conflict disappears.

## Node-side changes

- Workspace `sha2` moves 0.10.8 -> 0.11.0 to track the ledger's `storage-core`.
  `WellBehavedHasher` is implemented for *its* `Sha256`, so a 0.10
  `sha2::Sha256` is a different type and no longer satisfies `DB::Hasher` in
  `ParityDb<sha2::Sha256, ...>`. `digest 0.11` also replaced `generic_array`
  with `hybrid-array`, so `sha2::digest::generic_array::typenum::U32` becomes
  `sha2::digest::consts::U32`.
- `GenerationInfoAlreadyPresent` was renamed to `InitialNonceAlreadyPresent` on
  both `TransactionInvalid` and `SystemTransactionError`. The variant is spelled
  differently per ledger generation, so it moves out of the shared
  `common/conversions.rs` into the per-version `error_ext::ledger_8` /
  `error_ext::ledger_9` shims. The node-side `InvalidError` /
  `SystemTransactionError` variant names and their error codes (198 / 207) are
  unchanged - it is the same condition, and they are a stable runtime surface.

Runtime metadata is not regenerated: no pallet storage item, extrinsic signature
or runtime API changed.

PR: <link to PR>
Issue: <link to Github Issue, if applicable>
