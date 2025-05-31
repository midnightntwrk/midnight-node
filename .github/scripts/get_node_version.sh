#!/bin/bash

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
image_tag=$(echo $version | jq -r '.result' | awk -F '-' '{print $2}')
echo "$image_tag"
exit 0
