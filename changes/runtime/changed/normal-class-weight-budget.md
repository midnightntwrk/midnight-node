#runtime #weights
# Scale transaction cost against the Normal dispatch class budget

`get_transaction_cost` mapped a transaction's ledger cost onto Substrate weight
against `max_block`, but `send_mn_transaction` is a `Normal`-class extrinsic,
capped at `NORMAL_DISPATCH_RATIO` (75%) of the block. A transaction above 75% of
any ledger dimension therefore got a weight the `Normal` class could never fit,
so Substrate rejected it even though the ledger considered it valid. Cost is now
scaled against the `Normal` class `max_total` (falling back to `max_block`).

PR: https://github.com/midnightntwrk/midnight-node/pull/1540
