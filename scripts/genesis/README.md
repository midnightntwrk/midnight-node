# Genesis Scripts

This directory contains scripts for generating and verifying Midnight chain genesis and specifications.

## Construction

```bash
./genesis-construction.sh
```

An interactive wizard that guides you through the complete genesis construction process.

See: [Genesis Construction Guide](../../docs/genesis/construction.md)

## Verification

```bash
./genesis-verification.sh
```

An interactive wizard that verifies a generated chain specification against the Cardano smart contract state.

See: [Genesis Verification Guide](../../docs/genesis/verification.md)

---

## Local DB Sync Setup

Genesis construction and verification require access to a Cardano DB Sync PostgreSQL database. This section explains how to set up a local instance from a mainnet snapshot.

### Prerequisites

- **Docker Desktop** with sufficient resources allocated (see [Resource Requirements](#1-docker-desktop-resource-allocation) below)
- **PostgreSQL client tools** (`pg_restore`):
  ```bash
  brew install libpq
  brew link --force libpq   # adds pg_restore to PATH
  ```
- **~350 GB free disk space** (70 GB compressed snapshot + ~200 GB restored database + headroom for indexes)

### Step 1: Download a DB Sync Snapshot

Download a mainnet snapshot from:

```
https://update-cardano-mainnet.iohk.io/cardano-db-sync/index.html#13.6/
```

Pick the latest `.tgz` file (e.g., `db-sync-snapshot-schema-13.6-block-...-.tgz`). The compressed file is ~70 GB.

### Step 2: Start PostgreSQL

From the repository root:

```bash
cd scripts/genesis/db-sync
direnv allow   # generates a random password in postgres.password and exports POSTGRES_PASSWORD
docker compose up -d
```

The `.envrc` in this directory auto-generates a random password file (`postgres.password`, gitignored) on first run and exports `POSTGRES_PASSWORD` for docker-compose.

This starts a PostgreSQL 16 container with tuned settings for large dataset operations. Verify it's running:

```bash
docker compose logs postgres
```

You should see `database system is ready to accept connections`.

### Step 3: Restore the Snapshot

Extract and restore the snapshot into the running database:

```bash
PGPASSWORD=$POSTGRES_PASSWORD pg_restore \
  --host localhost \
  --port 5432 \
  --username postgres \
  --dbname cexplorer \
  --no-owner \
  --jobs 2 \
  -v /path/to/db
```

**Important notes:**
- Use `--jobs 2` or `--jobs 1` (not higher). Higher parallelism (`-j 4`) can exhaust memory on Docker and cause workers to crash.
- `--no-owner` avoids errors when the dump references a `cardano` user that doesn't exist locally.
- The restore takes **several hours** depending on disk speed.
- You will see harmless warnings like `schema "public" already exists` and `unrecognized configuration parameter "transaction_timeout"` (PG 17 parameter not present in PG 16) - these are safe to ignore.

### Step 4: Verify the Restore

Check that key tables have data:

```bash
PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U postgres -d cexplorer -c "SELECT COUNT(*) FROM block;"
```

For mainnet, you should see 11M+ blocks.

### Step 5: Create Indexes for Genesis Queries

The genesis commands create required indexes automatically when connecting. However, if the restore was interrupted (e.g., disk space ran out), some standard DB Sync indexes may be missing. You can check with:

```bash
PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U postgres -d cexplorer -c "\di"
```

If key indexes like `idx_tx_out_address`, `idx_tx_block_id`, or `idx_ma_tx_out_tx_out_id` are missing, the genesis queries will be extremely slow. In that case, you'll need to restore again with sufficient disk space (see [Troubleshooting](#troubleshooting)).

### Connection String

The connection string for the local DB Sync is:

```
postgres://postgres:<your-password>@localhost:5432/cexplorer
```

Both `genesis-construction.sh` and `genesis-verification.sh` will prompt for this value.

---

## Troubleshooting

### 1. Docker Desktop Resource Allocation

The most common source of failures is insufficient Docker Desktop resources. Before starting, go to **Docker Desktop > Settings > Resources** and configure:

| Resource | Minimum | Recommended |
|----------|---------|-------------|
| Memory | 8 GB | 12 GB |
| Virtual disk limit | 256 GB | 350 GB |

After changing these settings, Docker Desktop will restart.

### 2. No Space Left on Device

```
could not extend file "base/16384/...": No space left on device
```

The mainnet DB Sync database needs ~200 GB when fully restored. If `pg_restore` fails partway through, many indexes won't be created, making queries extremely slow.

**Fix:**
1. Increase Docker Desktop virtual disk limit (see above)
2. Clean up Docker resources to reclaim space:
   ```bash
   docker volume prune      # remove unused volumes
   docker image prune -a    # remove unused images
   ```
3. Remove the failed volume and start fresh:
   ```bash
   cd scripts/genesis/db-sync
   docker compose down -v   # removes container AND volume
   docker compose up -d     # start fresh
   ```
4. Re-run `pg_restore`

### 3. PostgreSQL Crashes with Shared Memory Error

```
could not resize shared memory segment: No space left on device
```

This happens when PostgreSQL's shared memory allocation exceeds Docker's default `/dev/shm` size (64 MB). The `docker-compose.yml` already sets `shm_size: 1g` to prevent this. If you see this error, make sure you're using `docker compose up -d` (not `docker run`).

If you previously set `shared_buffers` too high (e.g., 2 GB), PostgreSQL may fail to start entirely. The docker-compose uses `shared_buffers=512MB` which is a safe default.

### 4. pg_restore Command Not Found

```
pg_restore: command not found
```

**Fix (macOS):**
```bash
brew install libpq
brew link --force libpq
```

### 5. Password Authentication Failed

```
FATAL: password authentication failed for user "cardano"
```

The docker-compose creates user `postgres` (not `cardano`). Use:
```bash
PGPASSWORD=$POSTGRES_PASSWORD pg_restore --username postgres ...
```

### 6. Slow Queries / BufFileRead Disk Spilling

If genesis commands take hours or hang, check for active queries:

```bash
PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U postgres -d cexplorer -c \
  "SELECT pid, wait_event, state, LEFT(query, 80) FROM pg_stat_activity WHERE state = 'active';"
```

If you see `BufFileRead` as the wait event, PostgreSQL is spilling sort/hash operations to disk due to insufficient `work_mem`. The docker-compose already sets `work_mem=256MB`. If queries are still slow, the most likely cause is missing indexes (see Step 5 above).

### 7. Connection Refused

```
psql: error: connection refused
```

PostgreSQL may still be starting up, especially after a configuration change. Check the logs:

```bash
docker compose -f scripts/genesis/db-sync/docker-compose.yml logs --tail 20
```

Wait for `database system is ready to accept connections` before retrying.

### 8. Docker stop/start vs docker compose

If you change `docker-compose.yml` settings (memory tuning, shm_size, etc.), you must use:

```bash
docker compose up -d   # recreates container with new settings
```

Using `docker stop`/`docker start` preserves the old container configuration and ignores your changes.

### 9. Index Creation Takes a Long Time

Creating indexes on large tables (e.g., `tx_out` with 200M+ rows) takes 10-30 minutes per index. You can monitor progress:

```bash
PGPASSWORD=$POSTGRES_PASSWORD psql -h localhost -U postgres -d cexplorer -c \
  "SELECT phase, blocks_done, blocks_total, tuples_done, tuples_total
   FROM pg_stat_progress_create_index;"
```

Index creation is transactional - if interrupted, it rolls back cleanly. You can restart safely, but progress is lost.

### 10. Docker I/O Performance on macOS

PostgreSQL in Docker Desktop on macOS is slower than a native installation due to the overlay filesystem and VM layer. Index creation can be 2-5x slower. This is expected. If performance is critical, consider using a native PostgreSQL installation instead.
