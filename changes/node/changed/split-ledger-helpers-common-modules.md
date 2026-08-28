#node #refactor
# Give each ledger version its own copy of the ledger helpers

`ledger/helpers/src/versions/common/**` was compiled twice via a
`#[path = "versions"]` module-parameterization trick, once bound to the ledger-8
crates and once to the ledger-9 crates, with `super::` resolving differently in
each instantiation. A reader of a file under `versions/common/` could not tell
which ledger crate `mn_ledger` referred to, and an edit meant for one version
silently applied to both. This is the same footgun #2059 removed one layer down,
in `ledger/src/`.

Each version now has its own directory, `ledger/helpers/src/ledger_8/` and
`ledger/helpers/src/ledger_9/`, with absolute `crate::ledger_N::…` imports so
every file states the version it binds. The two inline modules in `lib.rs`
become the respective `mod.rs` files, and the version-specific single files are
renamed to line up: `versions/block_context/post_ledger_8.rs` →
`ledger_N/block_context.rs`, `versions/ecdsa_unimpl.rs` → `ledger_8/ecdsa.rs`,
`versions/test_utilities_compat.rs` → `ledger_8/test_utilities_local.rs`.

No behaviour change: rustc already emitted both instantiations, so this only
makes the existing duplication visible in the source tree. Public paths
(`ledger_8::…`, `ledger_9::…`, `latest::…`, and the crate-root glob) are
unchanged — `pub use common::*;` already flattened `common` away — so no
consumer needed an edit, and the runtime metadata is untouched.

`diff -r src/ledger_8 src/ledger_9` now shows exactly where the two versions
diverge: `mod.rs` (crate aliases, `LEDGER_VERSION`, `CRATE_NAME`, the
signature/verifier-key helpers), `ecdsa.rs`, `test_utilities_local.rs`, and the
ledger-9-only `ecdsa_wallet_tests.rs`.

PR:
Issue: https://github.com/midnightntwrk/midnight-node/issues/1768
