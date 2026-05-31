#node

# Speed up cNight db-sync observation queries

Pre-query coarse `tx` / `tx_out` / `ma_tx_out` id bounds for the requested
block-range window, then constrain the four cNight observation queries
(registration, deregistration, asset create, asset spend) by primary-key
range so postgres can prune rows before doing expensive joins. Extends the
same `tx.id` bounding to the `tx_in`-keyed spend/deregistration queries.

PR: https://github.com/midnightntwrk/midnight-node/pull/1365
