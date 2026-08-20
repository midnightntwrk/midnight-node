#!/bin/bash

# shellcheck source=qa/scripts/_lib.sh
. "$(dirname "$0")/_lib.sh"

DOCKER_PROJECT_NAME=$(derive_docker_project_name)

teardown_prior_stack "$DOCKER_PROJECT_NAME"

# Use docker to clean postgres and nats data (avoids sudo issues)
if [ -d "target/data/postgres" ] || [ -d "target/data/nats" ]; then
    echo "Cleaning postgres and nats data directories..."
    docker run --rm \
        -v "$(pwd):/project" \
        alpine sh -c "rm -rf /project/target"
    echo "Data directories cleaned"
fi

mkdir -p target/data
mkdir -p target/data/postgres
mkdir -p target/data/nats
mkdir -p target/debug

tree target/data

if [ -z "${NODE_TAG:-}" ]; then
  NODE_TAG=$(resolve_node_tag) || exit 1
  echo "NODE_TAG not set; using $NODE_TAG from the repo's node versions file"
fi
export NODE_TAG

if [ -n "${NODE_TOOLKIT_TAG:-}" ]; then
  echo "Using explicit NODE_TOOLKIT_TAG: $NODE_TOOLKIT_TAG"
else
  # The toolkit publishes a tag per node release, and a toolkit built for another
  # node fails against this one with SubxtError(Metadata(IncompatibleCodegen)) —
  # so pair it with the node, as the CI workflows do, rather than 'latest-main'.
  export NODE_TOOLKIT_TAG="$NODE_TAG"
  echo "NODE_TOOLKIT_TAG not set; pairing it with the node: $NODE_TOOLKIT_TAG"
fi

# Use the derived Docker Compose project name to create volume name
DOCKER_VOLUME_NAME="${DOCKER_PROJECT_NAME}_node_data"

# Create the named volume (empty) for Docker Compose to use BEFORE starting containers
echo "Creating empty node data volume..."
echo "Using Docker Compose project name: $DOCKER_PROJECT_NAME"
echo "Volume name: $DOCKER_VOLUME_NAME"
docker volume rm $DOCKER_VOLUME_NAME 2>/dev/null || true
docker volume create $DOCKER_VOLUME_NAME

echo "Empty node data volume created successfully"
echo "NOTE: Any docker-compose warning about 'volume already exists' is harmless and expected"
echo "      We explicitly manage the node volume externally to ensure it exists before docker compose"

resolve_indexer_image || exit 1

echo "Using the following tags:"
echo " NODE_TAG: $NODE_TAG"
echo " INDEXER_TAG: $INDEXER_TAG"
echo " NODE_TOOLKIT_TAG: $NODE_TOOLKIT_TAG"

docker compose --profile cloud up -d

wait_for_indexer_ready

docker compose --profile cloud logs | grep "Highest known block"


docker ps --format "table {{.Image}}\t{{.Names}}\t{{.Status}}"


echo "Plase make sure all the services are running and healthy"

clear_block_scanner_cache

echo "Regenarating new test data... "
ensure_block_scanner_deps
pushd qa/tools/block-scanner
bun run generate:data
popd
