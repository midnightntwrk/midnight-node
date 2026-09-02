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
        alpine sh -c "rm -rf /project/target/data /project/target/postgres /project/target/nats"
    echo "Data directories cleaned"
fi

docker run --rm \
    -v "$(pwd):/project" \
    alpine sh -c "mkdir -p /project/target/data /project/target/postgres /project/target/nats"

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

# Create the named volume and populate it with fresh node data BEFORE starting containers
echo "Creating and populating node data volume..."
echo "Using Docker Compose project name: $DOCKER_PROJECT_NAME"
echo "Volume name: $DOCKER_VOLUME_NAME"
docker volume rm $DOCKER_VOLUME_NAME 2>/dev/null || true
docker volume create $DOCKER_VOLUME_NAME

# Extract base version (X.Y.Z) from NODE_TAG, removing any tag suffix (e.g., "0.18.0-rc.11" -> "0.18.0")
# This handles the format change from X.Y.Z-<tag> to X.Y.Z
NODE_VERSION_BASE=$(echo "$NODE_TAG" | sed 's/-.*$//')

# Determine which .node directory format to use
# Try new format (X.Y.Z) first, then fallback to old format (X.Y.Z-<tag>) for backward compatibility
NODE_DATA_DIR=""
if [ -d "$(pwd)/.node/$NODE_VERSION_BASE" ]; then
    NODE_DATA_DIR=".node/$NODE_VERSION_BASE"
    echo "Using new format .node directory: $NODE_DATA_DIR"
elif [ -d "$(pwd)/.node/$NODE_TAG" ]; then
    NODE_DATA_DIR=".node/$NODE_TAG"
    echo "Using old format .node directory: $NODE_DATA_DIR"
else
    echo "ERROR: Neither .node/$NODE_VERSION_BASE nor .node/$NODE_TAG directory found!"
    echo "Available .node directories:"
    ls -la .node/ 2>/dev/null || echo "  (none found)"
    exit 1
fi

# Use a temporary container to copy data into the volume
echo "Copying fresh node data from $NODE_DATA_DIR/ into volume..."
docker run --rm \
  -v "$(pwd)/$NODE_DATA_DIR:/source:ro" \
  -v $DOCKER_VOLUME_NAME:/node \
  alpine sh -c "cp -r /source/. /node/ && chmod -R 777 /node/chain"

echo "Node data volume populated successfully"
echo "NOTE: Any docker-compose warning about 'volume already exists' is harmless and expected"
echo "      We explicitly manage the node volume externally to inject fresh test data"

resolve_indexer_image || exit 1

echo "Using the following tags:"
echo " NODE_TAG: $NODE_TAG"
echo " INDEXER_TAG: $INDEXER_TAG"
echo " NODE_TOOLKIT_TAG: $NODE_TOOLKIT_TAG"

docker compose --profile cloud up -d

wait_for_indexer_ready

echo "Chain startup info:"
docker compose --profile cloud logs | grep "Highest known block"

docker ps --format "table {{.Image}}\t{{.Names}}\t{{.Status}}"


echo "Plase make sure all the services are running and healthy"

clear_block_scanner_cache

echo "Regenarating new test data... "
ensure_block_scanner_deps
pushd qa/tools/block-scanner
bun run generate:data
popd
