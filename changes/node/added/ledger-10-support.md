#node #runtime #toolkit

# Add ledger 10 (10.1.0.0-alpha.1) as the third supported ledger generation

The node, runtime and toolkit now carry ledger 8, 9 and 10 side by side, with 10 as
`latest`: the runtime's `active_ledger_bridge` is the new `ledger_10_bridge` host API,
and the toolkit builds transactions at ledger 10 on a chain that has reached it while
still replaying ledger 8 and 9 history through their own copies.

Ledger 10 changes no on-chain serialisation tag (`ledger-state[v18]` is shared with
ledger 9), so the 9->10 switch needs no state migration: a v9 arena root *is* the v10
root, and the `migrate_state_v8_to_v9` host function is retained on the ledger-10
bridge so an 8->10 upgrade still translates through the v2 multi-block migration.
What does change is the proof system: transient-crypto 4 (`proof[v6]`, deferred
accumulators, `InnerProofWitness`), zkir-v3 `Bytes32`/curve ops, and
`DUST_SPEND_PROOF_SIZE` 2912 -> 2915.

Per-generation copies (`diff -r ledger_9 ledger_10` shows the delta, which is small):
`ledger/src/ledger_10`, `ledger/src/host_api/ledger_10.rs`,
`ledger/helpers/src/ledger_10`, `ledger/helpers/unsafe/src/ledger_10`,
`ledger/helpers/unsafe/src/fork/fork_9_to_10.rs`,
`util/toolkit/src/tx_generator/builder/builders/ledger_10`,
`util/toolkit/src/commands/fork/ledger_10`. `LedgerVersion` gains `Ledger10 = 3`
(runtime spec 3.0.0+; 2.x stays ledger 9), `ForkAwareLedgerContext::Ledger10`, and the
toolkit's replay/cache paths dispatch three ways.

## Dependency shape (ledger-10-only build)

The published `ledger-10.1.0.0-alpha.1` tag is a workspace tag whose manifests use
`path = "../x"` deps, so its crates cannot share serialize / base-crypto / storage-core with
the rc.5 crates ledger 8 and 9 pin (two `Tagged` traits in one graph). Until the ledger team
publishes per-crate isolate tags for ledger 10, the workspace consumes the alpha tag directly
and **ledger 8 and 9 are compiled out** behind a `legacy-ledgers` cargo feature that cannot
currently be enabled: their crate pins are kept commented out in `Cargo.toml`, every
ledger-8/9 code path is `#[cfg(feature = "legacy-ledgers")]`, and the ledger-8 host API is a
stub whose `dust_generation_values` reports `NoLedgerState` (the cNIGHT v2 migration treats
that as "nothing to restore"). Consequences while the feature is off:

- the node cannot run on a chain that has ledger-8/9 history (no host functions for it), and
  warp-sync of a v13 arena is unsupported;
- the toolkit refuses (with a clear panic) to replay a source that starts on ledger 8 or 9,
  and cannot decode pre-10 blocks;
- the shared storage/serialize stack is the alpha's own, which predates the rc.5 hardening
  (MPT canonicity invariant, canonical HashMap/HashSet decode) - acceptable on a dev chain,
  not for release.

PR: <link to PR>
Issue: <link to issue>
