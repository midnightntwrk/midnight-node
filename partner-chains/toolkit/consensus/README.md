# Partner Chains Consensus

This crate provides the consensus components used by Partner Chains nodes to attach
Partner Chains inherent data to block headers and validate it during block import,
without forking the block production gadget. Items provided:

* `InherentDigest` — re-exported from `sp-partner-chains-consensus`; maps inherent data to
  block header digest items and back
* `PartnerChainsProposerFactory` — wraps a `Proposer` so that each produced block header
  contains the `InherentDigest` items

The complete Partner Chains inherent check — recreating inherent data from Partner Chains
data sources and checking the block's inherents against it — is run at whichever point the
consensus gadget checks inherents. Two wrappers cover the two cases:

* `PartnerChainsBlockImport` + `PartnerChainsBodyRestore` — for stacks that check inherents
  during block import (e.g. `BabeBlockImport`). The outer wrapper runs the Partner Chains
  inherent check and withholds the body from the consensus import (so the consensus import's
  own inherent check is skipped); the restore stage placed directly beneath it puts the body
  back for GRANDPA and the client.
* `PartnerChainsVerifier` — for stacks that check inherents in the import-queue verifier
  instead (e.g. the Aura verifier). It wraps that verifier and runs the same Partner Chains
  inherent check. The inner verifier still performs its header-level consensus checks (seal
  signature, slot author, equivocation), but its own inherent check is skipped in favour of
  the complete Partner Chains one.
* `SlotExtractor` — implemented by the node for its block production gadget (e.g. via
  `sc_consensus_aura::find_pre_digest` for Aura), keeping this crate consensus-agnostic

Both wrappers rely on the wrapped component checking inherents *only* when the body is
present — an implicit contract with the consensus implementation, not something the
`Verifier`/`BlockImport` traits guarantee. It is pinned by contract canaries against the
real consensus components: `src/aura_verifier_contract.rs` (the `sc_consensus_aura`
verifier) and `src/babe_block_import_contract.rs` (the `sc_consensus_babe` block import);
re-check the contract when upgrading polkadot-sdk or wrapping a different consensus stack.

See `service.rs` in the `node` crate for usage.

License: GPL-3.0-or-later WITH Classpath-exception-2.0
