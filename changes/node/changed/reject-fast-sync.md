#node #sync

# Refuse to start with `--sync fast` or `--sync fast-unsafe`

These modes import blocks without executing them, which leaves the ledger and consensus state
incomplete. The node now exits at start-up with an error asking for another sync mode.

PR: https://github.com/midnightntwrk/midnight-node/pull/2517
