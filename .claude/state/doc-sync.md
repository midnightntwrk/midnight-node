# doc-sync: ledger-10 addition

- ledger/helpers/unsafe/src/fork/fork_aware_context.rs:218 - doc comment "Deserialize raw transactions and apply to a Ledger8 context, returning dust events." now sits above the newly-inserted `block_context_from_raw_10` (line 219) instead of `apply_block_8` (line 227), which it describes. Move the `///` line down to directly precede `pub fn apply_block_8`, and give `block_context_from_raw_10` its own (or no) doc comment.

## cargo rustdoc -D rustdoc::broken_intra_doc_links -D rustdoc::private_intra_doc_links (pre-existing, not introduced by this diff, but ledger_10 mechanically inherited one)

- ledger/src/lib.rs:20 - `[`boundary`]` links to private item (pre-existing)
- ledger/src/host_api/ledger_8.rs:40,50 - unresolved `Ledger8Bridge::dust_generation_values` (pre-existing)
- ledger/src/host_api/ledger_9.rs (approx) - unresolved `Ledger9Bridge::migrate_state_v8_to_v9` (pre-existing)
- ledger/src/ledger_8/mod.rs:1050, ledger_9/mod.rs:1050, ledger_10/mod.rs:1051 - `get_any_transaction_cost` doc links to private `SystemTransaction` (copied into ledger_10 verbatim from ledger_8/9)

All other pub-item docs, README.md/PRD.md ledger-version mentions, and the `ledger_10/*` copied files are internally consistent and correctly updated (verified: ledger/src/lib.rs, ledger/src/ledger_9/mod.rs, ledger/src/ledger_10/mod.rs, ledger/helpers/unsafe/src/fork/{mod.rs,fork_9_to_10.rs}, ledger/helpers/unsafe/src/extract_tx_with_context.rs, ledger/helpers/src/fork/raw_block_data.rs, util/toolkit/README.md, util/toolkit/PRD.md, util/toolkit/src/fetcher/wallet_state_cache.rs, util/toolkit/src/tx_generator/builder/mod.rs, util/toolkit/src/tx_generator/builder/builders/{mod.rs,ledger_9/type_convert.rs}, util/toolkit/src/commands/*).
