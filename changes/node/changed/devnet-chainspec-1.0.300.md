#node #chainspec
# Rebuild devnet chainspec on the 1.0.300 runtime

The devnet chainspec still embedded the 0.22.0 runtime (`spec_version` 22000), so a devnet
started from it reported 22000 regardless of the node binary. Rebuilt it against 1.0.300.

Genesis runtime is now `spec_version` 1000300; `Throttle` starts at storage version 1.
The ledger genesis state (`res/genesis/genesis_state_devnet.mn`) is unchanged.

New devnet genesis hash: `0xe50ea0ab2997fab2ea6fce3c198f4ff34e6c4f3e7747c133746053a0e82b60e5`
