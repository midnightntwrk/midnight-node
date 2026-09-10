#node #consensus #babe #polkadot-sdk

# Use the shieldedtech polkadot-sdk fork (stable2609 + BABE `build_verifier`) and unify the import queue

Points all polkadot-sdk dependencies at `shieldedtech/polkadot-sdk` branch `origin/stable2609`.
The branch is upstream `paritytech/polkadot-sdk` `stable2609` plus paritytech/polkadot-sdk#13061,
which exposes `sc_consensus_babe::build_verifier` / `BuildVerifierParams` (the Aura equivalent) so
a BABE verifier can be composed into a custom import queue.

With that, the AURA→BABE migration no longer runs two whole import queues behind a dispatcher
(`DispatchImportQueue`, with its cross-queue ordering gate and held BABE batches). The node has one
`BasicQueue` whose verifier and block import route each block to the AURA or BABE pipeline by the
engine that authored it (`EngineDispatchVerifier` / `EngineDispatchBlockImport`). Ordering at the
flip follows from the single import worker, BABE's epoch tree is seeded right before the first BABE
block is verified, and the BABE `answer_requests` worker (plus its keep-alive task) is gone. The
warp ledger-sync import gate now wraps both pipelines, so post-warp BABE blocks are held during
arena recovery like AURA blocks.

`shieldedtech/polkadot-sdk` is added to `deny.toml`'s git allow-list.

PR: https://github.com/midnightntwrk/midnight-node/pull/2113
Issue: https://github.com/midnightntwrk/midnight-node/issues/1757
