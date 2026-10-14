#security #ledger
# Key transaction validation caches with Blake2-256

The strict and soft transaction validation caches, and the transaction pool
`provides` tag, are now keyed by `Blake2_256(runtime_version || tx_bytes)`
instead of `Twox128`. Twox128 is not collision resistant, so two different
transactions could share a cache entry. Node-only change; the pool tag is
local and not part of consensus.

PR: https://github.com/shieldedtech/shadow-midnight-node/pull/65
Issue: https://github.com/shieldedtech/shielded-security-engineering/issues/731
