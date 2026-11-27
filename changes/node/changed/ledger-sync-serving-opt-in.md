#node #sync

# Serve ledger snapshots only when opted in

Nodes no longer serve Midnight ledger snapshots to warp-syncing peers by default. Validators and
non-validators alike serve only with `--serve-warp-ledger-sync`.

PR: https://github.com/midnightntwrk/midnight-node/pull/2517
