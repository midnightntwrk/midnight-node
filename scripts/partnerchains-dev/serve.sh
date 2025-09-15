#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) 2025 Midnight Foundation
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

NODE_POD_NAME=${POD_NAME:-db-sync-cardano-node-01-0}
DBSYNC_POD_NAME=${POD_NAME:-db-sync-cardano-01-0 }
NAMESPACE=${NAMESPACE:-qanet-spo-01}
NODE_HOST=${NODE_HOST:-localhost}
# Set AS_INIT to exit after running
AS_INIT=${AS_INIT:-}
USE_EXISTING_SOCKET=${USE_EXISTING_SOCKET:-0}

# Check socat is installed
if ! command -v socat &> /dev/null
then
    echo "socat could not be found, please install it"
    exit
fi

trap "trap - SIGTERM && kill -- -$$" SIGINT SIGTERM EXIT

if [ "$USE_EXISTING_SOCKET" == "0" ]; then
    echo "Starting socat to forward traffic to cardano-node..."
    SOCAT_PID=""
    (
      while true; do
          if ! socat UNIX-LISTEN:node.socket,fork,reuseaddr TCP:$NODE_HOST:30000; then
            echo "socat failed with exit code $?"
            echo "lost connection to cardano-node, retrying..."
            sleep 0.5
            continue
          fi
      done
    ) &
    SOCAT_PID=$!
    echo "Socat process started with PID: $SOCAT_PID"
fi

if [ -n "$SOCAT_PID" ]; then
  sleep 0.2
  if ! kill -0 $SOCAT_PID 2>/dev/null; then
    echo "Socat process failed to start"
    exit 1
  fi
fi

# Download byron-genesis.json if it doesn't exist
if [ ! -f byron-genesis.json ]; then
  echo "Downloading byron-genesis.json"
  curl -o byron-genesis.json https://book.world.dev.cardano.org/environments/preview/byron-genesis.json
fi

# Wait for socat to start
sleep 2

# Check we can connect to the cardano-node using ./cardano-cli query tip
if ! ./cardano-cli query tip; then
  echo "Failed to connect to cardano-node"
  exit
fi

echo "Writing postgres connection string to pc-chain-config.json..."

# Add to resources files
if [ ! -f pc-chain-config.json ]; then
  echo '{}' > pc-chain-config.json
fi
(
    cat pc-chain-config.json | 
    jq '. + {
        "db_sync_postgres_connection_string": "postgresql://'$DB_SYNC_POSTGRES_USER':'$DB_SYNC_POSTGRES_PASSWORD'@'$NODE_HOST':5432/cexplorer"
    }' > tmp.json
)
mv -f tmp.json pc-chain-config.json

echo "Adding midnight ogmios instance as default..."
(
  cat pc-chain-config.json |
  jq '. + {"ogmios": {
    "hostname": "ogmios.qanet.dev.midnight.network",
    "port": 443,
    "protocol": "https"
   }}' > tmp.json
)
mv -f tmp.json pc-chain-config.json

echo "Adding '/' to PATH..."

export PATH=$PATH:/

echo "Ready."

if [ -z "$AS_INIT" ] || [ "$AS_INIT" == 0 ]; then
    sleep infinity
fi
