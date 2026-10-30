#toolkit #refactor
# Give each ledger version its own copy of the builders

`util/toolkit/src/tx_generator/builder/builders/common/**` and
`util/toolkit/src/commands/fork/common/**` were each compiled twice via a
`#[path = "common"] pub mod inner { … }` trick, once bound to the ledger-8
helpers and once to the ledger-9 helpers, with `ledger_helpers_local` resolving
differently in each instantiation. A reader of a file under `common/` could not
tell which ledger version it was looking at, and an edit meant for one version
silently applied to both.

Each version now has its own directory — `builders/{ledger_8,ledger_9}/` and
`commands/fork/{ledger_8,ledger_9}/` — and each file names its own version with
a `use midnight_node_ledger_helpers::ledger_N as ledger_helpers_local;` line, so
the two copies stay byte-identical apart from that one word and `diff -r` can
police them.

The `inner` module wrapper is gone (it was referenced nowhere), so every
external path — `builders::ledger_8::SingleTxBuilder`,
`fork::ledger_9::show_wallet`, … — is unchanged.
`impl_encoded_zswap_conversions!` stays in `builders/mod.rs`: `ledger_storage`
still aliases to `ledger_storage_ledger_8` in both versions, so duplicating the
impls per version would still hit E0119.

With this, no directory in the repo is compiled more than once:
`grep -r '#\[path' --include='*.rs'` returns nothing.

PR: https://github.com/midnightntwrk/midnight-node/pull/2075
Closes: https://github.com/midnightntwrk/midnight-node/issues/1768
