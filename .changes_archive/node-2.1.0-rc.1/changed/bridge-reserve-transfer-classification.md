#node #runtime

# Fixes Reserve Transfer classification

Before this change Bridge was using transaction metadata as criterium of classifying transfers.
It would allow attack on M.R pool.
This PR modifies observability to distinguish Reserve Validator and ICS Validator inputs of the transaction to correctly classify transfer.
In the edge case one Cardano Tx can be ReserveTransfer and User Transfer at same time.

When node with this update is deployed, it will not be able to read MainChainScripts of the bridge pallet and inherent data provider will report `Inert` variant of IDP.
Bridge is not complete and non environment should have it configured, so everywhere the IDP should be `Inert` as well.
Governance action setting these addresses and data checkpoint will be required to enable the bridge.

PR: https://github.com/midnightntwrk/midnight-node/pull/1513
Required for https://github.com/midnightntwrk/midnight-node/issues/1479
