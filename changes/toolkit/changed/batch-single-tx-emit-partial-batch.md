#toolkit
# batch-single-tx can emit successful txs on partial failure (opt-in)

`generate-txs batch-single-tx` gains an `--emit-partial-batch` flag (off by
default). With the flag, transfers that built successfully are emitted (exit 0)
even when some transfers fail; only a batch where every transfer fails errors
out. Without the flag, the previous all-or-nothing behavior is kept: any failed
transfer fails the whole batch.

Transfers touching the same wallet are now serialized via per-seed locks, and a
failed transfer's wallet mutations (spent coins, pending change, DUST) are
rolled back, so emitted txs never depend on state from a failed, never-submitted
tx.

PR: https://github.com/midnightntwrk/midnight-node/pull/1731
