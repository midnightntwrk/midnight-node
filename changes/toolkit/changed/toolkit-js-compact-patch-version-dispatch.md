#toolkit #compactc
# Dispatch toolkit-js compact variants on the full patch version

toolkit-js now selects its `@midnight-ntwrk/compact-js*` variant on the full
`<major>.<minor>.<patch>` compactc version instead of truncating to
`<major>.<minor>`. A compactc patch can ship a contract format that expects a
different `compact-js` patch, so the previous "compact-js is patch-stable within a
minor line" assumption no longer holds.

The variant workspaces are renamed accordingly:
- `compact-0.29/` → `compact-0.29.0/` (compact-js 2.4.3)
- `compact-0.30/` → `compact-0.30.0/` (compact-js 2.5.0)
- `compact-0.31/` → `compact-0.31.0/` (compact-js 2.5.1)

`resolveCompactcVersion` now keeps the leading `<major>.<minor>.<patch>` and drops
any trailing build/tree-hash suffix (e.g. `0.31.0-6587676a9bb2`, the form the dev
shell exports from the root `COMPACTC_VERSION` file). An unsupported patch such as
`0.31.1` now errors loudly rather than silently mapping to the minor line.

`SUPPORTED_COMPACTC_VERSIONS`, the root `package.json` dependencies, each variant's
package name, the regenerated `package-lock.json`, the `test-all-compactc.sh`
comments, and the README maintenance docs all use the full-patch naming. The
`compact-*` workspace glob and `node-toolkit-compact-*` build filter pick the
renamed workspaces up automatically.

PR:
Issue:
