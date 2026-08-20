#!/usr/bin/env bash

set -euxo pipefail

# Cleanup function to ensure node container is removed.
cleanup() {
    docker rm -f node >/dev/null 2>&1 || true
}

# Set up trap to cleanup on exit.
trap cleanup EXIT

if [ -z "$1" ]; then
    echo "Error: node version parameter is required" >&2
    echo "Usage: $0 <node_version>" >&2
    exit 1
fi
readonly node_version="$1"
readonly toolkit_image="midnightntwrk/midnight-node-toolkit:$node_version"

# Start the node container.
docker run \
    -d \
    --name node \
    -p 9944:9944 \
    -e SHOW_CONFIG=false \
    -e CFG_PRESET=dev \
    -e SIDECHAIN_BLOCK_BENEFICIARY="04bcf7ad3be7a5c790460be82a713af570f22e0f801f6659ab8e84a52be6969e" \
    -e THRESHOLD=0 \
    midnightntwrk/midnight-node:$node_version

# Wait for node to be ready.
echo "Waiting for node to be ready..."
timeout=60
start_time=$(date +%s)
while true; do
    sleep 3

    if (( $(date +%s) - start_time > timeout )); then
        echo "Timeout after ${timeout}s waiting for node to be ready"
        exit 1
    fi

    finalized_hash=$(curl -s -X POST http://localhost:9944 \
        -H "Content-Type: application/json" \
        -d '{
            "jsonrpc":"2.0",
            "id":1,
            "method":"chain_getFinalizedHead",
            "params":[]
        }' | jq -r .result)
    if [[ -z "$finalized_hash" || "$finalized_hash" == "null" ]]; then
        echo "No finalized hash"
        continue
    fi

    finalized_number=$(curl -s -X POST http://localhost:9944 \
        -H "Content-Type: application/json" \
        -d "{
            \"jsonrpc\":\"2.0\",
            \"id\":2,
            \"method\":\"chain_getHeader\",
            \"params\":[\"$finalized_hash\"]
        }" | jq -r '.result.number')
    if [[ -z "$finalized_number" || "$finalized_number" == "null" ]]; then
        echo "No finalized number"
        continue
    fi

    height=$((finalized_number))
    echo "finalized height: $height"
    if [[ $height -ge 1 ]]; then
        echo "Node ready - finalized height: $height"
        break
    fi
done

# 1 to 2/2.
docker run \
    --rm \
    --network host \
    -v ./target:/out \
    $toolkit_image \
    generate-txs \
    --dest-file /out/tx_1_2_2.mn \
    single-tx \
    --shielded-amount 10 \
    --unshielded-amount 10 \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --destination-address mn_shield-addr_undeployed1tth9g6jf8he6cmhgtme6arty0jde7wnypsg53qc3x5navl9za355jqqvfftm8asg986dx9puzwkmedeune9nfkuqvtmccmxtjwvlrvccwypcs \
    --destination-address mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r
cat ./target/tx_1_2_2.mn | jq -r '.tx.Midnight' | xxd -r -p > ./target/tx_1_2_2.raw

# 1 to 2/3.
docker run \
    --rm \
    --network host \
    -v ./target:/out \
    $toolkit_image \
    generate-txs \
    --dest-file /out/tx_1_2_3.mn \
    single-tx \
    --shielded-amount 10 \
    --unshielded-amount 10 \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --destination-address mn_shield-addr_undeployed1ngp7ce7cqclgucattj5kuw68v3s4826e9zwalhhmurymwet3v7psvrs4gtpv5p2zx8rd3jxpgjr4m8mxh7js7u3l33g23gcty67uq9cug4xep \
    --destination-address mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm
cat ./target/tx_1_2_3.mn | jq -r '.tx.Midnight' | xxd -r -p > ./target/tx_1_2_3.raw

# Capture genesis ledger state — matches the state the txs above are signed against.
curl -s -X POST http://localhost:9944 \
    -H "Content-Type: application/json" \
    -d '{"jsonrpc":"2.0","id":3,"method":"system_properties","params":[]}' \
    | jq -r '.result.genesis_state' \
    | sed 's/^0x//' \
    | xxd -r -p > ./target/genesis_state.raw

mv target/*.raw indexer-common/tests
