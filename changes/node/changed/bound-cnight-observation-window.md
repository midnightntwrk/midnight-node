#node #cnight-observation #performance
# Bound cNight observation queries by `CardanoBlockWindowSize`

The cNight observation IDP previously queried cardano-db-sync from `NextCardanoPosition` all the way to the current Cardano tip on every Midnight block. For sparse assets like cNight, this resulted in multi-minute db-sync queries scanning effectively the full Cardano history, blocking block import to ~0.1 bps.

The `CardanoBlockWindowSize` storage value already existed and was already exposed via `CNightObservationApi::get_cardano_block_window_size`, but was unused by the IDP. This change reads the window size in the IDP, threads it through `MidnightCNightObservationDataSource::get_utxos_up_to_capacity`, and clamps the query upper bound to `start + window` in the data source. Each query now scans a bounded window (default 1000 Cardano blocks) instead of the full chain. The genesis-creation tool passes `u32::MAX` to preserve its existing full-history scan.

PR: https://github.com/midnightntwrk/midnight-node/pull/1432
