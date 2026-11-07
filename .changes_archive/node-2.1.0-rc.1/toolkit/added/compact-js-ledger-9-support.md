#toolkit #compactc #ledger9
# Add compact 0.33.0-rc.1 support (Ledger 9 compatible)

Adds the compact 0.33.0-rc.1 toolchain to toolkit-js so it can generate Ledger 9
contract transactions, and re-enables the ledger-9 tests/CI that had been gated.

- New `compact-0.33.0` toolkit-js variant pinning `@midnight-ntwrk/compact-js*`
  2.5.5-rc.6 and `compact-runtime` 0.18.0-rc.1 from public npm — no registry token
  needed.
- toolkit-js now selects its variant on the full `<major>.<minor>.<patch>` compactc
  version instead of `<major>.<minor>`, since a compactc patch can change the
  contract format.
- `+compactc-fetch` can now fetch dev builds by commit SHA (`compactc-dev-<sha>`) as
  well as tagged releases; `COMPACTC_VERSION` is pinned to `0.33.0-rc.1`.
- Re-enabled the `LEDGER9-TOOLKIT-JS`-gated tests and e2e CI jobs
  (`toolkit-maintenance`, `mint`, `tokens-minter`, `contracts`), and regenerated the
  checked-in counter test fixtures.

PR: https://github.com/midnightntwrk/midnight-node/pull/1711
Issue: https://github.com/midnightntwrk/midnight-node/issues/1624
