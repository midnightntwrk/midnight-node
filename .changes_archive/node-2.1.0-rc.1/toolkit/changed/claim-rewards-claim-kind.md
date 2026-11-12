#toolkit

# `generate-txs claim-rewards` supports `--claim-kind`

Add a `--claim-kind` selector to `toolkit generate-txs claim-rewards` so users
can build `ClaimRewardsTransaction`s for both ledger `ClaimKind` variants:
`reward` (block-production rewards, the historical default) and
`cardano-bridge` (mNIGHT bridged from Cardano via the c2m protocol bridge).
Previously the claim kind was hardcoded to `Reward`, leaving the user-facing claim
half of the c2m-bridge flow unreachable through the toolkit.

The flag defaults to `reward`, so existing behaviour is unchanged when it is
omitted. The selector is plumbed through `ClaimRewardsArgs` (a clap `ValueEnum`
adapter `ClaimKindArg`), `ClaimRewardsBuilder` (which maps it onto the active
ledger version's `ClaimKind`), and the `ClaimMintInfo::set_claim_kind` helper,
which now stamps the chosen kind onto the built transaction for all ledger
versions (v7/v8/v9).

Issue: https://github.com/midnightntwrk/midnight-node/issues/1678
PR: https://github.com/midnightntwrk/midnight-node/pull/1697
