#node #ledger #hardfork
# Pin ledger 9 to the final 9.1.0.1 tags

Ledger 9 moves from the `-rc.N` per-crate tags to the final ones: `crate-ledger-9.1.0.1`,
`zswap-9.0.1`, `coin-structure-3.0.1`, `base-crypto-1.1.1`, `base-crypto-derive-1.1.0`,
`transient-crypto-3.0.1`, `transient-crypto-2.2.0`, `onchain-vm-4.0.1`, `onchain-state-4.0.1`,
`crate-onchain-runtime-4.0.1`, `crate-zkir-2.2.0`, `serialize-1.2.1`, `storage-3.0.0`,
`storage-core-1.2.3`, `storage-macros-1.0.1`. Every crate now comes from one source; the three
that were still pinned to the public ledger repo (`base-crypto-derive`, the 2.x
`transient-crypto`, `zkir`) follow the rest.

Thirteen of the fifteen tags point at the same commit as the rc tag already locked, so the
ledger, VM, storage and zswap source is identical to the rc.2 build. Two crates moved:
`transient-crypto` 2.2.0 takes `CtOption` from `subtle` directly instead of through `k256`,
and `zkir` 2.2.0 raises its proofs/circuits/zk-stdlib requirements to the versions already
locked and silences the deprecated coordinate getters it keeps for verifier-key stability.
Ledger 8 stays on crates.io 8.1.3.

PR: https://github.com/shieldedtech/shadow-midnight-node/pull/109
