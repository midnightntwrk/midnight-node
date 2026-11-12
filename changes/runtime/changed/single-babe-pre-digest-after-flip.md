#runtime #consensus #babe #migration

# Check the BABE pre-runtime digest layout after the flip

In state `Babe`, `pallet-consensus-engine` requires exactly one BABE pre-runtime digest that
decodes exactly, mirroring the layout check it applies before the flip. Tools that build blocks
without consensus digests need a BABE pre-runtime digest after the flip. No storage, call or runtime API changes; metadata is unchanged.

PR: https://github.com/midnightntwrk/midnight-node/pull/2407
