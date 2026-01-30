#toolkit
# Add treasury initialization to genesis generation

Implements treasury initialization for the Midnight mainnet genesis block. The treasury is initialized via a one-off transfer of Night tokens to the ICS contract, configured through a new `cnight-treasury-config.json` file. The genesis creation tool validates UTxO configurations and computes the correct Night amount for treasury initialization.

PR: https://github.com/midnightntwrk/midnight-node/pull/563
JIRA: https://shielded.atlassian.net/browse/PM-20981
