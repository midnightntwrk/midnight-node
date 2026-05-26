#toolkit
# Add option when generating intents to write out the contract on-chain state

Adds support for multiple Ledger stacks to `toolkit-js`.

When called from the command-line, `toolkit-js` will default to the latest ledger version (and consequently the associated version of Compact.js that supports it), but this can be overridden by applying the `LEDGER_VERSION=d` environment variable. For example, to use a Compact.js that is built against Ledger 7, set `LEDGER_VERSION=7` in the environment.

PR: https://github.com/midnightntwrk/midnight-node/pull/946
Ticket: https://shielded.atlassian.net/browse/PM-22230
