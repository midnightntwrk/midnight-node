#toolkit
# Enforce derivation path role validation in wallet constructors

Add regression tests verifying that `DustWallet::from_path()` and
`ShieldedWallet::from_path()` reject derivation paths with mismatched
roles. Addresses Least Authority audit Issue AN.

Issue: https://github.com/midnightntwrk/midnight-node/issues/1327
PR: https://github.com/midnightntwrk/midnight-node/pull/1076
JIRA: https://shielded.atlassian.net/browse/PM-20015
