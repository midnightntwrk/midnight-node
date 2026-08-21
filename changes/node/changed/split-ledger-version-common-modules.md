#node #refactor
# Give each ledger generation its own copy of the wrapper code

`ledger/src/versions/common/**` was compiled twice via a `#[path = "versions"]`
module-parameterization trick, once bound to the ledger-8 crates and once to the
ledger-9 crates, with `super::` resolving differently in each instantiation. A
reader of a file under `versions/common/` could not tell which ledger crate
`mn_ledger_local` referred to, and an edit meant for one generation silently
applied to both.

Each generation now has its own directory, `ledger/src/ledger_8/` and
`ledger/src/ledger_9/`, with absolute `crate::ledger_N::…` imports so every file
states the generation it binds. `ledger/src/common/` — the SCALE types crossing
the runtime/client interface, which are version-independent by design and
compiled once — is renamed to `ledger/src/boundary/` so no folder is called
`common` any more.

No behaviour change: rustc already emitted both instantiations, so this only
makes the existing duplication visible in the source tree. Public paths
(`ledger_8::…`, `ledger_9::…`, `latest::…`, `types::…`, `host_api::…`) are
unchanged, and `diff -r src/ledger_8 src/ledger_9` now shows exactly where the
two generations diverge: `error_ext`, `guaranteed_validation`, `post_block_update`,
`system_tx`, and `mod.rs`'s crate aliases.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1768
