#node #partner-chains

# Dev chain mock candidates carry their BABE and BEEFY keys

The mock candidate data source used by `CFG_PRESET=dev` passed only the AURA and GRANDPA keys of
permissioned candidates and registrations. The runtime's session keys also hold BABE and BEEFY keys,
so every mock candidate was filtered out, committee selection failed on every epoch, and a dev chain
kept its genesis committee forever. The mock now requires the `babe_pub_key` and `beefy_pub_key`
fields, which the dev and qanet registrations files already set. `testnet-02-mock.json` gets them
by the convention those files follow: the BABE key is the AURA key, the BEEFY key is the sidechain
key.

The mock's data epoch now uses `offset_data_epoch`, as the db-sync source does: for a Cardano epoch
below 2 it returns an error instead of wrapping around, so the node reports why it cannot provide
committee selection data.

PR: https://github.com/midnightntwrk/midnight-node/pull/2511
