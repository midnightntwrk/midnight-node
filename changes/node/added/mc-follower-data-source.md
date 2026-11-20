#node
# Read Cardano data from mc-follower over gRPC

The main chain follower data sources can now read their Cardano data from an
mc-follower indexer over gRPC instead of from the cardano-db-sync Postgres
database. Set `mc_follower_grpc_endpoint` (eg. `http://localhost:50051`) to use
it; `db_sync_postgres_connection_string` is then not required. db-sync remains
the default, and the genesis commands still read db-sync.

The data sources run the same logic on either backend: they are now generic
over row source traits, implemented by the existing db-sync queries and by the
new `midnight-primitives-mc-follower` gRPC row source.

PR:
