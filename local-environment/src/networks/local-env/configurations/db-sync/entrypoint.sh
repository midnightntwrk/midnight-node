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

# Find the schema directory and remove migration-4-* files
schema_dir=$(find /nix/store -type d -name "*-schema" -print -quit)
if [ -n "$schema_dir" ]; then
    find "$schema_dir" -name "migration-4-*" -exec rm {} \;
else
    echo "Schema directory not found."
fi

# Find the entrypoint executable, make it executable, and run it
entrypoint_executable=$(find /nix/store -type f -path "*/bin/entrypoint" -print -quit)
if [ -n "$entrypoint_executable" ]; then
    chmod +x "$entrypoint_executable"
    exec "$entrypoint_executable" --config /shared/db-sync-config.json --socket-path /node-ipc/node.socket
else
    echo "Entrypoint executable not found."
fi
