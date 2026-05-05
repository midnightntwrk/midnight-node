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
#     epoch >= MIN_EPOCH + every historical block that produced or consumed
#     a Midnight UTxO + every block that produced a $POLICIES tx_out
#     consumed inside the detail window.
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
#   * `epoch` / `epoch_param` for the epoch range covering the window.
#   * Pool and stake tables left empty (`pool_hash`, `pool_metadata_ref`,
#     `pool_update`, `pool_owner`, `pool_retire`, `stake_address`,
#     `epoch_stake`): mainnet's first 1000 blocks have no stake-based
#     candidates.
#
# Approach: one psql session, server-side TEMP TABLEs to materialise the
# "consumed-in-window NIGHT producer" tx_out set once, reused everywhere.

set -Eeuo pipefail

SOURCE_DSN=${SOURCE_DSN:-"postgres://127.0.0.1:5432/cexplorer"}
OUTPUT=${OUTPUT:-"snapshot.sql.xz"}

# Cardano mainnet block window for Midnight blocks 1..1000.
#
# Lower bound 13164005 = the Midnight genesis cardano-tip
# (res/mainnet/cardano-tip.json, == cnight observation cursor in
# cnight-config.json). The cnight observation iterates Cardano blocks
# starting from this position, so trimming below this would miss token
# events the producer saw and break cnight-observation's byte-exact
# inherent check.
#
# Upper bound 13174340 = empirically the smallest value that still lets
# midnight-node sync to block 1000. mc_hash advanced faster than wall
# clock during the producer's initial catch-up phase: by Midnight block
# 332 the mc_hash is already at 13173503, by 924 it's at 13174301, and
# the highest mc_hash referenced in blocks 1..1000 is somewhere in
# (13174335, 13174340]. 13174335 stalls at #991; 13174340 reaches #1000+.
#
# Header range and detail range are deliberately the same: the
# block-data-source's get_latest_block_info / get_blocks_by_numbers only
# need to resolve mc_hashes for Midnight blocks 1..1000 (all in window),
# and chain-tip announce-verification fails anyway with a partial
# Cardano view, so widening the header range past the detail range
# would just bloat the snapshot.
MIN_BLOCK_NO=${MIN_BLOCK_NO:-13164005}
MAX_BLOCK_NO=${MAX_BLOCK_NO:-13174340}
MIN_EPOCH=${MIN_EPOCH:-617}
DETAIL_MIN_BLOCK_NO=${DETAIL_MIN_BLOCK_NO:-13164005}
DETAIL_MAX_BLOCK_NO=${DETAIL_MAX_BLOCK_NO:-13174340}

# Midnight script addresses on Cardano mainnet. Sourced from res/mainnet/*.json
# unless noted. Every tx_out at one of these addresses is kept in the snapshot
# regardless of block window -- needed for committee selection / cnight
# registrations / governed-map / reserve & ICS reads on cold start.
ADDRS=(
  addr1w9e7ft4rrdd4rkdseguxr9hudfxyytm5ckh2qy0yhz7lfeg9lvhq7  # cNIGHT mapping validator -- DustMappingDatum UTxOs (cnight-addresses.json)
  addr1wxg3mm3436f57r4r9t6cqdvxe0hwjusayz4ed8ulmlenttqj62ul2  # Federated authority: Council (federated-authority-addresses.json)
  addr1w8umlgsw6cfkxpdk2jekzwa7rjdx7tc937mpahhyn00430s074k8y  # Federated authority: Technical Committee
  addr1w950c5zxn5fhwlauvpy3ssk287q0qlwz6e2zc4gaj62vaxsy3s9p0  # Reserve validator -- locked NIGHT reserve (reserve-addresses.json)
  addr1wyczfpxfnf5hvp36mrn655ye4k2cwluvlez6phx8jx46k6s2ttdaq  # Illiquid Circulation Supply (ICS) validator (ics-addresses.json)
  addr1w9cky55qfmt98yvf0yxa0rzvynm7ag5c8c2f3xwsaja8y5cwpj7fy  # Committee candidates -- stake-based registrations (registered-candidates-addresses.json)
  addr1wykryf2zuv5p0un2wk7yn6408n5rrd3d4ljqgr3099hr8xst409lt  # Permissioned candidates / D-parameter list validator (holds NFTs of policy 2c322542..., not in res/mainnet/*.json)
)
ADDR_LIST=$(printf "'%s', " "${ADDRS[@]}")
ADDR_LIST="${ADDR_LIST%, }"

# Cardano native-asset policies whose producer tx_outs we backfill when consumed
# inside the detail window (asset_spend producer-side join). Sourced from
# res/mainnet/*.json.
#
# Deliberately absent (no in-window producer joins observed on first 1000
# blocks, so backfilling them would just bloat the snapshot):
#   * 68fc50469d13777fbc60491842ca3f80f07dc2d6542c551d9694ce9a (reserve validator NFT)
#   * d24b012f7b2a99a671b7e1196847f183982d70db02ed37068e4e49e9 (reserve two-stage)
#   * cb797228400c64a31a7a7053305f244a55af7602238e7428813f82ca (permissioned-candidates two-stage)
POLICIES=(
  0691b2fecca1ac4f53cb6dfb00b7013e561d1f34403b957cbb5af1fa  # $NIGHT asset (cnight_policy_id)
  911dee358e934f0ea32af5803586cbeee9721d20ab969f9fdff335ac  # Council NFT (council_policy_id)
  e91becb9536df62eed161713311cc534ae909636ba9529b38e2a18f3  # Council two-stage (council_two_stage_policy_id)
  f9bfa20ed6136305b654b3613bbe1c9a6f2f058fb61edee49bdf58be  # Technical Committee NFT (technical_committee_policy_id)
  11d1de535579d929060a22828992802c77f329470adadaec10d2490c  # Technical Committee two-stage
  00d92f55c57d6d95f863202885e76304e6ef970767249413561b289c  # Authorization / governed-map (authorization-addresses.json)
  302484c99a6976063ad8e7aa5099ad95877f8cfe45a0dcc791abab6a  # ICS validator NFT (illiquid_circulation_supply_validator_policy_id)
  8f2c043f857c6acb716d27d67e9cb609c9c9814b7d7b938d6c410733  # ICS two-stage
  2c322542e32817f26a75bc49eaaf3ce831b62dafe4040e2f296e339a  # Permissioned-candidates NFT (permissioned_candidates_policy_id)
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
# FKs, COPYing into the populated tables fails because some rows reference
# the empty pool / stake tables (pool_hash, pool_metadata_ref, pool_update,
# pool_owner, pool_retire, stake_address, epoch_stake).
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
--    b) in-window outputs that hold a \$POLICIES token (for cnight
--       asset_create at any holder). Non-\$POLICIES in-window outputs are
--       unreachable: every cnight scan over a block range joins ma_tx_out
--       and filters ma_tx_out.ident to a specific \$POLICIES asset.
--    c) the night-producer set from #3 (for cnight asset_spend's producer
--       join).
CREATE TEMP TABLE k_tx_outs (id bigint PRIMARY KEY, tx_id bigint) ON COMMIT PRESERVE ROWS;
INSERT INTO k_tx_outs (id, tx_id)
  SELECT id, tx_id FROM tx_out WHERE address IN ($ADDR_LIST)
  ON CONFLICT DO NOTHING;
INSERT INTO k_tx_outs (id, tx_id)
  SELECT DISTINCT o.id, o.tx_id FROM tx_out o
  INNER JOIN tx t ON t.id = o.tx_id
  WHERE t.block_id IN (SELECT id FROM w_blocks)
    AND EXISTS (
      SELECT 1 FROM ma_tx_out m
      INNER JOIN multi_asset ma ON ma.id = m.ident
      WHERE m.tx_out_id = o.id AND ma.policy IN ($POLICY_LIST)
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

-- 6. The full set of blocks we keep: detail window + Byron EBBs in
--    epoch >= MIN_EPOCH + every block referenced by a kept tx.
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

-- block: project only the columns the runtime actually reads (block_no,
-- hash, epoch_no, slot_no, time, tx_count, id+slot_leader_id for joins) and
-- emit zero/NULL for the rest. The receiving postgres still has the schema
-- columns, but vrf_key / op_cert / size / proto_*  / epoch_slot_no /
-- previous_id are unused by every db-sync query the node makes (verified by
-- grepping for block.<col>). FK constraints are already stripped earlier so
-- previous_id NULL is fine.
SELECT 'COPY public.block FROM stdin;';
COPY (
  SELECT
    b.id,
    b.hash,
    b.epoch_no,
    b.slot_no,
    NULL::integer  AS epoch_slot_no,
    b.block_no,
    NULL::bigint   AS previous_id,
    b.slot_leader_id,
    0              AS size,
    b.time,
    b.tx_count,
    0              AS proto_major,
    0              AS proto_minor,
    NULL::varchar  AS vrf_key,
    NULL::bytea    AS op_cert,
    NULL::bigint   AS op_cert_counter
  FROM block b WHERE b.id IN (SELECT id FROM k_blocks)
) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.slot_leader FROM stdin;';
COPY (SELECT DISTINCT sl.* FROM slot_leader sl
      INNER JOIN block b ON b.slot_leader_id = sl.id
      WHERE b.id IN (SELECT id FROM k_blocks)) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.tx FROM stdin;';
COPY (SELECT t.* FROM tx t WHERE t.id IN (SELECT id FROM k_txs)) TO STDOUT;
SELECT '\\.';

-- tx_metadata is read by exactly one query (the c2m bridge in
-- partner-chains/toolkit/data-sources/db-sync/src/db_model.rs), and that
-- join filters on key = TOKEN_TRANSFER_METADATUM_KEY = 6500973. All other
-- metadata labels are unreachable.
SELECT 'COPY public.tx_metadata FROM stdin;';
COPY (
  SELECT m.* FROM tx_metadata m
  WHERE m.tx_id IN (SELECT id FROM w_txs)
    AND m.key = 6500973
) TO STDOUT;
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

-- ma_tx_out / multi_asset: every consumer (cnight asset_create / asset_spend
-- / registrations, federated_authority, governed-map) joins with a strict
-- multi_asset.policy equality, or resolves the (policy, name) ident via
-- MultiAssetCache, both restricted to the \$POLICIES set. Rows for any other
-- policy are unreachable and pure bloat.
SELECT 'COPY public.ma_tx_out FROM stdin;';
COPY (
  SELECT m.* FROM ma_tx_out m
  INNER JOIN multi_asset ma ON ma.id = m.ident
  WHERE m.tx_out_id IN (SELECT id FROM k_tx_outs)
    AND ma.policy IN ($POLICY_LIST)
) TO STDOUT;
SELECT '\\.';

SELECT 'COPY public.multi_asset FROM stdin;';
COPY (
  SELECT * FROM multi_asset WHERE policy IN ($POLICY_LIST)
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

# Pipe assembled SQL through xz for shipping. -9 -e (extreme) buys an extra
# few percent over -6 at the cost of more CPU; the SQL is highly repetitive
# (COPY-tab-separated rows, hex-encoded bytea) so the bigger dictionary
# helps. The loader (run-sync.sh) decompresses on the fly with `xz -d`.
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
} | xz -9 -e -c >"$OUTPUT"

echo "Snapshot written: $OUTPUT ($(wc -c <"$OUTPUT" | awk '{printf "%.1f MB", $1/1024/1024}'))" >&2
