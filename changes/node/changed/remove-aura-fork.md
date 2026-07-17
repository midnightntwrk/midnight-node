#node #partner-chains
# Replace the forked consensus crates with consensus-agnostic wrappers

Remove `sc-partner-chains-consensus-aura` and `sp-partner-chains-consensus-aura`.
Add consensus-agnostic `sc-partner-chains-consensus`, which runs the full Partner Chains
inherent check at whichever point the consensus gadget checks inherents:
`PartnerChainsBlockImport` (with `PartnerChainsBodyRestore`) wraps a consensus block import
for stacks that check inherents at import (e.g. `BabeBlockImport`), and
`PartnerChainsVerifier` wraps the import-queue verifier for stacks that check inherents
there instead (e.g. Aura). `PartnerChainsProposerFactory` injects the `mcsh` digest at
proposal time.

PR: https://github.com/midnightntwrk/midnight-node/pull/1700
Issue: https://github.com/midnightntwrk/midnight-node/issues/1859
