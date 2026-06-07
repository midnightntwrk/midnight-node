#toolkit
# `version` subcommand reports the latest supported ledger version

The toolkit `version` subcommand previously looked up the `mn-ledger`
workspace dependency, which is the *oldest* compatible ledger crate, so it
printed `Ledger: =7.0.3` even though the toolkit supports newer ledgers. It now
looks up the semver of the *latest* ledger crate instead. A `CRATE_NAME`
constant was added to each ledger module in `midnight-node-ledger-helpers`, and
the command resolves the version via `find_dependency_version(latest::CRATE_NAME)`
so the reported version tracks the `latest` alias and stays correct as new
ledger generations are added. (The latest crate cannot be selected by comparing
semver, since the ledger-9 crate is independently versioned below ledger-8.)

PR: https://github.com/midnightntwrk/midnight-node/pull/1649
Issue: https://github.com/midnightntwrk/midnight-node/issues/1641
