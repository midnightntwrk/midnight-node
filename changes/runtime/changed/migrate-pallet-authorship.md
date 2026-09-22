#runtime
# Migrate block author tracking to upstream `pallet_authorship`

Replaces the local `runtime/src/authorship` helper module with upstream
`pallet_authorship` from polkadot-sdk. The pallet is wired at index 9,
declared after `SessionCommitteeManagement` and before `Session`, matching
polkadot-sdk hook-order requirements.

`FindAuthor` uses `pallet_session::FindAccountFromAuthorIndex<Self, Aura>` so
the block author is resolved from the AURA digest and mapped to the current
session validator account.

Requires a metadata rebuild.

PR: https://github.com/midnightntwrk/midnight-node/pull/1876
Issue: https://github.com/midnightntwrk/midnight-node/issues/1875
