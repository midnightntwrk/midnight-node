# Impl Hard Fork version in midnight-node-ledger-helpers

In order to reuse components from `midnight-node-ledger-helpers` in `midnight-node-ledger` and `midnight-node-toolkit`, we followed the same approach as in `midnight-node-ledger`, using module parameterization to support both hard-fork and non-hard-fork ledger dependencies.

Additionally, a disabled-by-default `can-panic` feature flag was added in `midnight-node-ledger-helpers` to prevent `midnight-node-ledger` from importing methods that can panic. These methods are acceptable for the Toolkit though, which imports the crate with that feature enabled.

PR: https://github.com/midnightntwrk/midnight-node/pull/128
Ticket: https://shielded.atlassian.net/browse/PM-19985
