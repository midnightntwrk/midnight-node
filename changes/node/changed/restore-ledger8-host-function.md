#node #ledger
# Restore the ledger_8 `construct_distribute_treasury_system_tx` host function

`ledger_7` and `ledger_8` are frozen host interfaces: runtimes already deployed on
live networks import their symbols and will do so forever. PR #1604 ("Ledger9 with
no migration") removed `construct_distribute_treasury_system_tx` from the
`Ledger8Bridge` interface, which silently made the ledger_9 node binary unable to
instantiate any deployed ledger_8 runtime (it still imports
`ext_ledger_8_bridge_construct_distribute_treasury_system_tx_version_1`), blocking
the ledger_8 -> ledger_9 upgrade.

This restores the host function byte-identically to its pre-#1604 implementation so
the same symbol and system transaction are regenerated, and adds a golden test that
fails loudly if the frozen ledger_8 method set ever changes again.

PR: https://github.com/midnightntwrk/midnight-node/pull/1651
