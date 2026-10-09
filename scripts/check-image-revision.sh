#!/usr/bin/env bash

# This file is part of midnight-node.
# Copyright (C) Midnight Foundation
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

set -euo pipefail

image="${1:?image reference required}"
revision="${2:?full Git commit SHA required}"
platform="${3:?platform required}"

if [[ ! "$revision" =~ ^[0-9a-f]{40}$ ]]; then
  echo "Expected a full Git commit SHA, got: $revision" >&2
  exit 1
fi

# Check the config, not a tag name: older commit aliases can point at a PR build.
config=$(docker buildx imagetools inspect "$image" --format '{{json .Image}}')
if ! jq -e --arg platform "$platform" --arg revision "$revision" \
  '(if has("architecture") then
      select((.os + "/" + .architecture) == $platform)
    else .[$platform] end)
    | .config.Labels["org.opencontainers.image.revision"] == $revision' \
  <<< "$config" > /dev/null; then
  echo "Image $image ($platform) was not built at $revision; run Main build/publish for that exact commit before releasing." >&2
  exit 1
fi
