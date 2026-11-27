#!/usr/bin/env bash
# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
# SPDX-License-Identifier: Apache-2.0
# Licensed under the Apache License, Version 2.0 (the "License");
# You may not use this file except in compliance with the License.
# You may obtain a copy of the License at
#
#	http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS,
# WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
# See the License for the specific language governing permissions and
# limitations under the License.

# What the ledger 8 -> 9 releases need on local-env, where the suite plays the operators.
# Sourced by local/local_env.sh.

# node-1.0.x pins Debian package versions that are gone from the mirrors.
unpin_contract_compiler_packages() {  # <checkout>
    local dockerfile; dockerfile="$(compose_dir "$1")/configurations/contract-compiler/Dockerfile"
    [ ! -f "$dockerfile" ] || sed_i -E 's/^([[:space:]]+)([a-z][a-z0-9.+-]+)=[^[:space:]]+/\1\2/' "$dockerfile"
}

# Started before its systemStart, the Cardano node backs off for 60 s and, with a 60-slot
# epoch, never forges a block.
start_cardano_node_after_system_start() {  # <checkout>
    python3 - "$(compose_dir "$1")/configurations/cardano/entrypoint.sh" <<'PY'
import sys
path = sys.argv[1]
script = open(path).read()
marker = "start the node only after systemStart"
if marker in script:
    sys.exit(0)
anchor = '\nstart_node\n\n# Wait for genesis time to arrive and node to be ready before submitting transactions\n'
if script.count(anchor) != 1:
    sys.exit("cardano entrypoint anchor not found")
wait = '\n# ' + marker + '\nwhile [ "$(date +%s)" -le "$target_time" ]; do sleep 1; done\n'
open(path, "w").write(script.replace(anchor, wait + anchor))
PY
}

prepare_old_checkout() {  # <checkout>
    unpin_contract_compiler_packages "$1"
    start_cardano_node_after_system_start "$1"
}

# Operators keep their databases in the separate layout across the swap; the new release's
# local-env runs some nodes unified, which fails on a database created by the old one.
prepare_new_checkout() {  # <checkout>
    local validator entrypoint
    for validator in "${VALIDATORS[@]}"; do
        entrypoint="$(compose_dir "$1")/configurations/midnight-nodes/$(validator_name "$validator")/entrypoint.sh"
        [ ! -f "$entrypoint" ] || sed_i 's/^export STORAGE_SEPARATION=unified$/export STORAGE_SEPARATION=separate/' "$entrypoint"
    done
}

query_postgres() {  # <sql> -> the answer without whitespace
    docker exec postgres sh -c 'psql -U "${POSTGRES_USER:-postgres}" -d postgres -tAc "$1"' sh "$1" 2>/dev/null | tr -d '[:space:]'
}

# Node 2.x needs Postgres with TLS, so the operators recreate Postgres and db-sync while the
# 1.0.400 validators keep producing.
#
# db-sync rebuilds its schema under the running nodes, and a node connection that prepared
# a query before that fails it from then on ("cached plan must not change result type").
# The node stops authoring and importing until it reconnects, so its connections are reset.
operator_step_before_first_wave() {
    local attempt health tls db_sync_address reset_connections validator best tip behind=""
    compose_new_release up -d --no-deps postgres db-sync > "$EV/recreate_postgres.log" 2>&1 || { echo "compose up postgres db-sync failed"; return 1; }
    for attempt in $(seq 1 30); do
        health=$(docker inspect -f '{{.State.Health.Status}}' postgres 2>/dev/null || echo none)
        [ "$health" = healthy ] && break
        sleep 2
    done
    [ "$health" = healthy ] || { echo "postgres is $health after the recreate"; return 1; }
    tls=$(query_postgres "show ssl")
    [ "$tls" = on ] || { echo "postgres ssl is '${tls:-unreadable}' after the recreate"; return 1; }
    for attempt in $(seq 1 60); do
        docker logs db-sync 2>&1 | grep -q 'Migration 71/71' && break
        sleep 2
    done
    db_sync_address=$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' db-sync)
    reset_connections=$(query_postgres "select count(pg_terminate_backend(pid)) from pg_stat_activity where datname = 'cexplorer' and client_addr <> '$db_sync_address' and pid <> pg_backend_pid()")
    for attempt in $(seq 1 30); do
        behind=""; tip=$(node_height)
        for validator in "${NODES[@]}"; do
            best=$(best_height_at "${validator#*=}")
            [ -n "$best" ] && [ "$best" -ge $(( tip - 2 )) ] || behind="$behind ${validator%%=*} at #${best:-?};"
        done
        [ -z "$behind" ] && break
        sleep 6
    done
    [ -z "$behind" ] || { echo "after the recreate, validators behind tip #$tip:$behind"; return 1; }
    wait_finality_progress 2 120 > /dev/null || { echo "after the recreate, finality stalled"; return 1; }
    echo "postgres recreated with TLS, db-sync migrated, $reset_connections node connections reset; every old-release validator at the tip (#$tip)"
}
