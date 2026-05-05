#toolkit #ledger
# Improve wallet seed, key pair, and address code quality

- Redact `Debug` for `WalletSeed`, add `Zeroize` / `ZeroizeOnDrop`, and remove implicit `Copy` for secret material
- Remove `Clone` from `Keypair` where unused; harden lazy-hex parsing and `add_addresses` iteration
- Update call sites and tests for explicit `Clone` where seeds are duplicated intentionally

PR: https://github.com/midnightntwrk/midnight-node/pull/1217
Issue: https://github.com/midnight-security/midnight-security/issues/112
Ticket: https://shielded.atlassian.net/browse/PM-22038
