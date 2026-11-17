#node #tests

# Stop Postgres test containers timing out in CI

Each db-sync data source test starts its own Postgres container. Running all of them at once next
to the rest of the workspace made some miss testcontainers' 60s startup timeout. A nextest test
group now runs at most 8 of them at a time, and the containers start faster: initdb skips fsync
and the data directory is on tmpfs.

PR: https://github.com/midnightntwrk/midnight-node/pull/2455
