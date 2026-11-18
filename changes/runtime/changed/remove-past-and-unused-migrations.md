#runtime #migrations

# Remove v2.1.x migrations and unused migrations from partner-chains toolkit

Midnight uses a number of migrations in upgrade from v1.0.x to v2.1.x and these
migrations are no longer need. Partner-chains also defined migrations code
that will not be used by Midnight. Since there are no other chains using
it, the migrations will never be required.

Also updates hardfork test to start from v2.1.0.

PR: https://github.com/midnightntwrk/midnight-node/pull/2256
Issue: https://github.com/midnightntwrk/midnight-node/issues/1524
