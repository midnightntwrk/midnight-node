#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
# http://www.apache.org/licenses/LICENSE-2.0
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# Builds a cexplorer (cardano-db-sync) snapshot containing exactly the rows
# that the Midnight Mainnet node's data sources read while syncing the first
# 1000 blocks. The snapshot has to be COMPLETE for those queries -- not just
# minimal -- because the cnight-observation pallet's check_inherent compares
# its db-derived UTxO set byte-exactly against the block author's claim, and
# any divergence aborts block import.
#
# Concretely the snapshot includes:
#   * meta / schema_version (tiny reference rows)
#   * `block` for: the cardano window 13160000..13180000 + Byron EBBs in
#     epoch >= MIN_EPOCH + every block past 13180000 up to live tip (so
#     get_latest_block_info / block-announce validation see a recent tip)
#     + every historical block that produced or consumed a Midnight UTxO
#     + every block that produced a $POLICIES tx_out consumed inside the
#     detail window.
#   * `slot_leader` referenced by those blocks (FK only; not queried).
#   * `tx` in the detail window + every tx that produced/consumed a Midnight
#     UTxO + every tx that produced a $POLICIES tx_out consumed in window.
#   * `tx_out` at Midnight addresses + every tx_out in the detail window
#     + every $POLICIES tx_out consumed inside the detail window.
#   * `tx_in` for every spending tx in the detail window + every tx_in
#     consuming a Midnight tx_out (historical deregistrations).
#   * `tx_metadata` for txs in the detail window (c2m bridge messages).
#   * `datum` referenced by any tx_out we kept.
#   * `ma_tx_out` for any tx_out we kept.
#   * `multi_asset` for any ident we kept.
#   * Pool / epoch tables (small).
#   * `stake_address` / `epoch_stake` left empty (mainnet's first 1000 blocks
#     have no registered candidates).
#
# Approach: one psql session, server-side TEMP TABLEs to materialise the
# "consumed-in-window NIGHT producer" tx_out set once, reused everywhere.

set -Eeuo pipefail

SOURCE_DSN=${SOURCE_DSN:-"postgres://127.0.0.1:10010/cexplorer"}
OUTPUT=${OUTPUT:-"snapshot.sql.xz"}

# Cardano mainnet block window. 13164005 is Midnight Mainnet's cardano-tip;
# Midnight block #1's mc_hash references Cardano 13172990 (2026-03-18), and
# block #1000 is ~300 blocks later. The detail window 13160000..13180000
# generously covers all 1000 mc_hash references with margin. We don't bother
# keeping a wider header-only block range past the detail window: chain-tip
# announce-verification fails anyway with a partial Cardano view, and the
# block-data-source's get_latest_block_info / get_blocks_by_numbers only
# need to resolve mc_hashes for Midnight blocks 1..1000, all in window.
MIN_BLOCK_NO=${MIN_BLOCK_NO:-13160000}
MAX_BLOCK_NO=${MAX_BLOCK_NO:-13180000}
MIN_EPOCH=${MIN_EPOCH:-617}
DETAIL_MIN_BLOCK_NO=${DETAIL_MIN_BLOCK_NO:-13160000}
DETAIL_MAX_BLOCK_NO=${DETAIL_MAX_BLOCK_NO:-13180000}

ADDRS=(
  addr1w9e7ft4rrdd4rkdseguxr9hudfxyytm5ckh2qy0yhz7lfeg9lvhq7
  addr1wxg3mm3436f57r4r9t6cqdvxe0hwjusayz4ed8ulmlenttqj62ul2
  addr1w8umlgsw6cfkxpdk2jekzwa7rjdx7tc937mpahhyn00430s074k8y
  addr1w950c5zxn5fhwlauvpy3ssk287q0qlwz6e2zc4gaj62vaxsy3s9p0
  addr1wyczfpxfnf5hvp36mrn655ye4k2cwluvlez6phx8jx46k6s2ttdaq
  addr1w9cky55qfmt98yvf0yxa0rzvynm7ag5c8c2f3xwsaja8y5cwpj7fy
  addr1wykryf2zuv5p0un2wk7yn6408n5rrd3d4ljqgr3099hr8xst409lt
)
ADDR_LIST=$(printf "'%s', " "${ADDRS[@]}")
ADDR_LIST="${ADDR_LIST%, }"

POLICIES=(
  0691b2fecca1ac4f53cb6dfb00b7013e561d1f34403b957cbb5af1fa
  911dee358e934f0ea32af5803586cbeee9721d20ab969f9fdff335ac
  e91becb9536df62eed161713311cc534ae909636ba9529b38e2a18f3
  f9bfa20ed6136305b654b3613bbe1c9a6f2f058fb61edee49bdf58be
  11d1de535579d929060a22828992802c77f329470adadaec10d2490c
  00d92f55c57d6d95f863202885e76304e6ef970767249413561b289c
  302484c99a6976063ad8e7aa5099ad95877f8cfe45a0dcc791abab6a
  8f2c043f857c6acb716d27d67e9cb609c9c9814b7d7b938d6c410733
  2c322542e32817f26a75bc49eaaf3ce831b62dafe4040e2f296e339a
)
POLICY_LIST=$(printf "decode('%s', 'hex'), " "${POLICIES[@]}")
POLICY_LIST="${POLICY_LIST%, }"

if ! command -v psql >/dev/null; then echo "psql is required" >&2; exit 1; fi
if ! command -v pg_dump >/dev/null; then echo "pg_dump is required" >&2; exit 1; fi

PSQL=(psql "$SOURCE_DSN" -v ON_ERROR_STOP=1 -X --no-align --tuples-only --pset=footer=off)
PG_DUMP_BASE=(pg_dump "$SOURCE_DSN" --no-owner --no-privileges --no-publications --no-subscriptions)

echo "Connecting to $SOURCE_DSN..." >&2
"${PSQL[@]}" -c "select 1" >/dev/null

echo "Window: header $MIN_BLOCK_NO..$MAX_BLOCK_NO  detail $DETAIL_MIN_BLOCK_NO..$DETAIL_MAX_BLOCK_NO" >&2

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

echo "Dumping schema..." >&2
SCHEMA_FILE=$TMPDIR/schema.sql
"${PG_DUMP_BASE[@]}" --schema-only >"$SCHEMA_FILE"

# Strip pg_dump 18 \restrict directives (postgres 17 client doesn't recognise
# them) and any FK constraint emitted across two lines. Without removing the
# FKs the empty stake_address / epoch_stake tables fail to load.
SCHEMA_NO_FK=$TMPDIR/schema_no_fk.sql
awk '
  /^\\restrict / || /^\\unrestrict / { next }
  /^ALTER TABLE ONLY/ {
    alter_line = $0
    if ((getline next_line) <= 0) { print alter_line; exit }
    if (next_line ~ /ADD CONSTRAINT.*FOREIGN KEY/) { next }
    print alter_line; print next_line; next
  }
  { print }
' "$SCHEMA_FILE" >"$SCHEMA_NO_FK"

# A single psql session does the entire data dump. Server-side TEMP TABLEs
# materialise the "consumed-in-window NIGHT producer" set once, then every
# COPY filters off it. Output stream interleaves SQL-string SELECTs (the
# COPY headers) with COPY ... TO STDOUT (the data) so the result is loadable
# back into a fresh postgres without any post-processing.
echo "Dumping data (one psql session, server-side temp tables)..." >&2
DATA_FILE=$TMPDIR/data.sql
"${PSQL[@]}" -q --no-psqlrc <<SQL >"$DATA_FILE"
\\set ON_ERROR_STOP on
SET enable_seqscan = off;
SET statement_timeout = 0;

-- =============================================================================
-- Build the relevant-row sets as TEMP TABLEs. They live for this session only.
-- =============================================================================

-- 1. Spending side: txs and blocks inside the detail window.
CREATE TEMP TABLE w_blocks (id bigint PRIMARY KEY) ON COMMIT PRESERVE ROWS;
INSERT INTO w_blocks
  SELECT id FROM block WHERE block_no BETWEEN $DETAIL_MIN_BLOCK_NO AND $DETAIL_MAX_BLOCK_NO;
ANALYZE w_blocks;

CREATE TEMP TABLE w_txs (id bigint PRIMARY KEY) ON COMMIT PRESERVE ROWS;
INSERT INTO w_txs
  SELECT tx.id FROM tx WHERE tx.block_id IN (SELECT id FROM w_blocks);
ANALYZE w_txs;

-- 2. (tx_out_id, tx_out_index) of every UTxO consumed by a tx in window.
CREATE TEMP TABLE w_consumed (tx_out_id bigint, tx_out_index integer) ON COMMIT PRESERVE ROWS;
INSERT INTO w_consumed
  SELECT DISTINCT ti.tx_out_id, ti.tx_out_index
  FROM tx_in ti
  WHERE ti.tx_in_id IN (SELECT id FROM w_txs);
CREATE INDEX ON w_consumed (tx_out_id, tx_out_index);
ANALYZE w_consumed;

-- 3. Producer tx_outs of those consumptions that hold a \$POLICIES token.
--    These are the rows asset_spend's producer-side join needs.
CREATE TEMP TABLE w_night_producers (tx_out_id bigint PRIMARY KEY, tx_id bigint) ON COMMIT PRESERVE ROWS;
INSERT INTO w_night_producers
  SELECT DISTINCT o.id, o.tx_id
  FROM tx_out o
  INNER JOIN w_consumed wc ON wc.tx_out_id = o.tx_id AND wc.tx_out_index = o.index
  WHERE EXISTS (
    SELECT 1 FROM ma_tx_out m
    INNER JOIN multi_asset ma ON ma.id = m.ident
    WHERE m.tx_out_id = o.id AND ma.policy IN ($POLICY_LIST)
  );
CREATE INDEX ON w_night_producers (tx_id);
ANALYZE w_night_producers;

-- 4. The full set of tx_outs we keep:
--    a) at Midnight addresses (any block, for committee-selection / cnight
--       registrations / governed-map);
--    b) every output in the detail window (for cnight asset_create at any
--       holder, and for spending-side tx_in joins);
--    c) the night-producer set from #3 (for cnight asset_spend's producer
--       join).
CREATE TEMP TABLE k_tx_outs (id bigint PRIMARY KEY, tx_id bigint) ON COMMIT PRESERVE ROWS;
INSERT INTO k_tx_outs (id, tx_id)
  SELECT id, tx_id FROM tx_out WHERE address IN ($ADDR_LIST)
  ON CONFLICT DO NOTHING;
INSERT INTO k_tx_outs (id, tx_id)
  SELECT o.id, o.tx_id FROM tx_out o
  INNER JOIN w_blocks wb ON wb.id IN (SELECT block_id FROM tx WHERE tx.id = o.tx_id)
  WHERE EXISTS (
    SELECT 1 FROM tx INNER JOIN w_blocks wb2 ON wb2.id = tx.block_id
    WHERE tx.id = o.tx_id
  )
  ON CONFLICT DO NOTHING;
INSERT INTO k_tx_outs (id, tx_id)
  SELECT tx_out_id, tx_id FROM w_night_producers
  ON CONFLICT DO NOTHING;
CREATE INDEX ON k_tx_outs (tx_id);
ANALYZE k_tx_outs;

-- 5. The full set of txs we keep: every tx referenced by any kept tx_out,
--    plus every tx in the detail window and every tx that consumes a
--    Midnight tx_out.
CREATE TEMP TABLE k_txs (id bigint PRIMARY KEY, block_id bigint) ON COMMIT PRESERVE ROWS;
INSERT INTO k_txs (id, block_id)
  SELECT id, block_id FROM w_txs
    INNER JOIN tx USING (id)
  ON CONFLICT DO NOTHING;
INSERT INTO k_txs (id, block_id)
  SELECT DISTINCT tx.id, tx.block_id FROM tx
  WHERE tx.id IN (SELECT tx_id FROM k_tx_outs)
  ON CONFLICT DO NOTHING;
INSERT INTO k_txs (id, block_id)
  SELECT DISTINCT consuming_tx.id, consuming_tx.block_id
  FROM tx consuming_tx
  INNER JOIN tx_in ti ON ti.tx_in_id = consuming_tx.id
  INNER JOIN tx_out po ON po.tx_id = ti.tx_out_id AND po.index = ti.tx_out_index
  WHERE po.address IN ($ADDR_LIST)
  ON CONFLICT DO NOTHING;
CREATE INDEX ON k_txs (block_id);
ANALYZE k_txs;

-- 6. The full set of blocks we keep: detail window + Byron EBBs + every
--    block past detail window up to live tip (header-only, for
--    block-announce validation) + every block referenced by a kept tx.
CREATE TEMP TABLE k_blocks (id bigint PRIMARY KEY) ON COMMIT PRESERVE ROWS;
INSERT INTO k_blocks
  SELECT id FROM block
  WHERE (block_no BETWEEN $MIN_BLOCK_NO AND $MAX_BLOCK_NO)
     OR (block_no IS NULL AND epoch_no >= $MIN_EPOCH)
  ON CONFLICT DO NOTHING;
INSERT INTO k_blocks
  SELECT DISTINCT block_id FROM k_txs
  ON CONFLICT DO NOTHING;
ANALYZE k_blocks;

-- =============================================================================
-- Emit the data block. SELECT 'literal' lines are interleaved with COPY ...
-- TO STDOUT to produce a self-loading SQL stream.
-- =============================================================================

SELECT 'SET session_replication_role = ''replica'';';
SELECT 'SET client_min_messages = warning;';
SELECT '';

SELECT 'COPY public.meta FROM stdin;';
COPY (SELECT * FROM meta) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.schema_version FROM stdin;';
COPY (SELECT * FROM schema_version) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.block FROM stdin;';
COPY (SELECT b.* FROM block b WHERE b.id IN (SELECT id FROM k_blocks)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.slot_leader FROM stdin;';
COPY (SELECT DISTINCT sl.* FROM slot_leader sl
      INNER JOIN block b ON b.slot_leader_id = sl.id
      WHERE b.id IN (SELECT id FROM k_blocks)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.tx FROM stdin;';
COPY (SELECT t.* FROM tx t WHERE t.id IN (SELECT id FROM k_txs)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.tx_metadata FROM stdin;';
COPY (SELECT m.* FROM tx_metadata m WHERE m.tx_id IN (SELECT id FROM w_txs)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.tx_out FROM stdin;';
COPY (SELECT o.* FROM tx_out o WHERE o.id IN (SELECT id FROM k_tx_outs)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.tx_in FROM stdin;';
COPY (
  SELECT ti.* FROM tx_in ti WHERE ti.tx_in_id IN (SELECT id FROM w_txs)
  UNION
  SELECT ti.* FROM tx_in ti
  INNER JOIN tx_out po ON po.tx_id = ti.tx_out_id AND po.index = ti.tx_out_index
  WHERE po.address IN ($ADDR_LIST)
) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.datum FROM stdin;';
COPY (
  SELECT * FROM datum WHERE hash IN (
    SELECT DISTINCT data_hash FROM tx_out
    WHERE id IN (SELECT id FROM k_tx_outs) AND data_hash IS NOT NULL
  )
) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.ma_tx_out FROM stdin;';
COPY (SELECT m.* FROM ma_tx_out m WHERE m.tx_out_id IN (SELECT id FROM k_tx_outs)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.multi_asset FROM stdin;';
COPY (
  SELECT * FROM multi_asset WHERE id IN (
    SELECT DISTINCT m.ident FROM ma_tx_out m
    WHERE m.tx_out_id IN (SELECT id FROM k_tx_outs)
  )
) TO STDOUT;
SELECT '\\.';

-- pool_hash / pool_metadata_ref / pool_update / pool_owner / pool_retire:
-- only consumed by stake-based committee selection. Mainnet's first 1000
-- blocks have zero registered (stake-based) candidates -- the chain runs
-- entirely on permissioned candidates -- so these queries always return
-- empty. Skip them entirely; the receiving postgres still has the empty
-- tables from the schema dump.
SELECT 'COPY public.pool_hash FROM stdin;';
COPY (SELECT * FROM pool_hash WHERE FALSE) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.stake_address FROM stdin;';
COPY (SELECT * FROM stake_address WHERE FALSE) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.pool_metadata_ref FROM stdin;';
COPY (SELECT * FROM pool_metadata_ref WHERE FALSE) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.pool_update FROM stdin;';
COPY (SELECT * FROM pool_update WHERE FALSE) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.pool_owner FROM stdin;';
COPY (SELECT * FROM pool_owner WHERE FALSE) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.pool_retire FROM stdin;';
COPY (SELECT * FROM pool_retire WHERE FALSE) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.epoch FROM stdin;';
COPY (SELECT * FROM epoch
      WHERE no >= $MIN_EPOCH - 2
        AND no <= (SELECT max(epoch_no) FROM block WHERE block_no <= $MAX_BLOCK_NO)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.epoch_param FROM stdin;';
COPY (SELECT * FROM epoch_param
      WHERE epoch_no >= $MIN_EPOCH - 2
        AND epoch_no <= (SELECT max(epoch_no) FROM block WHERE block_no <= $MAX_BLOCK_NO)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.epoch_stake FROM stdin;';
COPY (SELECT * FROM epoch_stake WHERE FALSE) TO STDOUT;
SELECT '\\.';

SELECT 'RESET session_replication_role;';
SELECT 'ANALYZE;';
SQL

# Pipe assembled SQL through xz for shipping. xz -6 gives ~6× ratio on this
# data; the loader (run-sync.sh) decompresses on the fly with `xz -d`.
echo "Assembling $OUTPUT..." >&2
{
  echo "-- Midnight Mainnet sync snapshot"
  echo "-- Source: $SOURCE_DSN"
  echo "-- Cardano window: blocks $MIN_BLOCK_NO..$MAX_BLOCK_NO (detail $DETAIL_MIN_BLOCK_NO..$DETAIL_MAX_BLOCK_NO, epoch $MIN_EPOCH+)"
  echo "-- Generated $(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo
  cat "$SCHEMA_NO_FK"
  echo
  cat "$DATA_FILE"
} | xz -6 -c >"$OUTPUT"

echo "Snapshot written: $OUTPUT ($(wc -c <"$OUTPUT" | awk '{printf "%.1f MB", $1/1024/1024}'))" >&2
