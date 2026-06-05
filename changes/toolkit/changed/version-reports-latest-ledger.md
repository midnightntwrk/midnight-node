#toolkit
# `version` subcommand reports the latest supported ledger version

The toolkit `version` subcommand previously looked up the `mn-ledger`
workspace dependency, which is the *oldest* compatible ledger crate, so it
printed `Ledger: =7.0.3` even though the toolkit supports newer ledgers. It now
reports the ledger generation of the `latest` ledger module (currently
`Ledger: 9`). A `LEDGER_VERSION` constant was added to each ledger module in
`midnight-node-ledger-helpers` so the value tracks the `latest` alias and stays
correct as new ledger generations are added.

PR: <link to PR>
Issue: https://github.com/midnightntwrk/midnight-node/issues/1641
