#!/usr/bin/env bash

set -eo pipefail

if [ -z "$1" ]; then
    echo "Error: node version parameter is required" >&2
    echo "Usage: $0 <node_version>" >&2
    exit 1
fi
node_version="$1"

# 1 to 2/2
docker run \
    --rm \
    --network host \
    -v ./target:/out \
    ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
    generate-txs \
    --dest-file /out/tx_1_2_2.mn \
    --to-bytes \
    single-tx \
    --shielded-amount 10 \
    --unshielded-amount 10 \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --destination-address mn_shield-addr_undeployed1tffkxdesnqz86wvds2aprwuprpvzvag5t3mkveddr33hr7xyhlhqxqzfqqxy54an7cyznaxnzs7p8tduku7fuje5mwqx9auvdn9e8x03kvvy5r6z \
    --destination-address mn_addr_undeployed1gkasr3z3vwyscy2jpp53nzr37v7n4r3lsfgj6v5g584dakjzt0xqun4d4r

docker run \
    --rm \
    --network host \
    -v ./target:/out \
    ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
    get-tx-from-context \
    --src-file /out/tx_1_2_2.mn \
    --network undeployed \
    --dest-file /out/tx_1_2_2.raw \
    --from-bytes

# 1 to 2/3
docker run \
    --rm \
    --network host \
    -v ./target:/out \
    ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
    generate-txs \
    --dest-file /out/tx_1_2_3.mn \
    --to-bytes \
    single-tx \
    --shielded-amount 10 \
    --unshielded-amount 10 \
    --source-seed "0000000000000000000000000000000000000000000000000000000000000001" \
    --destination-address mn_shield-addr_undeployed1tffkxdesnqz86wvds2aprwuprpvzvag5t3mkveddr33hr7xyhlhqxqzfqqxy54an7cyznaxnzs7p8tduku7fuje5mwqx9auvdn9e8x03kvvy5r6z \
    --destination-address mn_addr_undeployed1g9nr3mvjcey7ca8shcs5d4yjndcnmczf90rhv4nju7qqqlfg4ygs0t4ngm

docker run \
    --rm \
    --network host \
    -v ./target:/out \
    ghcr.io/midnight-ntwrk/midnight-node-toolkit:$node_version \
    get-tx-from-context \
    --src-file /out/tx_1_2_3.mn \
    --network undeployed \
    --dest-file /out/tx_1_2_3.raw \
    --from-bytes
