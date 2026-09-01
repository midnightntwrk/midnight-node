#toolkit
# Fix `--ledger-state-db` entries being unreadable — and silently deleted — when the cache directory is shared between users

Both writers in the file cache backend staged through
`NamedTempFile::new_in`, which hard-codes mode 0600, and `persist()` uses
`rename(2)` — so that mode landed on the finished cache file. A umask can
only clear bits, never add them, so no umask setting could widen it.

A `--ledger-state-db` shared between users is the normal case on a perf
box (CI jobs run as one user, interactive sessions as another, both in a
common group over a setgid directory). Every user but the writer therefore
saw each seed as a cache miss, and one missing entry forces
`build_fork_aware_context_cached` to replay from genesis for the whole
seed set — ~90 minutes on a 31k-block chain.

Two further defects turned that from "slow" into "destructive":

- `get_all_cached_wallet_heights` exists to collect snapshot GC
  references, but it scanned the *entire* wallets directory and deleted
  any file whose 9-byte header it could not parse — and its parser could
  not tell "unreadable by this user" from "corrupt", because
  `read_wallet_height` was `.ok()?` throughout. Since that scan runs on
  every cache save by every command, two users sharing a directory
  destroyed each other's entries on every save, and so did two toolkit
  builds with different `WALLET_CACHE_FORMAT_VERSION` values.
- `get_wallet_states` turned any io error into an unlogged cache miss, so
  the resulting hour-and-a-half replay came with nothing in the log to
  explain it.

Changes:

- New `staging_file` helper used by both writers: requests 0666 and lets
  the process umask narrow it (0002 → 0664, the usual 0022 → 0644, 0077 →
  0600 for a deliberately private cache). Directories are already left to
  the umask via `fs::create_dir_all`, so a shared deployment now sets one
  umask and both halves follow. `#[cfg(unix)]`-gated.
- `read_wallet_height` returns `io::Result<Option<u64>>`: `Err` for
  "could not read", `Ok(None)` for "not this format version".
- The GC scan is read-only. An entry it cannot account for is simply not
  counted as a snapshot reference — an older build's entries are left for
  that build, and a failed read never costs anyone their cache. Eviction
  of genuinely corrupt entries stays in `get_wallet_states`, where it is
  scoped to the seeds the caller asked for and where the format-versioned
  cache key means an undecodable body really is corruption.
- `write_wallet_if_newer` logs, then replaces, an existing entry whose
  height it cannot read — which is how a directory full of 0600 entries
  heals.
- `get_wallet_states` logs an unreadable entry instead of silently
  reporting a miss.

Tests pin the file mode against the process umask rather than a literal
(so the intent survives any CI umask), and cover the sweep leaving both
unreadable and foreign-format entries alone, an unreadable entry
recovering once it is readable again, and permissions healing on replace.

PR: https://github.com/midnightntwrk/midnight-node/pull/PENDING
