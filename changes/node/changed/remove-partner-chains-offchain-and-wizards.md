#node

# Remove Partner Chains smart-contracts offchain and wizards

Midnight does not use the Partner Chains Cardano smart contracts, so the code that built
transactions against them is removed:

- the `partner-chains-cardano-offchain` crate and the `raw-scripts`
  (`partner-chains-smart-contracts`) git dependency;
- the `partner-chains-cli` crate, and with it every `midnight-node wizards` subcommand:
  `generate-keys`, `prepare-configuration`, `create-chain-spec`, `setup-main-chain-state`,
  `start-node`, `register1`, `register2`, `register3` and `deregister`.

The datum types in `partner-chains-plutus-data` stay, since the main chain follower still decodes
candidate registrations, D-parameter and permissioned candidates from Cardano. The other Partner
Chains subcommands (`sidechain-params`, `registration-status`, `ariadne-parameters`,
`registration-signatures`) are unchanged.

The unused local-environment wizard scripts and the wizard user guides are removed as well.

PR: https://github.com/midnightntwrk/midnight-node/pull/2446
Issue: https://github.com/midnightntwrk/midnight-node/issues/1524
