#toolkit
# Enforce derivation path role validation in wallet constructors

`DustWallet::from_path()` and `ShieldedWallet::from_path()` now validate
that the `DerivationPath` role matches the wallet type (`Role::Dust` and
`Role::Zswap` respectively). Mismatched roles are rejected with a
descriptive panic. Addresses Least Authority audit Issue AN.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1327
PR: https://github.com/midnightntwrk/midnight-node/pull/1076
JIRA: https://shielded.atlassian.net/browse/PM-20015
