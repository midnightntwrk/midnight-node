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

echo "Installing dependencies..."
apt -qq update &> /dev/null
apt -qq -y install expect jq &> /dev/null
cp /usr/local/bin/midnight-node /data/midnight-node
cd /data


if [ -f "/shared/midnight-wizard-2.ready" ]; then
    echo "/shared/midnight-wizard-2.ready exists. Skipping configuration and starting the node..."
    expect <<EOF
spawn ./midnight-node wizards start-node
expect "Proceed? (Y/n)"
send "Y\r"
set timeout -1
expect eof
EOF
    exit 0
fi


echo "Beginning configuration..."
echo "Generating keys..."
expect <<EOF
spawn ./midnight-node wizards generate-keys
set timeout 60
expect "node base path (./data)"
send ".\r"
expect "All done!"
expect eof
EOF

cp midnight-public-keys.json /shared/midnight-public-keys.json
touch /shared/midnight-wizard-2-keys.ready


echo "Waiting for chain-spec.json and pc-chain-config.json to be ready..."
while true; do
    if [ -f "/shared/chain-spec.ready" ]; then
        break
    else
        sleep 1
    fi
done

cp /shared/chain-spec.json /data/chain-spec.json
cp /shared/pc-chain-config.json /data/pc-chain-config.json

echo "Configuring Node P2P port..."
jq '.node_p2p_port = 30334' pc-resources-config.json > tmp.json && mv tmp.json pc-resources-config.json

touch /shared/midnight-wizard-2.ready
echo "Configuration complete."

echo "Starting the node..."
expect <<EOF
spawn ./midnight-node wizards start-node
expect "DB-Sync Postgres connection string (postgresql://postgres-user:postgres-password@localhost:5432/cexplorer)"
send "postgresql://postgres:$POSTGRES_PASSWORD@postgres:$POSTGRES_PORT/cexplorer\r"
expect "Proceed? (Y/n)"
send "Y\r"
set timeout -1
expect eof
EOF
