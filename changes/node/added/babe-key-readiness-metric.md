#node #metrics

# Report whether a validator holds a BABE key registered on Cardano

Validators now publish `midnight_babe_key_registered`: `1` when the keystore
holds a BABE key that is also registered as the `babe` key of a permissioned
candidate on Cardano, `0` when it holds none. A probe intersects the keystore's
BABE keys with the `babe` keys of the permissioned candidates read from the main
chain, and a reporter runs it periodically.

Ahead of the switch to BABE consensus this makes a missing or wrong BABE key
visible is grafana.

The probe deliberately uses the plain node keystore, not
`AuraToBabeMigrationKeystore`, which answers BABE queries with AURA keys and
would report every node as ready.

We need reliable data about how good operators did in adding keys to keystores
to not lose chain density (hence link to https://github.com/midnightntwrk/midnight-node/issues/1753)

PR: https://github.com/midnightntwrk/midnight-node/pull/1996
Issue: https://github.com/midnightntwrk/midnight-node/issues/1753
