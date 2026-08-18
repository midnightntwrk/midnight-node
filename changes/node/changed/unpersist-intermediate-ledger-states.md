#storage
# Un-persist transient ledger states to allow for garbage collection

Replace the raw `Vec<u8>` ledger state key encoding with a typed enum so the Bridge distinguishes states that must be retained for history (post-block tips, genesis) from intra-block intermediates that can be cleaned up by their successor — at the type level rather than by convention.

```rust
pub enum LedgerStateKey {
    Anchored(Vec<u8>),  // never unpersisted on input
    Transient(Vec<u8>), // unpersisted by successor
}
```

`apply_transaction` and `apply_system_transaction` take `&LedgerStateKey`, return `Transient`, and only unpersist the input when it's `Transient`. `post_block_update` and `apply_post_block_update` return `Anchored`. Anchored inputs are left alone, which makes sibling forks safe (importing two blocks built on the same Anchored parent does not unpersist it twice) and shrinks the failed-block leak from K states to 1. The previous "extra persist" in `post_block_update` and the genesis double-persist are no longer needed: Anchored states sit at rc=1 and the Bridge never unpersists them.

The new signatures are exposed as `#[version(2)]` host functions on the **ledger-9 bridge only** — the one bridge the current runtime calls. Version 1 there keeps the raw-bytes ABI (used by runtime 2.0.0), treats its input as `Anchored`, and so reproduces the previous behaviour exactly. The ledger-7 and ledger-8 bridges keep the legacy ABI and nothing else, since only ledger-7/8 runtimes ever reach them.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1442
PR: https://github.com/midnightntwrk/midnight-node/pull/1443
