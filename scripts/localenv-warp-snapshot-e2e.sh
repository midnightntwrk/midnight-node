#!/usr/bin/env bash
# Local-environment warp-sync e2e against a REAL ledger_8 (v13) devnet snapshot.
#
# Validates the full version-compat chain that lets a fresh `--sync warp` node recover the ledger
# arena of an *older-ledger-version* network (real devnet: genesis+tip LedgerState v13 / ledger_8),
# under our 2.0.0/ledger_9 build:
#   1. host-fn fix        — node can instantiate the ledger_8 runtime (restored the removed
#                           `ext_ledger_8_bridge_construct_distribute_treasury_system_tx` host fn).
#   2. mock-authorities   — PR #77 `grandpa-finality-storage-fix` makes the synthetic block bump
#      grandpa fix          `System::Number` (+ GRANDPA aux) so the fork PRODUCES + FINALIZES blocks.
#   3. per-version         — serialize (server), import (client) and genesis-arena-init dispatch on the
#      dispatch             `ledger-state[vNN]` tag (v5->ledger_7, v13->ledger_8, v16->ledger_9), so a
#                           v13 genesis no longer panics the v16-only init and the v13 arena can be
#                           served/recovered. (commit 85db7767)
#
# STATUS: pieces 1+2 were validated end-to-end manually (fork boots + produces). Piece 3 + the warp
# node attach (this script's PHASE 4/5) is wired but was NOT yet run green end-to-end (the local
# docker daemon wedged after a disk-full event before the final run). Treat PHASE 4/5 as the
# remaining thing to confirm.
#
# PREREQS
#   - Docker Desktop healthy with >= ~130 GiB free (image builds ~40 GiB; fork ~12 GiB/validator).
#     If you just hit a disk-full / wedged daemon: restart Docker Desktop, then
#     `docker builder prune -af && docker system prune -af` (ONE at a time — concurrent prunes wedge it).
#   - Repos: this repo (mn5) + ../midnight-node-ops/mock-authorities (for the grandpa fix).
#   - ./GITHUB_TOKEN present (for `earthly +node-image`).
#   - local-environment npm deps installed (cd local-environment && npm ci).
set -uo pipefail
MN5=/Users/justinfrevert/dev/mn5
OPS=/Users/justinfrevert/dev/midnight-node-ops
SNAPSHOT_URL="https://dg39snjayoq3t.cloudfront.net/devnet/ip-10-0-47-151.eu-west-1.compute.internal/epoch-0/devnet-epoch-0-block-546428-v1.0.0.tar.gz"
MOCK_FIX_BRANCH="grandpa-finality-storage-fix"   # PR #77; once merged + an image is published, pull it instead
cd "$MN5"

echo "### PHASE 1 — build mock-authorities fix image (PR #77 / $MOCK_FIX_BRANCH)"
# Build from a worktree so the ops repo's checked-out branch is untouched.
git -C "$OPS" worktree add -f /tmp/ma-grandpa-fix "$MOCK_FIX_BRANCH" 2>/dev/null || true
docker build -t mock-authorities:grandpa-fix /tmp/ma-grandpa-fix/mock-authorities
export MOCK_AUTHORITIES_IMAGE=mock-authorities:grandpa-fix

echo "### PHASE 2 — build node image from this branch (warp + host-fn + per-version dispatch)"
# Must be built from a branch containing: the warp arena-sync feature, the ledger_8 host-fn fix,
# and the per-version dispatch (commit 85db7767). earthly picks up the working tree.
GITHUB_TOKEN="$(cat ./GITHUB_TOKEN)" earthly --secret GITHUB_TOKEN +node-image
# earthly tags `:latest-arm64` and `:2.0.0-<hash>-arm64`; use latest for simplicity.
export NODE_IMAGE=ghcr.io/midnight-ntwrk/midnight-node:latest-arm64
# Sanity: the image must contain BOTH fixes.
cid=$(docker create "$NODE_IMAGE"); docker cp "$cid:/midnight-node" /tmp/_nodebin >/dev/null 2>&1; docker rm "$cid" >/dev/null
echo "  host-fn symbol: $(strings -n6 /tmp/_nodebin | grep -c construct_distribute_treasury_system_tx) (want >0)"
echo "  warp protocol:  $(strings -n6 /tmp/_nodebin | grep -c midnight-ledger-sync) (want >0)"

echo "### PHASE 3 — restore the real devnet v1.0.0 snapshot as a producing fork"
# NOTE on disk: devnet ships 6 validators (~12 GiB each). To use fewer, trim the `services:` in
# local-environment/src/networks/well-known/devnet/devnet.network.yaml to node1..node3 before running
# (and the mock-authorities convert will auto-detect the validator count from the compose).
cd "$MN5/local-environment"
npm run run:devnet -- --from-snapshot "$SNAPSHOT_URL"
# Wait for production to RESUME past the mock synthetic block (needs the grandpa fix).
echo "  waiting for the fork to produce + finalize past the restored tip..."
base=$(docker logs node1 2>&1 | grep -oE 'finalized #[0-9]+' | grep -oE '[0-9]+' | sort -n | tail -1)
for _ in $(seq 1 30); do
  b=$(docker logs node1 2>&1 | grep -oE 'best: #[0-9]+' | grep -oE '[0-9]+' | sort -n | tail -1)
  [ -n "$b" ] && [ "$b" -gt "${base:-0}" ] && { echo "  fork producing: best=#$b"; break; }
  sleep 10
done

echo "### PHASE 4 — extract the REAL devnet chainspec (genesis 0x89f7, matches the snapshot)"
# Our repo's res/devnet was rebuilt for ledger_9 (genesis 0xa809) by #1604; the deployed devnet
# predates that. The pre-#1604 raw chainspec carries the original v13 genesis.
git -C "$MN5" show 01b51abb^:res/devnet/chain-spec-raw.json > /tmp/devnet-real-raw.json
echo "  chainspec entries: $(python3 -c "import json;print(len(json.load(open('/tmp/devnet-real-raw.json'))['genesis']['raw']['top']))")"

echo "### PHASE 5 — attach a fresh --sync warp node (B) and assert arena recovery"
MJ="$MN5/local-environment/src/networks/well-known/devnet/res/mock-bridge-data/devnet-mock.json"
# Build sticky --reserved-nodes from each running node's libp2p identity + its --port (node1=30334...).
RESERVED=""
i=0
for n in $(docker ps --format '{{.Names}}' | grep -E '^node[0-9]+$' | sort); do
  id=$(docker logs "$n" 2>&1 | grep -oE 'Local node identity is: [0-9A-Za-z]+' | awk '{print $NF}' | head -1)
  port=$(docker inspect "$n" --format '{{range .Config.Env}}{{println .}}{{end}}' | grep -oE '\-\-port [0-9]+' | grep -oE '[0-9]+' | head -1)
  [ -n "$id" ] && [ -n "$port" ] && RESERVED="$RESERVED --reserved-nodes /dns4/$n/tcp/$port/p2p/$id"
done
echo "  reserved peers:$RESERVED"
docker rm -f nodeB >/dev/null 2>&1
docker run -d --name nodeB --hostname nodeB --network devnet_default -p 9960:9944 \
  -v "$MJ":/res/mock-bridge-data/devnet-mock.json \
  -v /tmp/devnet-real-raw.json:/chainspec/devnet-real-raw.json:ro \
  -e CFG_PRESET=devnet -e USE_MAIN_CHAIN_FOLLOWER_MOCK=true -e DB_SYNC_POSTGRES_CONNECTION_STRING= \
  -e MOCK_REGISTRATIONS_FILE=/res/mock-bridge-data/devnet-mock.json \
  -e CARDANO_SECURITY_PARAMETER=432 -e CARDANO_ACTIVE_SLOTS_COEFF=0.05 -e BLOCK_STABILITY_MARGIN=10 \
  -e MC__FIRST_EPOCH_TIMESTAMP_MILLIS=1666656000000 -e MC__FIRST_EPOCH_NUMBER=0 \
  -e MC__EPOCH_DURATION_MILLIS=86400000 -e MC__FIRST_SLOT_NUMBER=0 -e MC__SLOT_DURATION_MILLIS=1000 \
  -e ARGS="--chain /chainspec/devnet-real-raw.json --node-key=00000000000000000000000000000000000000000000000000000000000000bb --port 30340 --rpc-port 9944 --base-path=/data --unsafe-rpc-external --rpc-cors=all --sync warp --no-mdns --public-addr /dns4/nodeB/tcp/30340 $RESERVED -lsync=info,grandpa=info,warp=debug,midnight-ledger-sync=debug" \
  "$NODE_IMAGE"

echo "  watching nodeB (up to ~8 min) for warp -> state-sync -> arena recovery..."
for _ in $(seq 1 96); do
  L=$(docker logs nodeB 2>&1)
  echo "$L" | grep -q "NoLedgerState" && { echo "FAIL: NoLedgerState"; docker logs nodeB 2>&1 | tail -25; exit 1; }
  echo "$L" | grep -q "different chain" && { echo "FAIL: genesis mismatch (wrong chainspec)"; exit 1; }
  echo "$L" | grep -q "failed to deserialize ledger genesis state" && { echo "FAIL: genesis-init version panic (per-version dispatch missing)"; exit 1; }
  if echo "$L" | grep -q "Recovered + verified ledger arena"; then
    echo "PASS: arena recovered + verified over the wire:"
    echo "$L" | grep -E "Warping|Warp sync detected|Recovering ledger arena|Recovered \+ verified|gate released|Imported #" | tail -20
    exit 0
  fi
  sleep 5
done
echo "TIMEOUT — nodeB tail:"; docker logs nodeB 2>&1 | grep -iE "warp|ledger arena|state sync|genesis|different chain|peers|Imported #" | tail -30
exit 1
