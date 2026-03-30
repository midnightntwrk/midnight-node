#toolkit
# Add option when generating intents to write out the contract on-chain state

Adds a new optional `--output-onchain-state` option to the `generate-intent` command, that when supplied, will write out the contract's on-chain (public) state to the specified file.

This file can then be supplied on subsequent usages of `generate-intent` via the `--input-onchain-state` option to _chain_ contract calls together, ensuring that changes in state are preserved through each of the contract invocations.

PR: https://github.com/midnightntwrk/midnight-node/pull/812
Ticket: https://shielded.atlassian.net/browse/PM-21901
