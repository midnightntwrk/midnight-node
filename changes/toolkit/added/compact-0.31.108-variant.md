#toolkit
# Add `compact-0.31.108` toolkit-js variant (compact-js 2.5.3, for ledger 9)

Adds a new `util/toolkit-js/compact-0.31.108/` variant workspace pinning `@midnight-ntwrk/compact-js*`
2.5.3, selected when `COMPACTC_VERSION` resolves to `0.31.108`.

Because compact-js 2.5.3 and its matching `compact-runtime` are unpublished, the variant consumes five
`npm pack` tarballs via `file:` references (`compact-js{,-command,-node}`, `compact-runtime`, `ledger-v9`).
The blobs are built in CI from the pinned `midnight-sdk` submodule by the new `+compact-js-bundle` Earthly
target (`scripts/build-compact-js-bundle.sh`) and committed to the PR branch by the
`rebuild-compact-js-bundle-bot` workflow (`/bot rebuild-compact-js-bundle`). The consume path
(`npm ci` in `util/toolkit-js`) needs no registry token — everything else resolves from public npm.

PR: <link to PR>
