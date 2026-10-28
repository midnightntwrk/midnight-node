#toolkit
# Add batched `dust_balance::execute_many` for multi-seed wallet cache warmup

New library entry point `dust_balance::execute_many(DustBalanceManyArgs)`
runs one shared block replay across `Vec<WalletSeed>` and returns per-seed
`DustBalanceResult`s in input order. Existing single-seed
`dust_balance::execute` is preserved and now delegates to `execute_many`.
The CLI surface is unchanged.

This lets callers populate the toolkit's file-based wallet cache for N
seeds with one expensive replay instead of N: `execute` always restarts
from genesis when its target seed is uncached, so N sequential
single-seed calls each pay a full chain replay. One batched call shares
that replay across all seeds. Useful for any operational workflow that
needs balances for a known set of seeds (validator wallets,
governance-member wallets, e2e test warmup), and load-bearing for the
upcoming Cardano Preview e2e observation tests which generate ~13 random
seeds per nightly run.

Also bumps the block-replay progress log emitted from
`replay_blocks_{7,8}` to info level at a throttled 30-second cadence
(`REPLAY_INFO_HEARTBEAT`). Detailed per-batch progress remains at debug;
the heartbeat is what users see by default during a multi-hour replay so
it doesn't look like the process has hung, matching the existing
fetch-progress info logging.

PR: https://github.com/midnightntwrk/midnight-node/pull/1603
Issue: https://github.com/midnightntwrk/midnight-node/issues/1568

