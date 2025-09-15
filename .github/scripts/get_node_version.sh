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

# Usage: ./get_node_version.sh <rpc_url>
RPC_URL="$1"

# Check if all arguments are provided
if [[ -z "$RPC_URL" ]]; then
  echo "Usage: $0 <rpc_url>"
  exit 1
fi

# Fetch the system_version RPC response
version=$(curl -X POST $RPC_URL -H "Content-Type: application/json" -d '{ "jsonrpc": "2.0", "id": 1, "method": "system_version"}')
if [[ $? -ne 0 ]]; then
  echo "Failed to fetch system_version from node RPC at '$RPC_URL'."
  exit 1
fi

if [[ $(echo $version | jq -r '.result') = null ]]; then
  echo "Node RPC at '$RPC_URL' not responding with system_version."
  exit 1
fi

# extract the image tag from the response
image_tag=$(echo $version | jq -r '.result')
echo "$image_tag"
exit 0
