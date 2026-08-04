#toolkit
# Dispatch `toolkit-js` variants by `compactc` version instead of ledger version

The `toolkit-js` workspace previously named its sibling packages by ledger
version (`v7/`, `v8/`) and dispatched on `LEDGER_VERSION`. This was incorrect:
the axis that actually varies between variants is the `@midnight-ntwrk/compact-js`
line, which tracks the `compactc` compiler version — and multiple `compactc`
versions can target the same ledger version (e.g. compactc `0.30.x` and `0.31.x`
both target ledger 8), which the old scheme could not represent.

Renamed the workspaces and switched the dispatcher accordingly:

- `v7/` → `compact-0.29/` (compact-js `2.4.3`)
- `v8/` → `compact-0.30/` (compact-js `2.5.0`)
- Added `compact-0.31/` (compact-js `2.5.1`)
- Env var: `COMPACTC_VERSION` (default `0.31`), replaces `LEDGER_VERSION`.
  Accepts either `<major>.<minor>` or the full `<major>.<minor>.<patch>` form
  shared with the rest of the toolchain.

Updated the toolkit-js tests to support executing against every supported
compactc version, and added a maintenance guide to the toolkit-js README.

PR: https://github.com/midnightntwrk/midnight-node/pull/1555
