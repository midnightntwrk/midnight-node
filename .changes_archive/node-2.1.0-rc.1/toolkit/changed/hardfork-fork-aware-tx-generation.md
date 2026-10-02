#toolkit #ledger9

# Fork-aware transaction generation across the ledger 8->9 hardfork

`replay_blocks` now detects the v8->v9 fork boundary and runs the same
`StateTranslationTable` translation used by the on-chain migration
(`fork_context_8_to_9` / `fork_8_to_9_if_needed`), so transactions generated
after the fork are built against the correctly-translated ledger-9 context
instead of a stale ledger-8 one.

The `runtime-upgrade` command now waits for the new runtime to actually
*execute* at a finalized block, not just be applied/stored. The stored spec
version flips at the apply block, but that block still executes under the
old runtime, so polling only the stored spec left transaction generation
reading a block short of any ledger-9-classified block.

PR: https://github.com/midnightntwrk/midnight-node/pull/1925
Issue: https://github.com/midnightntwrk/midnight-node/issues/1580
