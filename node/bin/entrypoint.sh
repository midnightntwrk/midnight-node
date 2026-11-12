#!/bin/bash
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


# Default base path from Docker ENV
DEFAULT_BASE_PATH="$BASE_PATH"

# Parse arguments to find --base-path
PARSED_BASE_PATH=""
prev_arg=""
for arg in "$@"; do
    if [[ "$arg" == --base-path=* ]]; then
        # Extract value after = sign
        PARSED_BASE_PATH="${arg#*=}"
    elif [[ "$prev_arg" == "--base-path" ]]; then
        # Handle --base-path <value> format
        PARSED_BASE_PATH="$arg"
    fi
    prev_arg="$arg"
done

# Use default if not specified
if [ -z "$PARSED_BASE_PATH" ]; then
    FINAL_BASE_PATH="$DEFAULT_BASE_PATH"
else
    FINAL_BASE_PATH="$PARSED_BASE_PATH"
fi

# Create directories and set permissions if they don't exist
if [ ! -d "$FINAL_BASE_PATH" ]; then
    mkdir -p "$FINAL_BASE_PATH"
fi

# Now run as appuser
exec /midnight-node "$@"
