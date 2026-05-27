#ledger #helpers
# Eliminate deadlock in `LedgerContext::with_wallets_from_seeds` (R-059)

`LedgerContext::with_wallets_from_seeds` previously acquired the `wallets`
mutex twice in sequence — once for the origin wallet and again for the
destination wallet — which deadlocks because `std::sync::Mutex` is not
reentrant. The function now acquires the lock exactly once and uses
`HashMap::get_disjoint_mut` to obtain two disjoint `&mut Wallet<D>`
references from a single `MutexGuard`, mirroring the locking shape of the
sibling `with_wallet_from_seed`. The same-seed-twice case is rejected
up front with a clear panic, and a missing seed panics with a message
matching the existing `wallet_for_seed` style. Three regression tests
in `ledger/helpers/src/versions/common/context.rs` pin the fix
(non-blocking completion, aliased-seed panic, missing-seed panic).

PR: https://github.com/midnightntwrk/midnight-node/pull/1471
JIRA: https://shielded.atlassian.net/browse/PM-21800
Issue: https://github.com/shieldedtech/shielded-security-engineering/issues/114
