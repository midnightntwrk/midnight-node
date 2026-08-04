#toolkit
# `version` subcommand reports the latest supported ledger version

The toolkit `version` subcommand previously looked up the `mn-ledger`
workspace dependency, which is the *oldest* compatible ledger crate, so it
printed `Ledger: =7.0.3` even though the toolkit supports newer ledgers. It now
reports both the ledger generation and the crate semver of the *latest* ledger,
e.g. `Ledger: 9 (=0.1.0)`. `LEDGER_VERSION` and `CRATE_NAME` constants were
added to each ledger module in `midnight-node-ledger-helpers`; the command reads
the generation from `latest::LEDGER_VERSION` and resolves the semver via
`find_dependency_version(latest::CRATE_NAME)`, so both values track the `latest`
alias and stay correct as new ledger generations are added. (The latest crate
cannot be selected by comparing semver, since the ledger-9 crate is
independently versioned below ledger-8.)

PR: https://github.com/midnightntwrk/midnight-node/pull/1649
Issue: https://github.com/midnightntwrk/midnight-node/issues/1641
