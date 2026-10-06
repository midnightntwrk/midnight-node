#node #consensus

# Check block inherents in the on-chain call context

The Partner Chains verifier and block import now call the runtime's `check_inherents`
API in the on-chain call context, the one the block builder uses, instead of the default
off-chain context of a plain runtime API call.

The context decides which runtime executes the call around a runtime upgrade. Since
`system_version` 3, an upgrade is staged in `:pending_code` and replaces `:code` only at
the end of the block *after* the upgrade block. On-chain calls resolve the staged
runtime; off-chain calls run the old one. The first post-upgrade block was therefore
built with the new runtime but inherent-checked with the old one, and any inherent whose
encoding changed across the upgrade (e.g. `sessionCommitteeManagement.set` when the
session keys gain a key) made the old runtime panic with
`check_extrinsics(): Unable to decode extrinsic`. Every node but the author rejected the
block, each authority then authored its own first post-upgrade block, and the chain split
into one fork per authority with finality stuck at the upgrade block.

Reproduced on local-env by timing `applyAuthorizedUpgrade` to land one block before a
committee `set` block (one in five blocks there, one in `SLOTS_PER_EPOCH` on real
networks). The runtime half of the fix, re-running pending migrations inside
`check_inherents`, is in the runtime change file of the same name.

PR:
Issue:
