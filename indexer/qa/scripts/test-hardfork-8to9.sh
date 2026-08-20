#!/bin/bash
# Devnet rehearsal of the ledger 8 -> 9 hard fork.
#
# Boots a fresh ledger-8 dev chain on the *2.1.0 migration node binary* (so the
# `migrate_state_v8_to_v9` host fn is present from genesis), attaches the cloud
# indexer stack, submits v8 transactions, drives the governance runtime upgrade to
# the 2.1.0 WASM (spec_version -> 2_001_000, migration fires at apply+1), then
# submits v9 transactions. The indexer must cross the boundary without a ledger/
# zswap root mismatch and keep indexing.
#
# This is the generative, from-genesis rehearsal described in
# docs/hardfork-devnet-rehearsal-8to9.md. It validates the live transaction path
# across the fork; it does not stress a production-sized `run(budget)` drain.
#
# Prerequisites (see the runbook's release checklist):
#   - Indexer images that recognize spec_version 2_001_000.
#   - The 2.1.0 migration node image + runtime WASM, and a ledger-8 (1.0.x) node image.
#   - A ledger-9-capable toolkit for the post-fork sends.
#
# Usage:
#   IMAGE_REGISTRY=midnightntwrk INDEXER_TAG=<sha> \
#   FROM_NODE_TAG=1.0.0 TO_NODE_TAG=<2.1.0-local-tag> \
#   FROM_TOOLKIT_TAG=1.0.0 TO_TOOLKIT_TAG=<2.1.0-local-tag> \
#   RUNTIME_WASM=/path/to/midnight_node_runtime.compact.compressed.wasm \
#     bash qa/scripts/test-hardfork-8to9.sh
#
# Environment knobs:
#   AUTO=0                 pause for manual inspection at each phase (default: run straight through)
#   RUNTIME_WASM=<file>    use this 2.1.0 runtime WASM instead of extracting it from the TO node
#                          image. REQUIRED when the 2.1.0 image was built by swapping only the
#                          binary into a 1.0.x base -- the embedded WASM is then still ledger-8 and
#                          the "upgrade" would be a no-op.
#   STORAGE_SEPARATION, TBLOCK_CORRECTION_OFFSET, TBLOCK_CORRECTION_DISABLE_AFTER
#                          passed through to the node service if the 2.1.0 binary refuses to boot
#                          on the dev chain-spec citing missing config (values from the node's
#                          res/cfg/default.toml).

set -euo pipefail

# --- Configuration ---
FROM_NODE_TAG="${FROM_NODE_TAG:-1.0.0}"          # ledger-8 node (chain-spec source)
TO_NODE_TAG="${TO_NODE_TAG:-2.1.0}"              # ledger-9 migration node (binary + WASM)
INDEXER_TAG="${INDEXER_TAG:?INDEXER_TAG is required (e.g. dev)}"
FROM_TOOLKIT_TAG="${FROM_TOOLKIT_TAG:-$FROM_NODE_TAG}"  # toolkit for v8 sends
TO_TOOLKIT_TAG="${TO_TOOLKIT_TAG:-$TO_NODE_TAG}"        # toolkit for v9 sends
export IMAGE_REGISTRY="${IMAGE_REGISTRY:-midnightntwrk}"
AUTO="${AUTO:-1}"

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
COMPOSE_PROJECT_NAME="${COMPOSE_PROJECT_NAME:-hardfork8to9}"
TMPDIR="${REPO_ROOT}/target/hardfork-8to9-test/${COMPOSE_PROJECT_NAME}"
NODE_RPC_PORT="${NODE_RPC_PORT:-9944}"
INDEXER_API_PORT="${INDEXER_API_PORT:-8088}"
POSTGRES_PORT="${POSTGRES_PORT:-5432}"
NATS_PORT="${NATS_PORT:-4222}"
NODE_RPC_HTTP="http://localhost:${NODE_RPC_PORT}"
NODE_RPC_WS_HOST="ws://localhost:${NODE_RPC_PORT}" # from a --network host toolkit container
NODE_RPC_WS_COMPOSE="ws://node:9944"             # from inside the compose network
INDEXER_API="http://localhost:${INDEXER_API_PORT}"

echo "=== Hard-fork 8 -> 9 devnet rehearsal ==="
echo "From (ledger-8): ${FROM_NODE_TAG}   To (ledger-9): ${TO_NODE_TAG}"
echo "Indexer: ${INDEXER_TAG}   Toolkits: ${FROM_TOOLKIT_TAG} / ${TO_TOOLKIT_TAG}"
echo "Compose project: ${COMPOSE_PROJECT_NAME}   RPC/API ports: ${NODE_RPC_PORT}/${INDEXER_API_PORT}"

mkdir -p "$TMPDIR"

# The cloud services need non-empty credentials. Keep rehearsal defaults local to this isolated
# Compose project; callers can still supply their normal environment values.
: "${APP__INFRA__STORAGE__PASSWORD:=hardfork-test-password}"
: "${APP__INFRA__PUB_SUB__PASSWORD:=hardfork-test-password}"
: "${APP__INFRA__SECRET:=0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef}"
export APP__INFRA__STORAGE__PASSWORD APP__INFRA__PUB_SUB__PASSWORD APP__INFRA__SECRET

# Read the current on-chain specVersion from the node RPC. Empty on failure.
spec_version() {
  curl -sf -H "Content-Type: application/json" \
    -d '{"id":1,"jsonrpc":"2.0","method":"state_getRuntimeVersion"}' \
    "$NODE_RPC_HTTP" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['result']['specVersion'])" 2>/dev/null || echo ""
}

# Latest indexed block height via GraphQL. Empty on failure.
indexer_height() {
  curl -sf -H "Content-Type: application/json" \
    -d '{"query":"{ block { height } }"}' \
    "${INDEXER_API}/api/v4/graphql" 2>/dev/null \
    | python3 -c "import json,sys; print(json.load(sys.stdin)['data']['block']['height'])" 2>/dev/null || echo ""
}

# Block until the node has finalized at least $1 blocks. The toolkit's generate-txs
# refuses to build a transaction against a chain that has only finalized genesis
# (`GetTransactions(NodeClientError(OnlyGenesisFinalized))`), so on a freshly booted
# dev chain we must wait past block 0 before submitting the pre-fork batch.
wait_for_finalized() {
  local want="$1" n=""
  echo ">>> Waiting for finalized height >= ${want}..."
  for i in {1..40}; do
    local fh
    fh=$(curl -sf -H "Content-Type: application/json" \
      -d '{"id":1,"jsonrpc":"2.0","method":"chain_getFinalizedHead"}' "$NODE_RPC_HTTP" 2>/dev/null \
      | python3 -c "import json,sys; print(json.load(sys.stdin)['result'])" 2>/dev/null)
    if [ -n "$fh" ]; then
      n=$(curl -sf -H "Content-Type: application/json" \
        -d "{\"id\":1,\"jsonrpc\":\"2.0\",\"method\":\"chain_getHeader\",\"params\":[\"$fh\"]}" "$NODE_RPC_HTTP" 2>/dev/null \
        | python3 -c "import json,sys; print(int(json.load(sys.stdin)['result']['number'],16))" 2>/dev/null)
      if [ -n "$n" ] && [ "$n" -ge "$want" ]; then echo "Finalized height: $n"; return 0; fi
    fi
    echo "Finalized not yet >= ${want} (got '${n:-}')... ($i)"; sleep 3
  done
  echo "ERROR: node did not finalize >= ${want} blocks in time" >&2
  return 1
}

pause() {
  [ "$AUTO" = "1" ] && return 0
  echo ">>> $1"
  echo "    Press Enter to continue..."
  read -r
}

# Generate a single shielded+unshielded tx against the live node, then submit it.
# Args: $1 = toolkit tag, $2 = label used for the dest-file name.
#
# The toolkit container attaches to the compose network and reaches the node as
# `node:9944` (NODE_RPC_WS_COMPOSE) rather than `--network host` + localhost:9944:
# Docker Desktop for Mac does not give containers real host networking, so
# `--network host` toolkit runs cannot reach the published port reliably.
submit_tx() {
  local toolkit_tag="$1" label="$2"
  local dest="/out/${label}.mn"
  echo ">>> Generating ${label} tx with toolkit ${toolkit_tag}..."
  docker run --rm --network "${NETWORK_NAME}" -v "${TMPDIR}:/out" \
    "${IMAGE_REGISTRY}/midnight-node-toolkit:${toolkit_tag}" \
    generate-txs \
    --src-url "$NODE_RPC_WS_COMPOSE" \
    --dest-file "$dest" \
    single-tx \
    --shielded-amount 10 \
    --unshielded-amount 10 \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --destination-address mn_shield-addr_undeployed1tth9g6jf8he6cmhgtme6arty0jde7wnypsg53qc3x5navl9za355jqqvfftm8asg986dx9puzwkmedeune9nfkuqvtmccmxtjwvlrvccwypcs \
    --destination-address mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r
  echo ">>> Submitting ${label} tx..."
  docker run --rm --network "${NETWORK_NAME}" -v "${TMPDIR}:/out" \
    "${IMAGE_REGISTRY}/midnight-node-toolkit:${toolkit_tag}" \
    generate-txs \
    --src-file "$dest" \
    send \
    -d "$NODE_RPC_WS_COMPOSE"
}

# --- Step 1: ledger-8 chain-spec from the old node ---
echo ""
echo ">>> Step 1: Building ledger-8 chain-spec from node ${FROM_NODE_TAG}..."
docker run --rm -e CFG_PRESET=dev "${IMAGE_REGISTRY}/midnight-node:${FROM_NODE_TAG}" build-spec \
  > "${TMPDIR}/chainspec.json"
echo "Chain-spec saved ($(wc -c < "${TMPDIR}/chainspec.json") bytes)"

# --- Step 2: 2.1.0 runtime WASM ---
# Prefer an explicitly provided WASM file. Extracting from the TO node image is only
# correct when that image genuinely carries the 2.1.0 runtime; a binary-swap-into-1.0.x
# image still embeds the ledger-8 WASM, which would make the upgrade a silent no-op.
echo ""
if [ -n "${RUNTIME_WASM:-}" ]; then
  echo ">>> Step 2: Using provided runtime WASM: ${RUNTIME_WASM}"
  [ -f "${RUNTIME_WASM}" ] || { echo "ERROR: RUNTIME_WASM file not found: ${RUNTIME_WASM}" >&2; exit 1; }
  cp "${RUNTIME_WASM}" "${TMPDIR}/runtime.wasm"
else
  echo ">>> Step 2: Extracting runtime WASM from node ${TO_NODE_TAG}..."
  echo "    WARNING: if that image was built by swapping only the binary into a 1.0.x"
  echo "    base, its embedded WASM is still ledger-8 and the upgrade will be a no-op."
  echo "    Pass RUNTIME_WASM=/path/to/...compact.compressed.wasm to be safe."
  ARCH=$(uname -m)
  if [ "$ARCH" = "arm64" ] || [ "$ARCH" = "aarch64" ]; then
    WASM_PATH="/artifacts-arm64/midnight_node_runtime.compact.compressed.wasm"
  else
    WASM_PATH="/artifacts-amd64/midnight_node_runtime.compact.compressed.wasm"
  fi
  docker run --rm --entrypoint cat "${IMAGE_REGISTRY}/midnight-node:${TO_NODE_TAG}" "$WASM_PATH" \
    > "${TMPDIR}/runtime.wasm"
fi
echo "Runtime WASM ready ($(wc -c < "${TMPDIR}/runtime.wasm") bytes)"

# --- Step 3: boot the 2.1.0 binary on the ledger-8 chain-spec + indexer stack ---
echo ""
echo ">>> Step 3: Starting stack (node ${TO_NODE_TAG} binary on ${FROM_NODE_TAG} chain-spec)..."
export NODE_TAG="${TO_NODE_TAG}"                 # the binary that runs
export CHAINSPEC_PATH="${TMPDIR}/chainspec.json" # consumed by docker-compose.runtime-upgrade.yaml

cd "$REPO_ROOT"

# The project name and per-run data paths isolate this rehearsal from other local stacks.
NETWORK_NAME="${COMPOSE_PROJECT_NAME}_default" # compose network; toolkit + runtime-upgrade attach here

# The 2.1.0 binary may reject the dev chain-spec unless these config fields are set
# (they came from the node's res/cfg/default.toml on the snapshot path). Inject them via
# a generated override only when the operator supplies them, so committed compose files
# and the same-ledger runtime-upgrade test stay untouched.
COMPOSE_FILES=(-f docker-compose.yaml -f docker-compose.runtime-upgrade.yaml)
COMPOSE_OVERRIDE="${TMPDIR}/docker-compose.isolated.yaml"
cat > "$COMPOSE_OVERRIDE" <<EOF
services:
  node:
    ports: !override ["${NODE_RPC_PORT}:9944"]
  indexer-api:
    ports: !override ["${INDEXER_API_PORT}:8088"]
  postgres:
    ports: !override ["${POSTGRES_PORT}:5432"]
    volumes: !override ["${TMPDIR}/postgres:/var/lib/postgresql/data"]
  nats:
    ports: !override ["${NATS_PORT}:4222"]
    volumes: !override ["${TMPDIR}/nats:/tmp/nats"]
EOF
COMPOSE_FILES+=(-f "$COMPOSE_OVERRIDE")

compose() {
  docker compose -p "$COMPOSE_PROJECT_NAME" "${COMPOSE_FILES[@]}" "$@"
}

compose down --volumes --remove-orphans 2>/dev/null || true
if [ -n "${STORAGE_SEPARATION:-}${TBLOCK_CORRECTION_OFFSET:-}${TBLOCK_CORRECTION_DISABLE_AFTER:-}" ]; then
  NODE_ENV_OVERRIDE="${TMPDIR}/docker-compose.node-env.yaml"
  {
    echo "services:"
    echo "  node:"
    echo "    environment:"
    [ -n "${STORAGE_SEPARATION:-}" ] && echo "      STORAGE_SEPARATION: \"${STORAGE_SEPARATION}\""
    [ -n "${TBLOCK_CORRECTION_OFFSET:-}" ] && echo "      TBLOCK_CORRECTION_OFFSET: \"${TBLOCK_CORRECTION_OFFSET}\""
    [ -n "${TBLOCK_CORRECTION_DISABLE_AFTER:-}" ] && echo "      TBLOCK_CORRECTION_DISABLE_AFTER: \"${TBLOCK_CORRECTION_DISABLE_AFTER}\""
  } > "$NODE_ENV_OVERRIDE"
  COMPOSE_FILES+=(-f "$NODE_ENV_OVERRIDE")
  echo "Applying node-env override:"; cat "$NODE_ENV_OVERRIDE"
fi

compose --profile cloud up -d

echo "Waiting for indexer API to become ready..."
for i in {1..60}; do
  if curl -sf "${INDEXER_API}/ready" >/dev/null; then echo "Indexer API is ready"; break; fi
  echo "Not ready yet... ($i)"; sleep 2
done

echo "Confirming the chain started at ledger-8..."
for i in {1..10}; do
  PRE_SPEC=$(spec_version)
  [ -n "$PRE_SPEC" ] && { echo "Pre-fork specVersion: ${PRE_SPEC}"; break; }
  echo "Waiting for node RPC... ($i)"; sleep 3
done
if [ -z "${PRE_SPEC:-}" ] || [ "$PRE_SPEC" -ge 2000000 ]; then
  echo "ERROR: chain did not start at a ledger-8 specVersion (got '${PRE_SPEC:-}'). Expected < 2_000_000." >&2
  exit 1
fi

# --- Step 4: submit v8 transactions and confirm the indexer follows ---
echo ""
echo ">>> Step 4: Submitting ledger-8 transactions..."
pause "About to submit pre-fork (v8) transactions."
wait_for_finalized 2
submit_tx "$FROM_TOOLKIT_TAG" "pre_fork_v8"
sleep 6
echo "Indexer height after v8 traffic: $(indexer_height)"

# --- Step 5: governance runtime upgrade to the 2.1.0 WASM ---
echo ""
echo ">>> Step 5: Governance runtime upgrade to the 2.1.0 WASM..."
pause "About to trigger the runtime upgrade (spec_version -> 2_001_000)."
docker run --rm \
  --network "${NETWORK_NAME}" \
  -v "${TMPDIR}/runtime.wasm:/wasm/runtime.wasm" \
  "${IMAGE_REGISTRY}/midnight-node-toolkit:${TO_TOOLKIT_TAG}" \
  runtime-upgrade \
  --wasm-file /wasm/runtime.wasm \
  --rpc-url "$NODE_RPC_WS_COMPOSE" \
  -c "//Eve" -c "//Ferdie" -c "//Dave" \
  -t "//Alice" -t "//Bob" -t "//Charlie" \
  --signer-key "//Alice"

# --- Step 6: verify the bump to 2_001_000 ---
echo ""
echo ">>> Step 6: Verifying the runtime upgrade..."
sleep 6
for i in {1..10}; do
  POST_SPEC=$(spec_version)
  if [ -n "$POST_SPEC" ] && [ "$POST_SPEC" != "$PRE_SPEC" ]; then
    echo "Runtime upgraded: specVersion ${PRE_SPEC} -> ${POST_SPEC}"; break
  fi
  echo "Waiting for runtime upgrade to take effect... ($i)"; sleep 6
done
if [ "${POST_SPEC:-}" = "$PRE_SPEC" ] || [ -z "${POST_SPEC:-}" ]; then
  echo "ERROR: runtime upgrade did not take effect. specVersion still ${PRE_SPEC}" >&2
  exit 1
fi
if [ "$POST_SPEC" -lt 2000000 ]; then
  echo "ERROR: post-upgrade specVersion ${POST_SPEC} is still ledger-8 (< 2_000_000)." >&2
  exit 1
fi

# --- Step 7: submit v9 transactions ---
echo ""
echo ">>> Step 7: Submitting ledger-9 transactions..."
pause "About to submit post-fork (v9) transactions."
submit_tx "$TO_TOOLKIT_TAG" "post_fork_v9"
sleep 6

# --- Step 8: assert the crossing ---
echo ""
echo ">>> Step 8: Asserting the indexer crossed the boundary..."
# A failed crossing bails in chain-indexer with one of these (application.rs):
#   "translate ledger state" | "ledger state root mismatch ..." | "zswap state root mismatch ..."
if compose --profile cloud logs chain-indexer 2>&1 \
     | grep -Eiq "ledger state root mismatch|zswap state root mismatch|translate ledger state"; then
  echo "ERROR: chain-indexer reported a boundary failure. Offending log lines:" >&2
  compose --profile cloud logs chain-indexer 2>&1 \
    | grep -Ei "ledger state root mismatch|zswap state root mismatch|translate ledger state" >&2
  exit 1
fi

FINAL_HEIGHT=$(indexer_height)
echo "Final indexed height: ${FINAL_HEIGHT} (post-fork specVersion ${POST_SPEC})"
echo ""
echo "SUCCESS: no boundary root/translate mismatch; the indexer crossed 8 -> 9 and kept indexing."
echo "Inspect further:"
echo "  compose --profile cloud logs -f chain-indexer"
echo "  curl -s ${INDEXER_API}/api/v4/graphql -H 'Content-Type: application/json' -d '{\"query\":\"{ block { height } }\"}'"
echo "Tear down:"
echo "  compose --profile cloud down --volumes --remove-orphans"
