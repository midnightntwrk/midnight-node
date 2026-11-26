#node #sync

# Support only full sync

The node now exits at start-up unless it runs with `--sync full`, the default. `--sync fast`
and `--sync fast-unsafe` import blocks without executing them, which leaves the ledger and
consensus state incomplete. `--sync warp` is disabled until warp sync is stable together with
GRANDPA.

PR: https://github.com/midnightntwrk/midnight-node/pull/2517
