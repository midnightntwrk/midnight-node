#toolkit #fix
# Keep the v8->v9 translated ledger state alive after the hardfork replay

Replaying a chain across the ledger 8->9 hardfork (e.g. Midnight Preview) panicked a few blocks
after the fork with "root should be in the arena". The translated state returned by
`TypedTranslationState::result()` is lazy, and its nodes were pinned only by the translation state,
which `fork_context_8_to_9` dropped straight away. The in-memory arena then evicted them. The
toolkit now persists the result before the translation state drops, as the node's own migration does.

Verified: cold replay of Preview (77,069 blocks) to the tip.

PR: TBD
Issue: https://github.com/midnightntwrk/midnight-node/issues/2422
