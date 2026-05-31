#toolkit
# Add file-based wallet and ledger state caching to toolkit

Introduces a two-tier file cache that persists ledger snapshots and per-wallet state across toolkit runs, eliminating the need to replay the full chain on every invocation. Ledger snapshots (postcard encoding, zstd-compressed, ~1.4 compression) are stored once per block height and shared across wallets; per-wallet state (postcard encoded) is keyed by seed hash. Write to `.tmp`, then atomic rename pattern prevents data corruption on concurrent writes on POSIX.

Includes a trusted deserialization path that computes hashes in a single bottom-up pass for self-generated cache data, bypassing the two-pass security verification and cutting deserialization time by half. Similarly, fast serialization calls `serialize_to_node_list()` once instead of twice cutting serialization time by half.

Stale snapshot garbage collection reads only the first 8 bytes of wallet files headers to extract block height without full deserialization.

New CLI flags: `--ledger-state-db <path>` to set the cache directory (default: `ledger_state_db`), and `--fetch-only-cached` for offline operation from a pre-populated cache.

PR:
- https://github.com/midnightntwrk/midnight-node/pull/820
- https://github.com/midnightntwrk/midnight-node/pull/939
JIRA: https://shielded.atlassian.net/browse/PM-22103
