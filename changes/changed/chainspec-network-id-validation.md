#node #binary
# Add validation for `networkId` on node boot to avoid mismatch with genesis state

Adds validation to ensure the `networkId` set in the chainspec matches the
`networkId` used to generate the genesis state.

PR:
Fix for: https://shielded.atlassian.net/browse/PM-22422
