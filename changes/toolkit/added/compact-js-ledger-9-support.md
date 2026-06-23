#toolkit #compactc #build #test
# Compact-js 2.5.3 / ledger-9 support in toolkit-js

Adds the compact toolchain needed to generate ledger-9 contract transactions from the
toolkit, and re-enables the toolkit-js tests/CI that `partial-ledger-9-support` had gated.

**New `compact-0.31.110` toolkit-js variant (compact-js 2.5.3).** A new
`util/toolkit-js/compact-0.31.110/` workspace pins `@midnight-ntwrk/compact-js*` 2.5.3,
selected when `COMPACTC_VERSION` resolves to `0.31.110`. Because compact-js 2.5.3 and its
matching `compact-runtime` are unpublished, the variant consumes five `npm pack` tarballs via
`file:` references (`compact-js{,-command,-node}`, `compact-runtime`, `ledger-v9`). The blobs
are built in CI from the pinned `midnight-sdk` submodule by the new `+compact-js-bundle`
Earthly target (`scripts/build-compact-js-bundle.sh`) and committed to the PR branch by the
`rebuild-compact-js-bundle-bot` workflow (`/bot rebuild-compact-js-bundle`). The consume path
(`npm ci` in `util/toolkit-js`) needs no registry token — everything else resolves from public npm.

**Dispatch on the full patch version.** toolkit-js now selects its variant on the full
`<major>.<minor>.<patch>` compactc version instead of truncating to `<major>.<minor>`, since a
compactc patch can ship a contract format that expects a different `compact-js` patch. The
existing variant workspaces are renamed accordingly (`compact-0.29` → `compact-0.29.0`,
`compact-0.30` → `compact-0.30.0`, `compact-0.31` → `compact-0.31.0`). `resolveCompactcVersion`
keeps the leading `<major>.<minor>.<patch>` and drops any trailing build/tree-hash suffix (e.g.
`0.31.0-6587676a9bb2`); an unsupported patch now errors loudly instead of silently mapping to
the minor line.

**Fetch compactc dev builds by commit SHA.** `compact` can publish public releases from
arbitrary commits ("dev builds"), tagged `compactc-dev-<40-char-sha>` with assets named
`compactc_dev-<sha>_<arch>-unknown-linux-musl.zip`. `+compactc-fetch` now picks the tag/asset by
inspecting the `COMPACTC_VERSION` suffix: a bare 40-char hex SHA selects the dev-build naming,
anything else keeps the conventional `compactc-v<version>` naming, and the
`<version>-<12-char-tree-hash>` submodule form still routes to build-from-source.
`COMPACTC_VERSION` is pinned to the `0.31.110` dev build
(`3a289c2e7811d2868e7810bd5a5f1f0b7055995f`); the `compact/` submodule stays in place, but its
tree hash differs from this value so CI and `just compactc` fetch the prebuilt binary.

**Re-enabled ledger-9 toolkit-js tests and e2e CI jobs.** Now that the Rust ledger is on
`crate-ledger-9.1.0.0-rc.2` (intent[v8]), matching the compact-js 2.5.3 output, the
`LEDGER9-TOOLKIT-JS` gates are reverted: the `#[ignore]`s on the `commands::generate_intent`
tests and the `bboard_private_witness_not_leaked` e2e test are dropped, the Earthfile
`GENERATE_JS_TEST_TXS` gate is removed, and the `toolkit-maintenance` / `mint` / `tokens-minter`
/ `contracts` e2e CI jobs are re-enabled. The checked-in counter fixtures are regenerated for
rc.2 (`contract_state.mn` v6→v8, `deploy.bin` intent v6→v8, `deploy_tx.mn` transaction v9→v11,
`contract_address.mn` re-derived). No `LEDGER9-TOOLKIT-JS` markers remain.

PR: https://github.com/midnightntwrk/midnight-node/pull/1711
Issue: https://github.com/midnightntwrk/midnight-node/issues/1624
