#node
# Tune autovacuum on db-sync hot tables for stable query plans

Lower `autovacuum_analyze_scale_factor` from the postgres default of 0.1 to 0.01 on the
cardano-db-sync tables midnight-node queries (`block`, `tx`, `tx_out`, `tx_in`, `ma_tx_out`,
`datum`). The default 10% growth threshold means autoanalyze never fires for big append-heavy
tables, leaving the planner on stale statistics and producing extreme worst-case plans
(observed >400s queries on otherwise idle preview/preprod DBs) for the cnight-observation
lookups. Applied alongside the existing index creation in `create_cnight_observation_indexes`,
idempotent.

PR: https://github.com/midnightntwrk/midnight-node/pull/1434
Issue: https://github.com/midnightntwrk/midnight-node/issues/1298
