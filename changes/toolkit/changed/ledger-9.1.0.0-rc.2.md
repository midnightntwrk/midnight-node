#toolkit
# Adapt toolkit to ledger 9.1.0.0-rc.2 crypto-stack split

Ledger 9 now uses coin-structure/transient-crypto 3.x while ledgers 7/8 stay
on 2.x, so the toolkit's `Encoded*` zswap conversions are split into separate
2.x and 3.x impl sets, and contract-address/type conversions go through
per-version `type_convert` helpers.

PR: https://github.com/midnightntwrk/midnight-node/pull/1692
