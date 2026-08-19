#toolkit #refactor
# Rename `StandardTrasactionInfo` to `StandardTransactionInfo`

Fixes a typo in the name of the `StandardTrasactionInfo` struct in
`ledger/helpers` and all of its usages in the toolkit transaction builders.
A deprecated type alias with the old spelling is kept so existing callers
continue to build. No behaviour change.

PR: https://github.com/midnightntwrk/midnight-node/pull/2016
